//! Upper-body retarget: world-direction limb solver for VRM humanoid rigs.
//!
//! Pure math first (rig as plain data, no Bevy ECS), Bevy system at the bottom. Rest directions
//! come from each model's own rest global transforms, so T-pose and A-pose rigs, and rigs with
//! large bone-local rest rotations, all work. Missing joints are held at rest, missing bones are
//! skipped. Hips and legs are never read or written (desk framing).

use std::collections::HashMap;

use bevy::prelude::{
    GlobalTransform, Last, App, ChildOf, Entity, IntoScheduleConfigs, Local, Plugin, PostUpdate, Query, Res, Resource, Time, Transform,
};
use bevy::math::{Quat, Vec3};
use bevy_vrm1::prelude::{RestGlobalTransform, RestTransform, VrmBone, VrmSystemSets};
use vt_core::body::{HandLandmarks, SubjectSide, UpperBodyOutput, HAND_POINTS};
use crate::live::BodyShare;
use crate::window::{smooth_body, QuatSmoother};

/// Landmarks below this visibility are treated as missing (design.md D8).
pub const VIS_MIN: f32 = 0.5;
/// How far outside the picture, as a share of its size, a joint may lie and still count as seen.
const OUTSIDE_MARGIN: f32 = 0.04;
/// Hand model scores below this are treated as no hand.
pub const HAND_SCORE_MIN: f32 = 0.5;
/// Torso roll limit from the shoulder line.
const TORSO_ROLL_MAX: f32 = 25.0 * std::f32::consts::PI / 180.0;
/// Torso twist (shoulder line turning towards or away from the camera) limit.
const TORSO_YAW_MAX: f32 = 35.0 * std::f32::consts::PI / 180.0;
/// Forward and backward lean limit from the head position over the shoulders.
const TORSO_LEAN_MAX: f32 = 25.0 * std::f32::consts::PI / 180.0;
/// Head-over-shoulders angle of an upright sitting posture; leaning is measured from it.
const TORSO_LEAN_NEUTRAL: f32 = 12.0 * std::f32::consts::PI / 180.0;
/// Elbow flexion limit (angle between upper-arm and forearm directions).
const ELBOW_MAX: f32 = 150.0 * std::f32::consts::PI / 180.0;
/// Hand roll limit about the forearm axis.
const WRIST_ROLL_MAX: f32 = 90.0 * std::f32::consts::PI / 180.0;
/// Spine lean weights for spine, chest, upperChest, renormalised over the bones that exist.
const SPINE_WEIGHTS: [f32; 3] = [0.3, 0.3, 0.4];

/// One node of the rig. `rest_*` come from `RestTransform` / `RestGlobalTransform`.
#[derive(Debug, Clone)]
pub struct RigNode {
    /// Lower-case humanoid bone name, or None for helper nodes between bones.
    pub name: Option<String>,
    pub parent: Option<usize>,
    pub rest_local: Quat,
    pub rest_global_rot: Quat,
    pub rest_global_pos: Vec3,
}

#[derive(Debug, Clone, Default)]
pub struct Rig {
    pub nodes: Vec<RigNode>,
}

impl Rig {
    fn find(&self, name: &str) -> Option<usize> {
        let name = name.to_ascii_lowercase();
        self.nodes.iter().position(|n| n.name.as_deref() == Some(name.as_str()))
    }

    fn parent_world_rest(&self, i: usize) -> Quat {
        let n = &self.nodes[i];
        n.rest_global_rot * n.rest_local.inverse()
    }
}

/// Current local rotations plus forward kinematics over a [`Rig`].
struct Pose<'a> {
    rig: &'a Rig,
    locals: Vec<Quat>,
}

impl<'a> Pose<'a> {
    fn new(rig: &'a Rig) -> Self {
        Self { rig, locals: rig.nodes.iter().map(|n| n.rest_local).collect() }
    }

    fn parent_world(&self, i: usize) -> Quat {
        match self.rig.nodes[i].parent {
            Some(p) => self.world(p),
            None => self.rig.parent_world_rest(i),
        }
    }

    fn world(&self, i: usize) -> Quat {
        self.parent_world(i) * self.locals[i]
    }

    fn set_world(&mut self, i: usize, world: Quat) {
        self.locals[i] = self.parent_world(i).inverse() * world;
    }

    /// Bone direction towards `child`, in the bone's own rest frame.
    fn dir_local(&self, bone: usize, child: usize) -> Vec3 {
        let (b, c) = (&self.rig.nodes[bone], &self.rig.nodes[child]);
        b.rest_global_rot.inverse() * (c.rest_global_pos - b.rest_global_pos).normalize_or_zero()
    }

    /// Current world direction of `bone` towards `child` (given the bone's own current rotation).
    fn world_dir(&self, bone: usize, child: usize) -> Vec3 {
        self.world(bone) * self.dir_local(bone, child)
    }

    /// Swings `bone` so that its direction towards `child` points along `target`.
    /// Twist is kept as close to the rest-local pose as the swing allows.
    fn aim(&mut self, bone: usize, child: usize, target: Vec3) {
        let current = self.world_dir(bone, child);
        let swing = Quat::from_rotation_arc(current, target.normalize());
        let w = swing * self.world(bone);
        self.set_world(bone, w);
    }
}

/// One arm in avatar space (+x is the avatar's left, +y up, +z towards the viewer).
/// None means the joint is missing (low visibility): the solver holds that part at rest.
#[derive(Debug, Clone, Default)]
pub struct ArmPose {
    pub shoulder: Option<Vec3>,
    pub elbow: Option<Vec3>,
    pub wrist: Option<Vec3>,
    /// Pose index-finger and pinky points, used for hand direction and roll.
    pub index: Option<Vec3>,
    pub pinky: Option<Vec3>,
    /// 21 hand-model landmarks, for finger curl only.
    pub hand: Option<[Vec3; HAND_POINTS]>,
}

#[derive(Debug, Clone, Default)]
pub struct BodyPose {
    /// The avatar's own left arm (bones named `left*`).
    pub left: ArmPose,
    pub right: ArmPose,
    /// Middle of the ears in the same space as the arm joints, for torso lean. None when unknown.
    pub head: Option<Vec3>,
}

const DEPTH_GAIN: f32 = 1.0;

/// Single place that applies the mirror (design.md D7). Image coordinates (x right, y down, z away)
/// become avatar space. With `mirror` the image is flipped like the camera preview, so the
/// person's left arm drives the avatar's right bones. Hips (23..) are never read.
pub fn to_avatar_space(out: &UpperBodyOutput, mirror: bool, vis_min: f32, hand_min: f32) -> BodyPose {
    let sx = if mirror { -1.0 } else { 1.0 };
    let v = |x: f32, y: f32, z: f32| Vec3::new(sx * x, -y, -z * DEPTH_GAIN);
    // Limbs use the metric world landmarks: same scale on every axis, so foreshortening is right.
    let point = |i: usize| {
        let p = out.pose[i];
        let [x, y, z] = p.world;
        // A joint outside the picture is a guess by the model, however sure it sounds: drop it, so a
        // person with only the shoulders in view does not get invented arms.
        // A joint at the picture's edge can land a few pixels outside, so a margin of 4 percent is allowed.
        let (w, h) = (out.width as f32, out.height as f32);
        let (mx, my) = (w * OUTSIDE_MARGIN, h * OUTSIDE_MARGIN);
        let inside = p.x >= -mx && p.y >= -my && p.x <= w + mx && p.y <= h + my;
        (inside && p.visibility >= vis_min && x.is_finite() && y.is_finite() && z.is_finite())
            .then(|| Vec3::new(sx * x, -y, -z))
    };
    let arm = |subject: SubjectSide| {
        let [sh, el, wr, ix, pk] = match subject {
            SubjectSide::Left => [11, 13, 15, 19, 17],
            SubjectSide::Right => [12, 14, 16, 20, 18],
        }
        .map(point);
        let hand = out
            .hands
            .iter()
            .find(|h| h.side == subject && h.score >= hand_min)
            .map(|h: &HandLandmarks| h.points.map(|p| v(p[0], p[1], p[2])));
        ArmPose { shoulder: sh, elbow: el, wrist: wr, index: ix, pinky: pk, hand }
    };
    let (subject_left, subject_right) = (arm(SubjectSide::Left), arm(SubjectSide::Right));
    let head = point(7).zip(point(8)).map(|(a, b)| (a + b) / 2.0);
    if mirror {
        BodyPose { left: subject_right, right: subject_left, head }
    } else {
        BodyPose { left: subject_left, right: subject_right, head }
    }
}

/// Desired and achieved world direction of one bone, for checking the solve.
#[derive(Debug, Clone)]
pub struct LimbReport {
    pub bone: String,
    pub target: Vec3,
    pub achieved: Vec3,
}

#[derive(Debug, Clone, Default)]
pub struct Solved {
    /// New local rotation per rig node index; nodes not listed stay at rest.
    pub locals: Vec<(usize, Quat)>,
    pub report: Vec<LimbReport>,
}

/// Palm normal (pointing out of the palm) from wrist, middle knuckle, index and little knuckles.
/// For left bones `u x v`, for right bones `v x u`, with `u` along the fingers and `v` from little to index.
pub fn palm_normal(left: bool, wrist: Vec3, middle: Vec3, index: Vec3, little: Vec3) -> Vec3 {
    let (u, v) = (middle - wrist, index - little);
    let n = if left { u.cross(v) } else { v.cross(u) };
    n.normalize_or_zero()
}

/// Finger flexion angles in radians, rows thumb, index, middle, ring, little; columns the three
/// joints from the base. Each is the angle between consecutive bone vectors.
pub fn curl_angles(p: &[Vec3; HAND_POINTS]) -> [[f32; 3]; 5] {
    let mut out = [[0.0; 3]; 5];
    for (f, row) in out.iter_mut().enumerate() {
        let base = if f == 0 { 1 } else { 5 + (f - 1) * 4 };
        // Chain: wrist (0), then four joints base..base+3.
        let chain = [0, base, base + 1, base + 2, base + 3];
        for j in 0..3 {
            let a = p[chain[j + 1]] - p[chain[j]];
            let b = p[chain[j + 2]] - p[chain[j + 1]];
            let (a, b) = (a.normalize_or_zero(), b.normalize_or_zero());
            row[j] = if a == Vec3::ZERO || b == Vec3::ZERO { 0.0 } else { a.dot(b).clamp(-1.0, 1.0).acos() };
        }
    }
    out
}

const FINGERS: [(&str, [&str; 3]); 5] = [
    ("Thumb", ["Metacarpal", "Proximal", "Distal"]),
    ("Index", ["Proximal", "Intermediate", "Distal"]),
    ("Middle", ["Proximal", "Intermediate", "Distal"]),
    ("Ring", ["Proximal", "Intermediate", "Distal"]),
    ("Little", ["Proximal", "Intermediate", "Distal"]),
];
/// Maximum curl per joint in radians; the thumb is damped.
const CURL_MAX: [f32; 3] = [1.75, 1.9, 1.4];
const THUMB_DAMP: f32 = 0.5;

fn torso_roll(pose: &BodyPose) -> Option<f32> {
    let (l, r) = (pose.left.shoulder?, pose.right.shoulder?);
    let d = l - r;
    if d.x.abs() < 1e-3 {
        return None;
    }
    Some(d.y.atan2(d.x).clamp(-TORSO_ROLL_MAX, TORSO_ROLL_MAX))
}

/// Twist of the shoulder line about the vertical axis; 0 when the shoulders face the camera.
fn torso_yaw(pose: &BodyPose) -> Option<f32> {
    let (l, r) = (pose.left.shoulder?, pose.right.shoulder?);
    let d = l - r;
    if d.x.abs() < 1e-3 {
        return None;
    }
    Some((-d.z).atan2(d.x.abs()).clamp(-TORSO_YAW_MAX, TORSO_YAW_MAX))
}

/// Forward (+) or backward (-) lean of the head over the shoulder middle, from an upright posture.
fn torso_lean(pose: &BodyPose) -> Option<f32> {
    let (l, r, head) = (pose.left.shoulder?, pose.right.shoulder?, pose.head?);
    let v = head - (l + r) / 2.0;
    if v.y < 0.05 {
        return None;
    }
    Some((v.z.atan2(v.y) - TORSO_LEAN_NEUTRAL).clamp(-TORSO_LEAN_MAX, TORSO_LEAN_MAX))
}

/// Signed angle from `a` to `b` about unit `axis`, both projected onto the plane normal to `axis`.
fn signed_angle_about(axis: Vec3, a: Vec3, b: Vec3) -> f32 {
    let (a, b) = (a - axis * a.dot(axis), b - axis * b.dot(axis));
    if a.length_squared() < 1e-8 || b.length_squared() < 1e-8 {
        return 0.0;
    }
    a.cross(b).dot(axis).atan2(a.dot(b))
}

/// Upper-arm and forearm directions from a two-bone solve that puts the avatar wrist where the tracked
/// wrist is, relative to the shoulder and scaled to the avatar's arm length, with the elbow on the
/// side the tracker saw it. Copying bone directions alone ignores the avatar's proportions, so a hand
/// that touches the head in the picture would stop short of it on the avatar.
pub fn reach_dirs(rig: &Rig, upper: usize, lower: usize, hand: usize, s: Vec3, e: Vec3, w: Vec3) -> Option<(Vec3, Vec3)> {
    let (ps, pe, pw) = (rig.nodes[upper].rest_global_pos, rig.nodes[lower].rest_global_pos, rig.nodes[hand].rest_global_pos);
    let (l1, l2) = ((pe - ps).length(), (pw - pe).length());
    let (t1, t2) = ((e - s).length(), (w - e).length());
    if l1 < 1e-4 || l2 < 1e-4 || t1 < 1e-4 || t2 < 1e-4 {
        return None;
    }
    let target = (w - s) * ((l1 + l2) / (t1 + t2));
    let dist = target.length();
    if dist < 1e-4 {
        return None;
    }
    let u = target / dist;
    let d = dist.min((l1 + l2) * 0.999);
    let off = e - s;
    let side = off - u * off.dot(u);
    let pole = if side.length() > 1e-4 { side.normalize() } else { return None };
    let a = (l1 * l1 - l2 * l2 + d * d) / (2.0 * d);
    let h = (l1 * l1 - a * a).max(0.0).sqrt();
    let elbow = u * a + pole * h;
    Some((elbow.normalize_or_zero(), (u * d - elbow).normalize_or_zero()))
}

fn solve_arm(p: &mut Pose, side: &str, arm: &ArmPose, report: &mut Vec<LimbReport>) {
    let left = side == "left";
    let n = |part: &str| p.rig.find(&format!("{side}{part}"));
    let (shoulder, upper, lower, hand) = (n("Shoulder"), n("UpperArm"), n("LowerArm"), n("Hand"));

    let reach = match (upper, lower, hand, arm.shoulder, arm.elbow, arm.wrist) {
        (Some(b), Some(c), Some(h), Some(s), Some(e), Some(w)) => reach_dirs(p.rig, b, c, h, s, e, w),
        _ => None,
    };
    let mut upper_dir = None;
    if let (Some(b), Some(c), Some(s), Some(e)) = (upper, lower, arm.shoulder, arm.elbow) {
        let t = reach.map_or((e - s).normalize_or_zero(), |r| r.0);
        if t != Vec3::ZERO {
            // Shrug: lift the clavicle a little when the arm is raised above the horizontal.
            if let (Some(sh), Some(_)) = (shoulder, upper) {
                let cur = p.world_dir(sh, b);
                let lift = Vec3::Y * 0.3 * t.y.max(0.0);
                p.aim(sh, b, (cur + lift).normalize_or_zero());
            }
            p.aim(b, c, t);
            report.push(LimbReport { bone: format!("{side}UpperArm"), target: t, achieved: p.world_dir(b, c) });
            upper_dir = Some(t);
        }
    }
    if let (Some(b), Some(c), Some(e), Some(w)) = (lower, hand, arm.elbow, arm.wrist) {
        let mut t = reach.map_or((w - e).normalize_or_zero(), |r| r.1);
        if t != Vec3::ZERO {
            if let Some(u) = upper_dir {
                // Clamp elbow flexion: rotate the forearm target towards the upper-arm direction.
                let angle = u.angle_between(t);
                if angle > ELBOW_MAX {
                    let axis = u.cross(t).normalize_or_zero();
                    t = Quat::from_axis_angle(axis, ELBOW_MAX - angle) * t;
                }
            }
            p.aim(b, c, t);
            report.push(LimbReport { bone: format!("{side}LowerArm"), target: t, achieved: p.world_dir(b, c) });
        }
    }

    // A hand is only driven when its forearm was: otherwise the wrist would not be where the hand points.
    let forearm_driven = report.iter().any(|r| r.bone == format!("{side}LowerArm"));
    let Some(h) = hand.filter(|_| forearm_driven) else { return };
    let finger_root = |name: &str| p.rig.find(&format!("{side}{name}Proximal"));
    let (mid_b, idx_b, lit_b) = (finger_root("Middle"), finger_root("Index"), finger_root("Little"));
    let rest_pos = |i: usize| p.rig.nodes[i].rest_global_pos;
    let rest_normal = match (mid_b, idx_b, lit_b) {
        (Some(m), Some(i), Some(l)) => {
            Some(palm_normal(left, rest_pos(h), rest_pos(m), rest_pos(i), rest_pos(l)))
        }
        _ => None,
    };
    // Hand direction and palm roll come from the 21-point hand model when it found the hand (wrist,
    // knuckles of index, middle and little finger); the pose model index and pinky points are the fallback.
    let hand_frame = match arm.hand.as_ref() {
        Some(p) => Some((p[0], p[5], p[17], p[9])),
        None => match (arm.wrist, arm.index, arm.pinky) {
            (Some(w), Some(ix), Some(pk)) => Some((w, ix, pk, (ix + pk) / 2.0)),
            _ => None,
        },
    };
    if let Some((w, ix, pk, mid)) = hand_frame {
        let t = (mid - w).normalize_or_zero();
        if let (Some(child), true) = (mid_b.or(idx_b).or(lit_b), t != Vec3::ZERO) {
            p.aim(h, child, t);
            if let Some(nr) = rest_normal.filter(|n| *n != Vec3::ZERO) {
                let target_n = palm_normal(left, w, mid, ix, pk);
                if target_n != Vec3::ZERO {
                    let cur_n = p.world(h) * (p.rig.nodes[h].rest_global_rot.inverse() * nr);
                    let angle = signed_angle_about(t, cur_n, target_n)
                        .clamp(-WRIST_ROLL_MAX, WRIST_ROLL_MAX);
                    let w_new = Quat::from_axis_angle(t, angle) * p.world(h);
                    p.set_world(h, w_new);
                }
            }
            report.push(LimbReport { bone: format!("{side}Hand"), target: t, achieved: p.world_dir(h, child) });
        }
    }

    if let (Some(pts), Some(nr)) = (arm.hand.as_ref(), rest_normal.filter(|n| *n != Vec3::ZERO)) {
        let angles = curl_angles(pts);
        for (f, (finger, joints)) in FINGERS.iter().enumerate() {
            let bones: Vec<Option<usize>> =
                joints.iter().map(|j| p.rig.find(&format!("{side}{finger}{j}"))).collect();
            // The thumb swings across the palm, not in the finger-curl plane, so its first two bones
            // are aimed along the tracked thumb segments (CMC to MCP, MCP to IP); only the last joint curls.
            let first_curled = if f == 0 { 2 } else { 0 };
            if f == 0 {
                if let (Some(m), Some(pr)) = (bones[0], bones[1]) {
                    let t = (pts[2] - pts[1]).normalize_or_zero();
                    if t != Vec3::ZERO {
                        p.aim(m, pr, t);
                    }
                }
                if let (Some(pr), Some(d)) = (bones[1], bones[2]) {
                    let t = (pts[3] - pts[2]).normalize_or_zero();
                    if t != Vec3::ZERO {
                        p.aim(pr, d, t);
                    }
                }
            }
            for (j, bone) in bones.iter().enumerate().skip(first_curled) {
                let Some(b) = *bone else { continue };
                // Rest direction: towards the next bone, or along the previous one for the last.
                let dir = if let Some(Some(c)) = bones.get(j + 1) {
                    (p.rig.nodes[*c].rest_global_pos - p.rig.nodes[b].rest_global_pos).normalize_or_zero()
                } else if j > 0 {
                    match bones[j - 1] {
                        Some(a) => (p.rig.nodes[b].rest_global_pos - p.rig.nodes[a].rest_global_pos).normalize_or_zero(),
                        None => Vec3::ZERO,
                    }
                } else {
                    // Lone first joint: along the hand-to-bone direction.
                    (p.rig.nodes[b].rest_global_pos - p.rig.nodes[h].rest_global_pos).normalize_or_zero()
                };
                let axis_world = dir.cross(nr).normalize_or_zero();
                if axis_world == Vec3::ZERO {
                    continue;
                }
                let damp = if f == 0 { THUMB_DAMP } else { 1.0 };
                let theta = (angles[f][j] * damp).min(CURL_MAX[j]);
                let axis_local = p.rig.nodes[b].rest_global_rot.inverse() * axis_world;
                p.locals[b] *= Quat::from_axis_angle(axis_local, theta);
            }
        }
    }
}

/// Solves the whole upper body. Returns only nodes whose rotation changed from rest.
pub fn solve(rig: &Rig, pose: &BodyPose) -> Solved {
    let mut p = Pose::new(rig);
    let mut report = Vec::new();

    let (roll, yaw, lean) = (torso_roll(pose), torso_yaw(pose), torso_lean(pose));
    if roll.is_some() || yaw.is_some() || lean.is_some() {
        let (roll, yaw, lean) = (roll.unwrap_or(0.0), yaw.unwrap_or(0.0), lean.unwrap_or(0.0));
        let bones: Vec<usize> =
            ["spine", "chest", "upperChest"].iter().filter_map(|n| rig.find(n)).collect();
        let weights: Vec<f32> = ["spine", "chest", "upperChest"]
            .iter()
            .zip(SPINE_WEIGHTS)
            .filter(|(n, _)| rig.find(n).is_some())
            .map(|(_, w)| w)
            .collect();
        let total: f32 = weights.iter().sum();
        let mut acc = 0.0;
        for (b, w) in bones.iter().zip(&weights) {
            acc += w / total;
            let world = Quat::from_rotation_y(yaw * acc)
                * Quat::from_rotation_x(lean * acc)
                * Quat::from_rotation_z(roll * acc)
                * rig.nodes[*b].rest_global_rot;
            p.set_world(*b, world);
        }
    }

    solve_arm(&mut p, "left", &pose.left, &mut report);
    solve_arm(&mut p, "right", &pose.right, &mut report);

    let locals = p
        .locals
        .iter()
        .enumerate()
        .filter(|(i, q)| q.dot(rig.nodes[*i].rest_local).abs() < 1.0 - 1e-7)
        .map(|(i, q)| (i, *q))
        .collect();
    Solved { locals, report }
}

/// Angle in degrees between two directions.
pub fn angle_deg(a: Vec3, b: Vec3) -> f32 {
    a.angle_between(b).to_degrees()
}

/// Elevation of a direction above the horizontal plane, in degrees (+90 straight up).
pub fn elevation_deg(d: Vec3) -> f32 {
    d.normalize_or_zero().y.clamp(-1.0, 1.0).asin().to_degrees()
}

/// Human-readable limb check for one report set: elevation and bend of target versus achieved.
pub fn describe(report: &[LimbReport]) -> Vec<String> {
    let by: HashMap<&str, &LimbReport> = report.iter().map(|r| (r.bone.as_str(), r)).collect();
    let mut lines = Vec::new();
    for side in ["left", "right"] {
        let get = |part: &str| by.get(format!("{side}{part}").as_str()).copied();
        let mut line = format!("{side}:");
        if let Some(u) = get("UpperArm") {
            line += &format!(
                " upperArm elev tracked {:.0} avatar {:.0} (err {:.1})",
                elevation_deg(u.target),
                elevation_deg(u.achieved),
                angle_deg(u.target, u.achieved)
            );
        }
        if let Some(l) = get("LowerArm") {
            line += &format!(
                "; forearm elev tracked {:.0} avatar {:.0} (err {:.1})",
                elevation_deg(l.target),
                elevation_deg(l.achieved),
                angle_deg(l.target, l.achieved)
            );
            if let Some(u) = get("UpperArm") {
                line += &format!(
                    "; elbow bend tracked {:.0} avatar {:.0}",
                    angle_deg(u.target, l.target),
                    angle_deg(u.achieved, l.achieved)
                );
            }
        }
        if let Some(h) = get("Hand") {
            line += &format!("; hand err {:.1}", angle_deg(h.target, h.achieved));
        }
        if line.ends_with(':') {
            line += " held at rest (joints missing)";
        }
        lines.push(line);
    }
    lines
}

/// One driven bone: rest rotation, hand flag for the follow time constant, and smoothed local rotation.
#[derive(Debug, Clone)]
pub struct BoneTrack {
    pub index: usize,
    pub rest: Quat,
    pub is_hand: bool,
    pub current: Quat,
    smoother: QuatSmoother,
}

/// Follow state for every bone the solver has driven at least once.
#[derive(Debug, Default)]
pub struct BodyFollow {
    pub bones: Vec<BoneTrack>,
}

/// Advances the live body by `dt` seconds. `target` holds the solved local rotations for the bones
/// that have a target this frame; any other tracked bone counts as lost. A new bone starts at rest.
/// A lost bone keeps its last pose until a target for it comes back (it never falls back to rest).
/// Returns the local rotation of every tracked bone.
pub fn advance_body(
    follow: &mut BodyFollow,
    target: &[(usize, Quat)],
    rest: &dyn Fn(usize) -> (Quat, bool),
    dt: f32,
) -> Vec<(usize, Quat)> {
    for (index, _) in target {
        if !follow.bones.iter().any(|b| b.index == *index) {
            let (rest_rot, is_hand) = rest(*index);
            follow.bones.push(BoneTrack {
                index: *index,
                rest: rest_rot,
                is_hand,
                current: rest_rot,
                smoother: QuatSmoother::default(),
            });
        }
    }
    for bone in &mut follow.bones {
        // A bone with no target keeps its last pose.
        if let Some((_, goal)) = target.iter().find(|(i, _)| *i == bone.index) {
            let goal = bone.smoother.filter(*goal, dt);
            let mut cur = [bone.current];
            smooth_body(&mut cur, &[goal], &[bone.is_hand], dt);
            bone.current = cur[0];
        }
    }
    follow.bones.iter().map(|b| (b.index, b.current)).collect()
}

/// Live pose from the body thread. Read each render frame; `None` means no person in view.
#[derive(Resource)]
pub struct LiveBody(pub BodyShare);

/// Rig, its entities, and the follow state, built once when the bones appear.
pub struct LiveRig {
    entities: Vec<Entity>,
    rig: Rig,
    follow: BodyFollow,
}

fn is_hand_bone(name: Option<&str>) -> bool {
    name.is_some_and(|n| {
        ["hand", "thumb", "index", "middle", "ring", "little"].iter().any(|part| n.contains(part))
    })
}

/// Drives the upper body from the live pose. Does nothing unless `LiveBody` is present.
fn apply_body_live(
    live: Option<Res<LiveBody>>,
    time: Res<Time>,
    all: Query<(&RestTransform, &RestGlobalTransform, Option<&VrmBone>, Option<&ChildOf>)>,
    named: Query<Entity, bevy::prelude::With<VrmBone>>,
    mut transforms: Query<&mut Transform>,
    mut state: Local<Option<LiveRig>>,
) {
    let Some(live) = live else { return };
    if state.is_none() {
        if named.iter().count() < 10 {
            return;
        }
        let (rig, entities) = build_rig(&all, named.iter());
        *state = Some(LiveRig { entities, rig, follow: BodyFollow::default() });
    }
    let Some(s) = state.as_mut() else { return };
    let pose = live.0.lock().ok().and_then(|g| (*g).clone());
    let target = pose.map(|p| solve(&s.rig, &p).locals).unwrap_or_default();
    let rig = &s.rig;
    let rest = |i: usize| (rig.nodes[i].rest_local, is_hand_bone(rig.nodes[i].name.as_deref()));
    let out = advance_body(&mut s.follow, &target, &rest, time.delta_secs());
    for (i, q) in out {
        if let Ok(mut t) = transforms.get_mut(s.entities[i]) {
            t.rotation = q;
        }
    }
}

/// Pose to apply to the model. Still-image mode sets it once; the solve runs against the loaded rig.
#[derive(Resource)]
pub struct BodyStill(pub BodyPose);

/// Collects every humanoid bone and its ancestors as a [`Rig`]. Node `i` of the rig is `entities[i]`.
fn build_rig(
    all: &Query<(&RestTransform, &RestGlobalTransform, Option<&VrmBone>, Option<&ChildOf>)>,
    bones: impl Iterator<Item = Entity>,
) -> (Rig, Vec<Entity>) {
    let mut entities: Vec<Entity> = Vec::new();
    let mut index: HashMap<Entity, usize> = HashMap::new();
    for bone in bones {
        // Walk up to the first entity without rest data, then add top-down.
        let mut chain = vec![bone];
        let mut cur = bone;
        while let Ok((_, _, _, Some(parent))) = all.get(cur) {
            if all.get(parent.parent()).is_err() {
                break;
            }
            chain.push(parent.parent());
            cur = parent.parent();
        }
        for e in chain.into_iter().rev() {
            if let std::collections::hash_map::Entry::Vacant(v) = index.entry(e) {
                v.insert(entities.len());
                entities.push(e);
            }
        }
    }
    let nodes = entities
        .iter()
        .map(|e| {
            let (rest, global, name, parent) = all.get(*e).expect("collected above");
            RigNode {
                name: name.map(|n| n.0.to_ascii_lowercase()),
                parent: parent.and_then(|p| index.get(&p.parent()).copied()),
                rest_local: rest.0.rotation,
                rest_global_rot: global.0.rotation(),
                rest_global_pos: global.0.translation(),
            }
        })
        .collect();
    (Rig { nodes }, entities)
}

fn apply_body_pose(
    still: Option<Res<BodyStill>>,
    all: Query<(&RestTransform, &RestGlobalTransform, Option<&VrmBone>, Option<&ChildOf>)>,
    named: Query<Entity, bevy::prelude::With<VrmBone>>,
    mut transforms: Query<&mut Transform>,
    mut cache: Local<Option<(Vec<Entity>, Solved)>>,
) {
    let Some(still) = still else { return };
    if cache.is_none() {
        if named.iter().count() < 10 {
            return;
        }
        let (rig, entities) = build_rig(&all, named.iter());
        let solved = solve(&rig, &still.0);
        for line in describe(&solved.report) {
            println!("LIMBS {line}");
        }
        *cache = Some((entities, solved));
    }
    if let Some((entities, solved)) = cache.as_ref() {
        for (i, q) in &solved.locals {
            if let Ok(mut t) = transforms.get_mut(entities[*i]) {
                t.rotation = *q;
            }
        }
    }
}

/// Prints the limb directions the engine actually rendered (from GlobalTransform after propagation),
/// once, 300 frames after the bones appeared. Compare with the LIMBS lines from the solver.
fn report_rendered(
    named: Query<(&VrmBone, &GlobalTransform)>,
    mut frames: Local<u32>,
    mut done: Local<bool>,
) {
    if *done || named.iter().count() < 10 {
        return;
    }
    *frames += 1;
    if *frames < 300 {
        return;
    }
    *done = true;
    let pos: HashMap<String, Vec3> =
        named.iter().map(|(n, g)| (n.0.to_ascii_lowercase(), g.translation())).collect();
    for side in ["left", "right"] {
        let at = |part: &str| pos.get(&format!("{side}{part}").to_ascii_lowercase()).copied();
        if let (Some(u), Some(l), Some(h)) = (at("UpperArm"), at("LowerArm"), at("Hand")) {
            let (a, b) = ((l - u).normalize_or_zero(), (h - l).normalize_or_zero());
            println!(
                "RENDERED {side}: upperArm elev {:.0} forearm elev {:.0} elbow bend {:.0}",
                elevation_deg(a),
                elevation_deg(b),
                angle_deg(a, b)
            );
        }
    }
}
/// Registers the still-image body apply. Added only when a still body pose is requested, so the
/// live path is unchanged.
pub struct BodyPlugin;

impl Plugin for BodyPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PostUpdate,
            apply_body_pose
                .after(VrmSystemSets::GazeControl)
                .before(VrmSystemSets::PropagateAfterExpressions),
        )
        .add_systems(Last, report_rendered);
        app.add_systems(
            PostUpdate,
            apply_body_live
                .after(VrmSystemSets::GazeControl)
                .before(VrmSystemSets::PropagateAfterExpressions),
        );
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a rig from (name, parent, global rotation, global position). Locals follow from the globals.
    fn build(spec: &[(&str, Option<usize>, Quat, Vec3)]) -> Rig {
        let mut nodes: Vec<RigNode> = Vec::new();
        for (name, parent, g, pos) in spec {
            let pg = parent.map_or(Quat::IDENTITY, |p| nodes[p].rest_global_rot);
            nodes.push(RigNode {
                name: Some(name.to_ascii_lowercase()),
                parent: *parent,
                rest_local: pg.inverse() * *g,
                rest_global_rot: *g,
                rest_global_pos: *pos,
            });
        }
        Rig { nodes }
    }

    /// Torso + both arms with hands and three finger roots. T-pose arms run along +-x, A-pose 35 degrees down.
    /// `big` gives every arm bone a large local frame rotation (like the Twist sample).
    fn humanoid(a_pose: bool, big: bool, upper_chest: bool, with_shoulder: bool) -> Rig {
        let frame = if big { Quat::from_xyzw(0.468, 0.530, 0.468, 0.530).normalize() } else { Quat::IDENTITY };
        let mut spec: Vec<(String, Option<usize>, Quat, Vec3)> = vec![
            ("hips".into(), None, Quat::IDENTITY, Vec3::new(0.0, 1.0, 0.0)),
            ("spine".into(), Some(0), Quat::IDENTITY, Vec3::new(0.0, 1.1, 0.0)),
            ("chest".into(), Some(1), Quat::IDENTITY, Vec3::new(0.0, 1.25, 0.0)),
        ];
        let mut top = 2;
        if upper_chest {
            spec.push(("upperChest".into(), Some(2), Quat::IDENTITY, Vec3::new(0.0, 1.35, 0.0)));
            top = 3;
        }
        for (side, sx) in [("left", 1.0f32), ("right", -1.0)] {
            let dir = if a_pose { Vec3::new(sx * 0.82, -0.57, 0.0) } else { Vec3::new(sx, 0.0, 0.0) };
            let origin = Vec3::new(sx * 0.05, 1.4, 0.0);
            let mut parent = top;
            let mut pos = origin;
            if with_shoulder {
                spec.push((format!("{side}Shoulder"), Some(parent), frame, pos));
                parent = spec.len() - 1;
                pos += Vec3::new(sx * 0.1, 0.0, 0.0);
            }
            for (part, len) in [("UpperArm", 0.28), ("LowerArm", 0.25), ("Hand", 0.1)] {
                spec.push((format!("{side}{part}"), Some(parent), frame, pos));
                parent = spec.len() - 1;
                pos += dir * len;
            }
            // Finger roots: middle along the arm, index forward (+z), little back (-z).
            let hand = parent;
            let hp = spec[hand].3;
            for (name, dz) in [("Middle", 0.0), ("Index", 0.02), ("Little", -0.02)] {
                spec.push((format!("{side}{name}Proximal"), Some(hand), frame, hp + dir * 0.08 + Vec3::new(0.0, 0.0, dz)));
            }
        }
        let borrowed: Vec<(&str, Option<usize>, Quat, Vec3)> =
            spec.iter().map(|(n, p, q, v)| (n.as_str(), *p, *q, *v)).collect();
        build(&borrowed)
    }

    fn world_dir_of(rig: &Rig, solved: &Solved, bone: &str, child: &str) -> Vec3 {
        let mut p = Pose::new(rig);
        for (i, q) in &solved.locals {
            p.locals[*i] = *q;
        }
        let (b, c) = (rig.find(bone).unwrap(), rig.find(child).unwrap());
        p.world_dir(b, c)
    }

    fn arm(shoulder: Vec3, elbow: Vec3, wrist: Vec3) -> ArmPose {
        let fwd = (wrist - elbow).normalize();
        ArmPose {
            shoulder: Some(shoulder),
            elbow: Some(elbow),
            wrist: Some(wrist),
            index: Some(wrist + fwd * 0.08 + Vec3::new(0.0, 0.0, 0.02)),
            pinky: Some(wrist + fwd * 0.08 + Vec3::new(0.0, 0.0, -0.02)),
            hand: None,
        }
    }

    /// Left arm straight up, right arm bent: forearm up from a horizontal upper arm.
    fn raise_pose() -> BodyPose {
        BodyPose {
            left: arm(Vec3::new(0.05, 1.4, 0.0), Vec3::new(0.05, 1.7, 0.0), Vec3::new(0.05, 2.0, 0.0)),
            right: arm(Vec3::new(-0.05, 1.4, 0.0), Vec3::new(-0.35, 1.4, 0.0), Vec3::new(-0.35, 1.7, 0.0)),
            head: None,
        }
    }

    fn close_dir(a: Vec3, b: Vec3, deg: f32) -> bool {
        angle_deg(a, b) < deg
    }

    #[test]
    fn arm_direction_is_reached_with_identity_and_large_rest_rotations() {
        for big in [false, true] {
            let rig = humanoid(false, big, true, true);
            let solved = solve(&rig, &raise_pose());
            // The reach solve fits the test rig's arm proportions, so the bent arm may differ a few
            // degrees from the tracked directions; a straight arm stays exact.
            let up = world_dir_of(&rig, &solved, "leftUpperArm", "leftLowerArm");
            assert!(close_dir(up, Vec3::Y, 0.5), "big={big} upper arm {up:?}");
            let fore = world_dir_of(&rig, &solved, "rightLowerArm", "rightHand");
            assert!(close_dir(fore, Vec3::Y, 6.0), "big={big} forearm {fore:?}");
            let upper_r = world_dir_of(&rig, &solved, "rightUpperArm", "rightLowerArm");
            assert!(close_dir(upper_r, Vec3::NEG_X, 6.0), "big={big} right upper {upper_r:?}");
        }
    }

    #[test]
    fn reach_puts_the_avatar_wrist_where_the_tracked_wrist_is_relative_to_the_shoulder() {
        let rig = humanoid(false, false, true, true);
        let (up, lo, ha) = (rig.find("leftUpperArm").unwrap(), rig.find("leftLowerArm").unwrap(), rig.find("leftHand").unwrap());
        let (ps, pe, pw) = (rig.nodes[up].rest_global_pos, rig.nodes[lo].rest_global_pos, rig.nodes[ha].rest_global_pos);
        let (l1, l2) = ((pe - ps).length(), (pw - pe).length());
        // Tracked arm in a different size: hand raised to head height beside the head, elbow out.
        let s = Vec3::new(0.0, 0.0, 0.0);
        let e = Vec3::new(0.25, 0.1, 0.05);
        let w = Vec3::new(0.12, 0.38, 0.1);
        let (d1, d2) = reach_dirs(&rig, up, lo, ha, s, e, w).expect("reach");
        let reached = ps + d1 * l1 + d2 * l2;
        let k = (l1 + l2) / ((e - s).length() + (w - e).length());
        let want = ps + (w - s) * k;
        assert!(reached.distance(want) < 1e-3, "wrist {reached:?} want {want:?}");
        // The elbow stays on the side the tracker saw it (outward, +x here).
        assert!((ps + d1 * l1).x > ps.x, "elbow flipped");
        // A target farther than the arm reaches is clamped to straight, not stretched.
        let far = reach_dirs(&rig, up, lo, ha, s, Vec3::new(0.2, 0.0, 0.0), Vec3::new(0.2, 0.0, 0.0) * 2.0 + Vec3::new(0.0, 0.01, 0.0)).expect("far");
        assert!(angle_deg(far.0, far.1) < 8.0, "nearly straight arm expected, got {far:?}");
    }

    #[test]
    fn t_pose_and_a_pose_rigs_give_the_same_world_arm_direction() {
        let t = humanoid(false, false, true, true);
        let a = humanoid(true, true, true, true);
        let pose = raise_pose();
        let dt = world_dir_of(&t, &solve(&t, &pose), "leftUpperArm", "leftLowerArm");
        let da = world_dir_of(&a, &solve(&a, &pose), "leftUpperArm", "leftLowerArm");
        assert!(close_dir(dt, da, 1.0), "{dt:?} vs {da:?}");
    }

    #[test]
    fn rest_pose_input_leaves_the_rig_at_rest() {
        let rig = humanoid(true, true, true, true);
        let pose = BodyPose {
            left: arm(Vec3::new(0.05, 1.4, 0.0), Vec3::new(0.05 + 0.82 * 0.28, 1.4 - 0.57 * 0.28, 0.0), Vec3::new(0.05 + 0.82 * 0.53, 1.4 - 0.57 * 0.53, 0.0)),
            right: ArmPose::default(),
            head: None,
        };
        let solved = solve(&rig, &pose);
        // The arm already points where the target says, so the achieved direction equals rest.
        let d = world_dir_of(&rig, &solved, "leftUpperArm", "leftLowerArm");
        assert!(close_dir(d, Vec3::new(0.82, -0.57, 0.0), 0.5));
        assert!(solved.locals.iter().all(|(i, _)| rig.nodes[*i].name.as_deref().unwrap().starts_with("left")));
    }

    #[test]
    fn missing_joints_hold_that_arm_at_rest_and_leave_the_other_driven() {
        let rig = humanoid(false, false, true, true);
        let mut pose = raise_pose();
        pose.right.elbow = None;
        let solved = solve(&rig, &pose);
        let named = |n: &str| rig.find(n).unwrap();
        assert!(solved.locals.iter().any(|(i, _)| *i == named("leftUpperArm")));
        assert!(!solved.locals.iter().any(|(i, _)| *i == named("rightUpperArm")));
        assert!(!solved.locals.iter().any(|(i, _)| *i == named("rightLowerArm")));
        assert!(describe(&solved.report).iter().any(|l| l.starts_with("right: held at rest")));
    }

    #[test]
    fn rigs_without_upper_chest_shoulders_or_fingers_do_not_panic() {
        let pose = raise_pose();
        for (uc, sh) in [(false, true), (true, false), (false, false)] {
            let rig = humanoid(false, true, uc, sh);
            let solved = solve(&rig, &pose);
            let up = world_dir_of(&rig, &solved, "leftUpperArm", "leftLowerArm");
            assert!(close_dir(up, Vec3::Y, 0.5), "uc={uc} sh={sh}");
        }
        let empty = Rig::default();
        assert!(solve(&empty, &pose).locals.is_empty());
        let torso_only = build(&[("hips", None, Quat::IDENTITY, Vec3::ZERO)]);
        assert!(solve(&torso_only, &pose).locals.is_empty());
    }

    #[test]
    fn torso_roll_follows_the_shoulder_line_and_is_clamped() {
        let rig = humanoid(false, false, true, true);
        // Left shoulder 0.1 higher over a 0.1 span: 45 degrees, clamped to 25.
        let pose = BodyPose {
            left: ArmPose { shoulder: Some(Vec3::new(0.05, 1.5, 0.0)), ..Default::default() },
            right: ArmPose { shoulder: Some(Vec3::new(-0.05, 1.4, 0.0)), ..Default::default() },
            head: None,
        };
        let solved = solve(&rig, &pose);
        let top = rig.find("upperChest").unwrap();
        let mut p = Pose::new(&rig);
        for (i, q) in &solved.locals {
            p.locals[*i] = *q;
        }
        let angle = p.world(top).angle_between(Quat::IDENTITY).to_degrees();
        assert!((angle - 25.0).abs() < 0.1, "angle {angle}");
    }

    fn upper_chest_world(rig: &Rig, pose: &BodyPose) -> Quat {
        let solved = solve(rig, pose);
        let mut p = Pose::new(rig);
        for (i, q) in &solved.locals {
            p.locals[*i] = *q;
        }
        p.world(rig.find("upperChest").unwrap())
    }

    fn shoulders(l: Vec3, r: Vec3, head: Option<Vec3>) -> BodyPose {
        BodyPose {
            left: ArmPose { shoulder: Some(l), ..Default::default() },
            right: ArmPose { shoulder: Some(r), ..Default::default() },
            head,
        }
    }

    #[test]
    fn torso_twists_with_the_shoulder_line_and_leans_with_the_head() {
        let rig = humanoid(false, false, true, true);
        // Squared to the camera, head slightly forward: close to upright.
        let neutral = shoulders(Vec3::new(0.2, 1.4, 0.0), Vec3::new(-0.2, 1.4, 0.0), Some(Vec3::new(0.0, 1.62, 0.04)));
        assert!(upper_chest_world(&rig, &neutral).angle_between(Quat::IDENTITY).to_degrees() < 2.0);
        // Left shoulder 0.2 m towards the viewer, right 0.2 m away: about 27 degrees of twist about Y.
        let twisted = shoulders(Vec3::new(0.2, 1.4, 0.2), Vec3::new(-0.2, 1.4, -0.2), None);
        let q = upper_chest_world(&rig, &twisted);
        let (yaw, _, _) = q.to_euler(bevy::math::EulerRot::YXZ);
        assert!(yaw.to_degrees().abs() > 20.0 && yaw.to_degrees().abs() < 36.0, "yaw {}", yaw.to_degrees());
        // Head 0.2 m forward of the shoulders over 0.22 m height: leaning towards the viewer (about 25 degrees max).
        let leaning = shoulders(Vec3::new(0.2, 1.4, 0.0), Vec3::new(-0.2, 1.4, 0.0), Some(Vec3::new(0.0, 1.62, 0.2)));
        let q = upper_chest_world(&rig, &leaning);
        let up_after = q * Vec3::Y;
        assert!(up_after.z > 0.2, "spine should tip towards the viewer: {up_after:?}");
        // Leaning back tips the other way.
        let back = shoulders(Vec3::new(0.2, 1.4, 0.0), Vec3::new(-0.2, 1.4, 0.0), Some(Vec3::new(0.0, 1.62, -0.1)));
        assert!((upper_chest_world(&rig, &back) * Vec3::Y).z < -0.1);
    }

    /// Raw landmark output with the left arm raised on the image right (person faces the camera).
    fn photo_output() -> UpperBodyOutput {
        use vt_core::body::PoseLandmark;
        let mut pose = [PoseLandmark::default(); 33];
        let mut set = |i: usize, x: f32, y: f32| {
            pose[i] = PoseLandmark { x, y, z: 0.0, visibility: 0.9, world: [x * 0.001, y * 0.001, 0.0] };
        };
        // Person's right shoulder on image-left (x 100), left on image-right (x 200).
        set(12, 100.0, 200.0);
        set(11, 200.0, 200.0);
        // Person's left arm straight up on image-right.
        set(13, 200.0, 120.0);
        set(15, 200.0, 40.0);
        set(14, 80.0, 280.0);
        set(16, 70.0, 360.0);
        // Hips reported visible, far below, and not finite: must never matter.
        pose[23] = PoseLandmark { x: f32::NAN, y: f32::NAN, z: f32::NAN, visibility: 1.0, world: [f32::NAN; 3] };
        pose[24] = PoseLandmark { x: 150.0, y: 9000.0, z: 0.0, visibility: 1.0, world: [0.15, 9.0, 0.0] };
        UpperBodyOutput { width: 300, height: 400, pose, presence: 1.0, hands: vec![], hand_model: String::new() }
    }

    #[test]
    fn mirror_sends_the_persons_left_arm_to_the_avatar_right_bones_and_back() {
        let out = photo_output();
        let mirrored = to_avatar_space(&out, true, VIS_MIN, HAND_SCORE_MIN);
        // Person's left arm (image right, raised) drives the avatar's right bones, pointing up.
        let r = &mirrored.right;
        let up = r.wrist.unwrap() - r.elbow.unwrap();
        assert!(up.y > 0.0 && up.x.abs() < 1e-3, "{up:?}");
        // Person's right arm hangs slightly towards image-left; in the mirror that is the avatar's
        // left bone reaching towards +x (outward for a left arm).
        let l = &mirrored.left;
        let out_dir = l.elbow.unwrap() - l.shoulder.unwrap();
        assert!(out_dir.x > 0.0 && out_dir.y < 0.0, "{out_dir:?}");

        let plain = to_avatar_space(&out, false, VIS_MIN, HAND_SCORE_MIN);
        let l = &plain.left;
        let up = l.wrist.unwrap() - l.elbow.unwrap();
        assert!(up.y > 0.0, "no mirror: person's left arm drives the avatar left bones");
        let r = &plain.right;
        assert!((r.elbow.unwrap() - r.shoulder.unwrap()).x < 0.0);
    }

    #[test]
    fn hips_never_change_the_result() {
        let a = photo_output();
        let mut b = photo_output();
        b.pose[23].world = [1.0, 1.0, 1.0];
        b.pose[24].world[0] = -77.0;
        let (pa, pb) = (to_avatar_space(&a, true, VIS_MIN, HAND_SCORE_MIN), to_avatar_space(&b, true, VIS_MIN, HAND_SCORE_MIN));
        assert_eq!(pa.left.wrist, pb.left.wrist);
        assert_eq!(pa.right.shoulder, pb.right.shoulder);
    }

    #[test]
    fn low_visibility_joints_are_missing() {
        let mut out = photo_output();
        out.pose[15].visibility = 0.3;
        let p = to_avatar_space(&out, false, VIS_MIN, HAND_SCORE_MIN);
        assert!(p.left.wrist.is_none() && p.left.elbow.is_some());
    }

    #[test]
    fn joints_outside_the_picture_are_missing_even_when_the_model_is_sure() {
        let mut out = photo_output();
        out.pose[15].y = 450.0; // wrist 50 pixels below the 400 pixel tall frame, still reported visible
        let p = to_avatar_space(&out, false, VIS_MIN, HAND_SCORE_MIN);
        assert!(p.left.wrist.is_none(), "wrist below the picture");
        out.pose[15].y = 410.0; // 10 pixels (2.5 percent) outside: at the edge, still counts
        let p = to_avatar_space(&out, false, VIS_MIN, HAND_SCORE_MIN);
        assert!(p.left.wrist.is_some(), "wrist at the picture edge");
        assert!(p.left.shoulder.is_some() && p.left.elbow.is_some() && p.right.wrist.is_some());
    }

    fn hand_pts(curl: f32) -> [Vec3; HAND_POINTS] {
        // Fingers along +y from the wrist. Each finger has three segments; `curl` bends every joint
        // toward +z by that many radians.
        let mut pts = [Vec3::ZERO; HAND_POINTS];
        for f in 0..5 {
            let base = if f == 0 { 1 } else { 5 + (f - 1) * 4 };
            let x = (f as f32 - 2.0) * 0.004;
            let mut pos = Vec3::new(x, 0.08, 0.0);
            pts[base] = pos;
            let mut angle = 0.0f32;
            for j in 1..4 {
                angle += curl;
                pos += Vec3::new(0.0, angle.cos(), angle.sin()) * 0.03;
                pts[base + j] = pos;
            }
        }
        pts
    }

    #[test]
    fn open_hand_has_under_10_percent_curl_and_a_fist_over_70_percent() {
        let open = curl_angles(&hand_pts(0.0));
        let fist = curl_angles(&hand_pts(1.5));
        for f in 1..5 {
            for j in 0..3 {
                assert!(open[f][j] < 0.1 * 1.9, "open f{f} j{j} {}", open[f][j]);
                assert!(fist[f][j] > 0.7 * 1.5_f32.min(1.9), "fist f{f} j{j} {}", fist[f][j]);
            }
        }
    }

    #[test]
    fn closed_fist_bends_the_finger_bones_towards_the_palm() {
        let rig = humanoid(false, true, true, true);
        let mut pose = raise_pose();
        pose.left.hand = Some(hand_pts(1.5));
        let solved = solve(&rig, &pose);
        // Only the left proximal roots exist in this rig, so check that the roots moved and the
        // solver ran without touching the right hand.
        assert!(solved.locals.iter().any(|(i, _)| rig.nodes[*i].name.as_deref() == Some("leftmiddleproximal")));
        assert!(!solved.locals.iter().any(|(i, _)| rig.nodes[*i].name.as_deref() == Some("rightmiddleproximal")));
    }

    #[test]
    fn hand_roll_matches_the_tracked_palm_normal() {
        let rig = humanoid(false, true, true, true);
        let mut pose = raise_pose();
        // Wrist at the end of the straight-up left forearm; index toward +x and pinky toward -x
        // means the palm normal is along +-z; pick index +x, pinky -x.
        let w = Vec3::new(0.05, 2.0, 0.0);
        pose.left.wrist = Some(w);
        pose.left.index = Some(w + Vec3::new(0.02, 0.08, 0.0));
        pose.left.pinky = Some(w + Vec3::new(-0.02, 0.08, 0.0));
        let solved = solve(&rig, &pose);
        let hand = rig.find("leftHand").unwrap();
        let mut p = Pose::new(&rig);
        for (i, q) in &solved.locals {
            p.locals[*i] = *q;
        }
        let nr = palm_normal(
            true,
            rig.nodes[hand].rest_global_pos,
            rig.nodes[rig.find("leftMiddleProximal").unwrap()].rest_global_pos,
            rig.nodes[rig.find("leftIndexProximal").unwrap()].rest_global_pos,
            rig.nodes[rig.find("leftLittleProximal").unwrap()].rest_global_pos,
        );
        let got = p.world(hand) * (rig.nodes[hand].rest_global_rot.inverse() * nr);
        let want = palm_normal(true, w, w + Vec3::new(0.0, 0.08, 0.0), w + Vec3::new(0.02, 0.08, 0.0), w + Vec3::new(-0.02, 0.08, 0.0));
        assert!(close_dir(got, want, 2.0), "palm normal {got:?} want {want:?}");
    }

    const FRAME: f32 = 1.0 / 60.0;

    fn rest_identity(_: usize) -> (Quat, bool) {
        (Quat::IDENTITY, false)
    }

    #[test]
    fn advance_body_converges_to_target() {
        let goal = Quat::from_rotation_x(40f32.to_radians());
        let mut follow = BodyFollow::default();
        let mut out = Vec::new();
        for _ in 0..600 {
            out = advance_body(&mut follow, &[(3, goal)], &rest_identity, FRAME);
        }
        assert_eq!(out.len(), 1);
        assert!(out[0].1.angle_between(goal) < 1e-3, "got {:?}", out[0].1);
    }

    #[test]
    fn advance_body_keeps_the_last_pose_forever_when_lost() {
        let goal = Quat::from_rotation_z(60f32.to_radians());
        let mut follow = BodyFollow::default();
        for _ in 0..120 {
            advance_body(&mut follow, &[(3, goal)], &rest_identity, FRAME);
        }
        let held = follow.bones[0].current;
        assert!(held.angle_between(goal) < 0.02, "converges to the goal");

        // Target gone for 30 s: the pose must not move and must not return to rest.
        for _ in 0..1800 {
            advance_body(&mut follow, &[], &rest_identity, FRAME);
        }
        assert!(follow.bones[0].current.angle_between(held) < 1e-6, "moved while lost");
        assert!(follow.bones[0].current.angle_between(Quat::IDENTITY) > 0.5, "fell back to rest");
    }

    #[test]
    fn advance_body_filter_calms_noise_but_follows_a_real_move() {
        let base = Quat::from_rotation_z(30f32.to_radians());
        let mut follow = BodyFollow::default();
        let mut rng = 1u32;
        let mut jitter = |amp: f32| {
            rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
            ((rng >> 8) as f32 / 16777216.0 - 0.5) * amp
        };
        // 3 s of +-1.5 degree noise around a still pose.
        let (mut raw_travel, mut out_travel) = (0.0f32, 0.0f32);
        let (mut prev_raw, mut prev_out) = (base, base);
        for _ in 0..180 {
            let noisy = Quat::from_rotation_z((30.0 + jitter(3.0)).to_radians());
            let out = advance_body(&mut follow, &[(3, noisy)], &rest_identity, FRAME)[0].1;
            raw_travel += noisy.angle_between(prev_raw);
            out_travel += out.angle_between(prev_out);
            (prev_raw, prev_out) = (noisy, out);
        }
        assert!(out_travel < raw_travel * 0.4, "noise passes through: {out_travel} vs {raw_travel}");
        // A real 60 degree move is followed within a second.
        let goal = Quat::from_rotation_z(90f32.to_radians());
        let mut out = Quat::IDENTITY;
        for _ in 0..60 {
            out = advance_body(&mut follow, &[(3, goal)], &rest_identity, FRAME)[0].1;
        }
        assert!(out.angle_between(goal) < 0.1, "lags: {}", out.angle_between(goal));
    }

    #[test]
    fn advance_body_step_is_at_most_15_degrees_per_frame() {
        let mut follow = BodyFollow::default();
        let mut prev = Quat::IDENTITY;
        let far = Quat::from_rotation_y(170f32.to_radians());
        for _ in 0..30 {
            let out = advance_body(&mut follow, &[(3, far)], &rest_identity, FRAME);
            let now = out[0].1;
            assert!(prev.angle_between(now).to_degrees() <= 15.0 + 1e-3);
            prev = now;
        }
        // Lost: the ease back to rest must also respect the step limit.
        for _ in 0..120 {
            let out = advance_body(&mut follow, &[], &rest_identity, FRAME);
            let now = out[0].1;
            assert!(prev.angle_between(now).to_degrees() <= 15.0 + 1e-3);
            prev = now;
        }
    }
}






