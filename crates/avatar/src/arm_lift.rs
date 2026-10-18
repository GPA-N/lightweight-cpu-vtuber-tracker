//! 2D to 3D arm lift for 2D-only limb models (CIGPose-m, OpenSpec task 11.5).
//!
//! Lives in `avatar` because `ArmPose` / `BodyPose` (with Bevy `Vec3`) live here and `infer` does
//! not depend on `avatar`; the input is a plain `[Kp; 17]` the caller fills from the limb stage.
//!
//! Method: the shoulder width in pixels is the unit. Each arm bone has a fixed 3D length in that
//! unit (anthropometric prior, grown from the largest projected length seen). The depth
//! magnitude of a bone is `sqrt(max(0, L^2 - l2d^2))` (foreshortening); its sign comes from
//! continuity with the previous frame plus a prior that the hands are toward the camera.
//! Output is a [`BodyPose`] in avatar space exactly like `to_avatar_space` (+x avatar left, +y up,
//! +z toward the viewer), in shoulder-width units, so `solve` is unchanged. Hips are never read.

pub use bevy::math::Vec3;

use crate::body::{ArmPose, BodyPose};

/// Upper arm length (shoulder to elbow) in shoulder widths. Drillis and Contini segment ratios:
/// upper arm 0.186 H, forearm 0.146 H, biacromial width about 0.259 H, with COCO shoulder
/// keypoints (joint centres) a little wider than the acromia; 0.75 and 0.62 are the values the
/// task fixed.
pub const UPPER_ARM_RATIO: f32 = 0.75;
/// Forearm length (elbow to wrist) in shoulder widths.
pub const FOREARM_RATIO: f32 = 0.62;
/// Learned length is clamped to this band around the prior.
const LEN_MIN: f32 = 0.7;
const LEN_MAX: f32 = 1.4;
/// Rate at which a longer projected length raises the length estimate.
const LEN_UP_ALPHA: f32 = 0.5;
/// Smoothing of the shoulder-width scale.
const SCALE_ALPHA: f32 = 0.2;
/// Degrees of extra cost for a bone pointing away from the camera (the forward prior).
const AWAY_PENALTY_DEG: f32 = 8.0;
/// Cost per degree of disagreement with the optional hand-model forearm hint.
const HINT_WEIGHT: f32 = 0.5;
/// Keypoint scores below this are treated as missing (person-box.md: found joints 0.49 to 0.85).
pub const MIN_SCORE: f32 = 0.45;

pub const L_SHOULDER: usize = 5;
pub const R_SHOULDER: usize = 6;
pub const L_ELBOW: usize = 7;
pub const R_ELBOW: usize = 8;
pub const L_WRIST: usize = 9;
pub const R_WRIST: usize = 10;

/// One 2D keypoint in frame pixels (x right, y down) with the limb model's score.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Kp {
    pub x: f32,
    pub y: f32,
    pub score: f32,
}

/// Optional hand-model forearm direction (unit, avatar space, elbow to wrist), per avatar side.
/// Future use: when the hand stage runs it can settle the forearm sign. Not required.
#[derive(Debug, Clone, Copy, Default)]
pub struct LiftHints {
    pub left_forearm: Option<Vec3>,
    pub right_forearm: Option<Vec3>,
}

/// 2D scores of the joints behind one arm (0 when missing).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ArmConfidence {
    pub shoulder: f32,
    pub elbow: f32,
    pub wrist: f32,
}

#[derive(Debug, Clone, Default)]
pub struct Lifted {
    pub pose: BodyPose,
    pub left: ArmConfidence,
    pub right: ArmConfidence,
}

#[derive(Debug, Clone, Copy)]
struct BoneState {
    prior: f32,
    len: f32,
    prev: Option<Vec3>,
}

#[derive(Debug, Clone)]
struct ArmState {
    upper: BoneState,
    fore: BoneState,
}

/// Stateful lifter: scale, learned bone lengths and last bone directions. The mirror flag is
/// fixed for the lifter's lifetime because the stored directions are in avatar space.
#[derive(Debug, Clone)]
pub struct ArmLifter {
    mirror: bool,
    scale: Option<f32>,
    /// Indexed by subject side: 0 person's left, 1 person's right.
    arms: [ArmState; 2],
}

impl ArmLifter {
    pub fn new(mirror: bool) -> Self {
        let arm = || ArmState {
            upper: BoneState { prior: UPPER_ARM_RATIO, len: UPPER_ARM_RATIO, prev: None },
            fore: BoneState { prior: FOREARM_RATIO, len: FOREARM_RATIO, prev: None },
        };
        Self { mirror, scale: None, arms: [arm(), arm()] }
    }

    /// Lifts one frame of 2D keypoints. Joints under [`MIN_SCORE`] are missing; a wrist needs its
    /// elbow and an elbow its shoulder. Without both shoulders the last scale is reused, and with
    /// no scale yet the pose is empty.
    pub fn lift(&mut self, kp: &[Kp; 17], hints: LiftHints) -> Lifted {
        let ok = |i: usize| kp[i].score >= MIN_SCORE && kp[i].x.is_finite() && kp[i].y.is_finite();
        if ok(L_SHOULDER) && ok(R_SHOULDER) {
            let w = (kp[L_SHOULDER].x - kp[R_SHOULDER].x).hypot(kp[L_SHOULDER].y - kp[R_SHOULDER].y);
            if w > 1.0 {
                self.scale = Some(self.scale.map_or(w, |s| s + SCALE_ALPHA * (w - s)));
            }
        }
        let Some(scale) = self.scale else { return Lifted::default() };
        let sx = if self.mirror { -1.0 } else { 1.0 };
        let conf = |i: usize| if ok(i) { kp[i].score } else { 0.0 };

        let mut arms = [ArmPose::default(), ArmPose::default()];
        let mut confs = [ArmConfidence::default(); 2];
        for (side, [sh, el, wr]) in [[L_SHOULDER, L_ELBOW, L_WRIST], [R_SHOULDER, R_ELBOW, R_WRIST]].into_iter().enumerate() {
            confs[side] = ArmConfidence { shoulder: conf(sh), elbow: conf(el), wrist: conf(wr) };
            if !ok(sh) {
                continue;
            }
            // Avatar side of this subject side; hints are given per avatar side.
            let avatar_left = (side == 0) != self.mirror;
            let hint = if avatar_left { hints.left_forearm } else { hints.right_forearm };
            let to_a = |a: usize, b: usize| Vec3::new(sx * (kp[b].x - kp[a].x), -(kp[b].y - kp[a].y), 0.0) / scale;
            let shoulder = Vec3::new(sx * kp[sh].x, -kp[sh].y, 0.0) / scale;
            let state = &mut self.arms[side];
            let mut arm = ArmPose { shoulder: Some(shoulder), ..Default::default() };
            if ok(el) {
                // A hand nearer or farther than the shoulders says the whole arm went that way, so the
                // forearm hint's depth sign also settles the upper arm.
                let upper_d = to_a(sh, el);
                let upper_hint = hint.map(|h| {
                    Vec3::new(upper_d.x, upper_d.y, h.z.signum() * 0.7 * upper_d.truncate().length().max(0.05)).normalize()
                });
                let elbow = shoulder + lift_bone(&mut state.upper, upper_d, upper_hint);
                arm.elbow = Some(elbow);
                if ok(wr) {
                    arm.wrist = Some(elbow + lift_bone(&mut state.fore, to_a(el, wr), hint));
                }
            }
            arms[side] = arm;
        }
        let [subject_left, subject_right] = arms;
        let [cl, cr] = confs;
        if self.mirror {
            Lifted { pose: BodyPose { left: subject_right, right: subject_left, head: None }, left: cr, right: cl }
        } else {
            Lifted { pose: BodyPose { left: subject_left, right: subject_right, head: None }, left: cl, right: cr }
        }
    }
}

/// Lifts one bone. `d` is the projected vector in shoulder widths with avatar axes (z unused).
/// Learns the bone length, takes the depth magnitude from foreshortening and picks its sign.
fn lift_bone(state: &mut BoneState, d: Vec3, hint: Option<Vec3>) -> Vec3 {
    let l2d = d.truncate().length();
    if l2d > state.len {
        state.len += LEN_UP_ALPHA * (l2d - state.len);
    }
    state.len = state.len.clamp(state.prior * LEN_MIN, state.prior * LEN_MAX);
    let mag = (state.len * state.len - l2d * l2d).max(0.0).sqrt();
    let (pos, neg) = (Vec3::new(d.x, d.y, mag), Vec3::new(d.x, d.y, -mag));
    let cost = |c: Vec3, away: bool| {
        if c.length() < 1e-6 {
            return 0.0;
        }
        // A hint is a fresh measurement every frame (hand size), so it alone decides; continuity
        // would otherwise hold the old sign (live take 2026-10-18: 65 backward hints, none obeyed).
        if let Some(h) = hint {
            return HINT_WEIGHT * h.angle_between(c).to_degrees();
        }
        let mut k = if away { AWAY_PENALTY_DEG } else { 0.0 };
        if let Some(p) = state.prev {
            k += p.angle_between(c).to_degrees();
        }
        k
    };
    let pick = if mag > 0.0 && cost(neg, true) < cost(pos, false) { neg } else { pos };
    if pick.length() > 1e-6 {
        state.prev = Some(pick.normalize());
    }
    pick
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::math::Quat;

    /// Small deterministic generator so the error table is reproducible.
    struct Rng(u64);
    impl Rng {
        fn f(&mut self) -> f32 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((self.0 >> 40) as f32) / ((1u64 << 24) as f32)
        }
        fn normal(&mut self) -> f32 {
            (0..4).map(|_| self.f()).sum::<f32>() - 2.0
        }
        /// Uniform direction within `max_deg` of `axis`.
        fn cone(&mut self, axis: Vec3, max_deg: f32) -> Vec3 {
            let cmax = max_deg.to_radians().cos();
            let c = cmax + (1.0 - cmax) * self.f();
            let s = (1.0 - c * c).max(0.0).sqrt();
            let phi = std::f32::consts::TAU * self.f();
            let a = axis.normalize();
            let helper = if a.x.abs() < 0.9 { Vec3::X } else { Vec3::Y };
            let u = a.cross(helper).normalize();
            let v = a.cross(u);
            a * c + (u * phi.cos() + v * phi.sin()) * s
        }
    }

    const S: f32 = 100.0; // pixels per shoulder width
    const CX: f32 = 320.0;
    const CY: f32 = 240.0;
    const REST: Vec3 = Vec3::new(0.0, -1.0, 0.0);

    fn project(p: Vec3, persp: Option<f32>) -> (f32, f32) {
        let m = persp.map_or(1.0, |d| d / (d - p.z));
        (CX + S * p.x * m, CY - S * p.y * m)
    }

    struct Arm3 {
        shoulder: Vec3,
        elbow: Vec3,
        wrist: Vec3,
    }

    fn arm3(shoulder: Vec3, u: Vec3, f: Vec3, lu: f32, lf: f32) -> Arm3 {
        let elbow = shoulder + u * lu;
        Arm3 { shoulder, elbow, wrist: elbow + f * lf }
    }

    fn frame(l: &Arm3, r: &Arm3, persp: Option<f32>, noise: Option<&mut Rng>, sigma: f32) -> [Kp; 17] {
        let mut kp = [Kp::default(); 17];
        let mut noise = noise;
        let mut put = |i: usize, p: Vec3| {
            let (mut x, mut y) = project(p, persp);
            if let Some(n) = noise.as_deref_mut() {
                x += sigma * n.normal();
                y += sigma * n.normal();
            }
            kp[i] = Kp { x, y, score: 0.8 };
        };
        put(L_SHOULDER, l.shoulder);
        put(L_ELBOW, l.elbow);
        put(L_WRIST, l.wrist);
        put(R_SHOULDER, r.shoulder);
        put(R_ELBOW, r.elbow);
        put(R_WRIST, r.wrist);
        kp
    }

    fn deg(a: Vec3, b: Vec3) -> f32 {
        a.normalize().angle_between(b.normalize()).to_degrees()
    }

    #[test]
    fn straight_in_plane_arm_is_recovered_exactly() {
        // Both bones lie in the image plane: no depth to recover.
        let l = arm3(Vec3::new(0.5, 0.0, 0.0), Vec3::X, Vec3::X, UPPER_ARM_RATIO, FOREARM_RATIO);
        let r = arm3(Vec3::new(-0.5, 0.0, 0.0), -Vec3::X, -Vec3::X, UPPER_ARM_RATIO, FOREARM_RATIO);
        let mut lifter = ArmLifter::new(false);
        let out = lifter.lift(&frame(&l, &r, None, None, 0.0), LiftHints::default());
        let a = &out.pose.left;
        let (s, e, w) = (a.shoulder.unwrap(), a.elbow.unwrap(), a.wrist.unwrap());
        assert!(deg(e - s, Vec3::X) < 0.5, "{:?}", e - s);
        assert!(deg(w - e, Vec3::X) < 0.5);
        assert!(out.pose.right.wrist.is_some());
    }

    #[test]
    fn foreshortened_forward_arm_gets_toward_camera_depth() {
        // Upper arm 60 degrees out of the image plane, toward the camera.
        let u = Vec3::new(0.5, -0.0, 0.866).normalize();
        let l = arm3(Vec3::new(0.5, 0.0, 0.0), u, u, UPPER_ARM_RATIO, FOREARM_RATIO);
        let r = arm3(Vec3::new(-0.5, 0.0, 0.0), REST, REST, UPPER_ARM_RATIO, FOREARM_RATIO);
        let mut lifter = ArmLifter::new(false);
        let out = lifter.lift(&frame(&l, &r, None, None, 0.0), LiftHints::default());
        let a = &out.pose.left;
        assert!(deg(a.elbow.unwrap() - a.shoulder.unwrap(), u) < 1.0);
        assert!(a.elbow.unwrap().z > 0.0, "z is toward the viewer");
    }

    #[test]
    fn low_score_joints_are_missing_and_confidence_reports_scores() {
        let l = arm3(Vec3::new(0.5, 0.0, 0.0), Vec3::X, Vec3::X, UPPER_ARM_RATIO, FOREARM_RATIO);
        let r = arm3(Vec3::new(-0.5, 0.0, 0.0), REST, REST, UPPER_ARM_RATIO, FOREARM_RATIO);
        let mut kp = frame(&l, &r, None, None, 0.0);
        kp[L_ELBOW].score = 0.2;
        kp[L_WRIST].score = 0.9;
        let out = ArmLifter::new(false).lift(&kp, LiftHints::default());
        assert!(out.pose.left.shoulder.is_some());
        assert!(out.pose.left.elbow.is_none() && out.pose.left.wrist.is_none(), "no wrist without an elbow");
        assert_eq!(out.left.elbow, 0.0);
        assert_eq!(out.left.shoulder, 0.8);
        assert!(out.pose.right.wrist.is_some());
    }

    #[test]
    fn missing_shoulder_before_any_scale_gives_an_empty_pose() {
        let mut kp = [Kp { x: 1.0, y: 1.0, score: 0.9 }; 17];
        kp[R_SHOULDER].score = 0.1;
        let out = ArmLifter::new(false).lift(&kp, LiftHints::default());
        assert!(out.pose.left.shoulder.is_none() && out.pose.right.wrist.is_none());
    }

    #[test]
    fn mirror_swaps_arms_and_negates_x() {
        let u = Vec3::new(0.3, -0.5, 0.6).normalize();
        let l = arm3(Vec3::new(0.5, 0.0, 0.0), u, u, UPPER_ARM_RATIO, FOREARM_RATIO);
        let r = arm3(Vec3::new(-0.5, 0.0, 0.0), REST, REST, UPPER_ARM_RATIO, FOREARM_RATIO);
        let kp = frame(&l, &r, None, None, 0.0);
        let plain = ArmLifter::new(false).lift(&kp, LiftHints::default()).pose;
        let mir = ArmLifter::new(true).lift(&kp, LiftHints::default()).pose;
        let (pw, mw) = (plain.left.wrist.unwrap(), mir.right.wrist.unwrap());
        let (ps, ms) = (plain.left.shoulder.unwrap(), mir.right.shoulder.unwrap());
        let (a, b) = (pw - ps, mw - ms);
        assert!((a.x + b.x).abs() < 1e-4 && (a.y - b.y).abs() < 1e-4 && (a.z - b.z).abs() < 1e-4);
    }

    #[test]
    fn sign_follows_the_previous_frame_not_the_prior_once_established() {
        // Arm clearly behind the torso plane for many frames: the lift must keep it behind.
        let l0 = |z: f32| {
            let u = Vec3::new(0.6, -0.3, z).normalize();
            arm3(Vec3::new(0.5, 0.0, 0.0), u, u, UPPER_ARM_RATIO, FOREARM_RATIO)
        };
        let r = arm3(Vec3::new(-0.5, 0.0, 0.0), REST, REST, UPPER_ARM_RATIO, FOREARM_RATIO);
        let mut lifter = ArmLifter::new(false);
        // First frame in front, then the arm swings behind via a path whose depth stays small
        // enough that the previous sign wins until it is clearly the other side.
        let mut last = Vec3::ZERO;
        for z in [0.8, 0.6, 0.4, 0.2, 0.1, 0.0, -0.1, -0.2, -0.4, -0.6, -0.8] {
            let out = lifter.lift(&frame(&l0(z), &r, None, None, 0.0), LiftHints::default());
            let a = &out.pose.left;
            last = a.elbow.unwrap() - a.shoulder.unwrap();
        }
        // Through-plane crossing is ambiguous in 2D alone: the lifter stays on the forward branch.
        assert!(last.z > 0.0, "continuity keeps the first branch: {last:?}");
    }

    #[test]
    fn forearm_hint_settles_the_sign() {
        let u = Vec3::new(0.0, -1.0, 0.0);
        let f = Vec3::new(0.4, 0.0, -0.9).normalize(); // forearm points away from camera
        let l = arm3(Vec3::new(0.5, 0.0, 0.0), u, f, UPPER_ARM_RATIO, FOREARM_RATIO);
        let r = arm3(Vec3::new(-0.5, 0.0, 0.0), REST, REST, UPPER_ARM_RATIO, FOREARM_RATIO);
        let kp = frame(&l, &r, None, None, 0.0);
        let without = ArmLifter::new(false).lift(&kp, LiftHints::default()).pose.left;
        let hints = LiftHints { left_forearm: Some(f), right_forearm: None };
        let with = ArmLifter::new(false).lift(&kp, hints).pose.left;
        let dir = |a: &ArmPose| a.wrist.unwrap() - a.elbow.unwrap();
        assert!(dir(&without).z > 0.0, "prior picks toward camera");
        assert!(deg(dir(&with), f) < 2.0, "hint picks the away branch");
    }

    #[test]
    fn hint_overrides_an_established_forward_sign() {
        let u = Vec3::new(0.0, -0.8, 0.6); // upper arm tilted toward the camera
        let fwd = Vec3::new(0.4, 0.0, 0.9).normalize();
        let back = Vec3::new(0.4, 0.0, -0.9).normalize();
        let r = arm3(Vec3::new(-0.5, 0.0, 0.0), REST, REST, UPPER_ARM_RATIO, FOREARM_RATIO);
        let mut lifter = ArmLifter::new(false);
        // 20 frames forward, no hint: the forward sign is established.
        let l = arm3(Vec3::new(0.5, 0.0, 0.0), u, fwd, UPPER_ARM_RATIO, FOREARM_RATIO);
        for _ in 0..20 {
            lifter.lift(&frame(&l, &r, None, None, 0.0), LiftHints::default());
        }
        // Same 2D pose, but the hand now says backward: the sign must follow the hint at once.
        let hints = LiftHints { left_forearm: Some(back), right_forearm: None };
        let a = lifter.lift(&frame(&l, &r, None, None, 0.0), hints).pose.left;
        assert!((a.wrist.unwrap() - a.elbow.unwrap()).z < 0.0, "forearm follows the backward hint");
        assert!((a.elbow.unwrap() - a.shoulder.unwrap()).z < 0.0, "upper arm follows it too");
    }

    // ---- Error table over random smooth sequences ----

    #[derive(Clone, Copy, PartialEq, Debug)]
    enum Class {
        Forward,
        Backward,
        Crossing,
    }

    /// Depth within 0.1 bone lengths of the image plane (about 6 degrees) counts as neither side.
    fn class_of(zs: &[f32]) -> Class {
        let (pos, neg) = (zs.iter().any(|z| *z > 0.1), zs.iter().any(|z| *z < -0.1));
        match (pos, neg) {
            (_, false) => Class::Forward,
            (false, true) => Class::Backward,
            _ => Class::Crossing,
        }
    }

    struct Cfg {
        persp: Option<f32>,
        sigma: f32,
        len_scale: f32,
        flex_max: f32,
    }

    /// (class, per-frame errors of both bones in degrees, per-sequence median error).
    fn simulate(seqs: usize, seed: u64, cfg: &Cfg) -> Vec<(Class, Vec<f32>)> {
        let mut rng = Rng(seed);
        let mut out = Vec::new();
        for _ in 0..seqs {
            let mut dirs = Vec::new(); // per arm: (u0,u1,f0,f1)
            for _ in 0..2 {
                let (u0, u1) = (rng.cone(REST, 150.0), rng.cone(REST, 150.0));
                let (f0, f1) = (rng.cone(u0, cfg.flex_max), rng.cone(u1, cfg.flex_max));
                dirs.push((u0, u1, f0, f1));
            }
            let (lu, lf) = (UPPER_ARM_RATIO * cfg.len_scale, FOREARM_RATIO * cfg.len_scale);
            let mut lifter = ArmLifter::new(false);
            let mut errs = [Vec::new(), Vec::new()];
            let mut zs = [Vec::new(), Vec::new()];
            let mut noise = Rng(seed ^ 0x9e37_79b9);
            for t in 0..30 {
                let s = t as f32 / 29.0;
                let s = s * s * (3.0 - 2.0 * s);
                let truth: Vec<(Vec3, Vec3)> = dirs
                    .iter()
                    .map(|(u0, u1, f0, f1)| {
                        let qu = Quat::IDENTITY.slerp(Quat::from_rotation_arc(*u0, *u1), s);
                        let qf = Quat::IDENTITY.slerp(Quat::from_rotation_arc(*f0, *f1), s);
                        (qu * *u0, qf * *f0)
                    })
                    .collect();
                let l = arm3(Vec3::new(0.5, 0.0, 0.0), truth[0].0, truth[0].1, lu, lf);
                let r = arm3(Vec3::new(-0.5, 0.0, 0.0), truth[1].0, truth[1].1, lu, lf);
                let n = if cfg.sigma > 0.0 { Some(&mut noise) } else { None };
                let kp = frame(&l, &r, cfg.persp, n, cfg.sigma);
                let got = lifter.lift(&kp, LiftHints::default()).pose;
                for (i, (arm, (u, f))) in [(&got.left, truth[0]), (&got.right, truth[1])].into_iter().enumerate() {
                    let (s_, e, w) = (arm.shoulder.unwrap(), arm.elbow.unwrap(), arm.wrist.unwrap());
                    errs[i].push(deg(e - s_, u));
                    errs[i].push(deg(w - e, f));
                    zs[i].push(u.z);
                    zs[i].push(f.z);
                }
            }
            for i in 0..2 {
                out.push((class_of(&zs[i]), std::mem::take(&mut errs[i])));
            }
        }
        out
    }

    fn pct(v: &mut [f32], p: f32) -> f32 {
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        v[((v.len() - 1) as f32 * p) as usize]
    }

    /// Prints one table row and returns (median, p95) of the class.
    fn row(label: &str, runs: &[(Class, Vec<f32>)], class: Option<Class>) -> (f32, f32) {
        let sel: Vec<&(Class, Vec<f32>)> = runs.iter().filter(|r| class.is_none_or(|c| r.0 == c)).collect();
        let mut all: Vec<f32> = sel.iter().flat_map(|r| r.1.iter().copied()).collect();
        let frames_bad = all.iter().filter(|e| **e > 15.0).count() as f32 / all.len() as f32;
        let seqs_bad = sel.iter().filter(|r| {
            let mut v = r.1.clone();
            pct(&mut v, 0.5) > 15.0
        }).count() as f32 / sel.len() as f32;
        let (m, p) = (pct(&mut all, 0.5), pct(&mut all, 0.95));
        println!(
            "{label:<34} n_seq {:>5} median {m:6.2} p95 {p:6.2} frames>15deg {:5.1}% seqs(median>15deg) {:5.1}%",
            sel.len(), frames_bad * 100.0, seqs_bad * 100.0
        );
        (m, p)
    }

    const CLEAN: Cfg = Cfg { persp: None, sigma: 0.0, len_scale: 1.0, flex_max: 150.0 };

    #[test]
    fn forward_sequences_stay_within_15_degrees_orthographic_and_mild_perspective() {
        for (name, persp) in [("ortho", None), ("persp D=10", Some(10.0))] {
            let cfg = Cfg { persp, ..CLEAN };
            let runs = simulate(1500, 7, &cfg);
            for c in [Class::Forward, Class::Backward, Class::Crossing] {
                row(&format!("{name} {c:?}"), &runs, Some(c));
            }
            row(&format!("{name} all"), &runs, None);
            let (m, p) = row(&format!("{name} GATE Forward"), &runs, Some(Class::Forward));
            assert!(m <= 15.0 && p <= 15.0, "{name}: median {m} p95 {p}");
        }
    }

    #[test]
    fn robustness_rows_for_noise_and_length_error() {
        for (label, cfg) in [
            ("noise 1px", Cfg { sigma: 1.0, ..CLEAN }),
            ("noise 2px", Cfg { sigma: 2.0, ..CLEAN }),
            ("arms 10% longer", Cfg { len_scale: 1.1, ..CLEAN }),
            ("arms 10% shorter", Cfg { len_scale: 0.9, ..CLEAN }),
        ] {
            let runs = simulate(800, 11, &cfg);
            row(&format!("{label} Forward"), &runs, Some(Class::Forward));
        }
    }

    #[test]
    fn arm_pointing_at_the_camera_error_by_angle_from_axis() {
        let r = arm3(Vec3::new(-0.5, 0.0, 0.0), REST, REST, UPPER_ARM_RATIO, FOREARM_RATIO);
        for theta in [0.0f32, 3.0, 6.0, 10.0, 20.0, 40.0] {
            let mut worst = 0.0f32;
            for k in 0..12 {
                let az = k as f32 * 30.0f32.to_radians();
                let t = theta.to_radians();
                let u = Vec3::new(t.sin() * az.cos(), t.sin() * az.sin(), t.cos());
                let l = arm3(Vec3::new(0.5, 0.0, 0.0), u, u, UPPER_ARM_RATIO, FOREARM_RATIO);
                let out = ArmLifter::new(false).lift(&frame(&l, &r, None, None, 0.0), LiftHints::default());
                let a = &out.pose.left;
                worst = worst.max(deg(a.elbow.unwrap() - a.shoulder.unwrap(), u));
            }
            println!("axis theta {theta:>4} deg: worst upper-arm error {worst:.2} deg");
        }
    }
}
