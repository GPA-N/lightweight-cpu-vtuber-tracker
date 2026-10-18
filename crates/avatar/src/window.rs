//! Window that loads a VRM 1.0 file from a filesystem path and shows it.
//!
//! Bevy's asset server only reads under `AssetPlugin::file_path` (default `assets/`) and
//! forbids paths outside it. So the asset root is set to the VRM file's parent directory
//! and the file is loaded by its bare file name.

use std::any::Any;
use std::collections::HashMap;
use std::fmt;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use bevy::asset::{AssetPlugin, UnapprovedPathMode};
use bevy::camera::primitives::Aabb;
use bevy::prelude::*;
use bevy::window::{PrimaryWindow, WindowPlugin};
use bevy_vrm1::prelude::*;
use vt_core::frame::TrackingOutput;

use crate::body::{BodyPlugin, BodyPose, BodyStill, LiveBody};
use crate::errors::{describe_graphics_failure, AvatarError};
use crate::vrm0::{open_model, OpenedModel};
use crate::expressions::{ExpressionPlugin, ExpressionTarget};
use crate::head::{quat_from_tracking, HeadPlugin, HeadTarget};
use crate::hud::{HudPlugin, Toast, ToastLevel};
use crate::lighting::{LightRole, LightingPlugin, LightingPreset};
use crate::live::{LiveConfig, PreviewShare, RenderSamples};
use crate::settings::{LiveTuning, Settings, SettingsRes};
use crate::settings_apply::{LiveTuningRes, SettingsApplyPlugin};
use crate::settings_ui::SettingsUiPlugin;
use crate::map_apply;
use crate::map_table::{self, MapError, MapTable};
use crate::render_report::{query_dxgi_memory, DxgiMemory, FpsMeter, RenderReportPlugin};

/// Latest tracking output. The tracking thread writes it; the app thread takes it each frame.
pub type TrackingShare = Arc<Mutex<Option<TrackingOutput>>>;

/// Inputs for one avatar window run.
pub struct AvatarConfig {
    /// The model to open at start. An empty path starts with no model and waits for a drop or Ctrl+O.
    pub vrm_path: PathBuf,
    pub map_path: PathBuf,
    pub tracking: TrackingShare,
    /// Live mode: camera preview beside the avatar, a HUD in the window title, a time limit.
    pub live: Option<LiveConfig>,
}

/// Figures reported after the window closes.
#[derive(Debug, Clone, PartialEq)]
pub struct RunSummary {
    pub ticks: u64,
    pub average_fps: Option<f64>,
    pub memory: Option<DxgiMemory>,
}

/// Why the window run failed. Each variant is reported before or instead of the window.
#[derive(Debug)]
pub enum RunError {
    /// The VRM file is missing or not VRM 1.0. No Bevy App was built.
    Avatar(AvatarError),
    /// The mapping table could not be loaded. No Bevy App was built.
    Map(MapError),
    /// The graphics adapter could not be created during the run.
    Graphics(AvatarError),
    /// Any other panic during the run.
    Panic(String),
}

impl fmt::Display for RunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RunError::Avatar(e) | RunError::Graphics(e) => write!(f, "{e}"),
            RunError::Map(e) => write!(f, "{e}"),
            RunError::Panic(msg) => write!(f, "avatar window panicked: {msg}"),
        }
    }
}

impl std::error::Error for RunError {}

/// True when a panic message comes from a missing graphics adapter.
pub fn graphics_panic_message(msg: &str) -> bool {
    msg.contains("GPU") || msg.contains("adapter")
}

fn payload_text(payload: &(dyn Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        s.to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic payload".to_string()
    }
}

/// Converts one tracking pass to a head rotation and expression weights.
pub fn tracking_to_targets(out: &TrackingOutput, map: &MapTable) -> (Quat, HashMap<String, f32>) {
    let c = out.head_rotation.coords.as_slice();
    targets_from_parts([c[0], c[1], c[2], c[3]], &out.weights, map)
}

fn targets_from_parts(
    coords: [f32; 4],
    weights: &[f32; 52],
    map: &MapTable,
) -> (Quat, HashMap<String, f32>) {
    (quat_from_tracking(coords), map_apply::apply(map, weights))
}

#[derive(Resource)]
struct SharedSlot(TrackingShare);

#[derive(Resource)]
struct MappingRes(MapTable);

/// Latest tracker output in avatar terms. The tracker updates at camera rate (12 to 30 per
/// second), so the avatar follows it with `smooth_targets` instead of jumping on each update.
#[derive(Resource, Default)]
struct RawTarget {
    active: bool,
    head: Quat,
    expressions: HashMap<String, f32>,
    brows: crate::brows::BrowInput,
}

/// Time constant of the render-rate follow. Short enough not to add visible lag.
const SMOOTH_TAU_SECONDS: f32 = 0.03;

/// Fraction of the remaining distance to cover this frame (exponential follow).
pub fn follow_factor(dt_seconds: f32, tau_seconds: f32) -> f32 {
    if dt_seconds <= 0.0 {
        0.0
    } else {
        1.0 - (-dt_seconds / tau_seconds).exp()
    }
}

/// Moves `current` towards `target` by `factor`; snaps when the remaining gap is negligible.
pub fn follow_weight(current: f32, target: f32, factor: f32) -> f32 {
    let next = current + (target - current) * factor;
    if (target - next).abs() < 1e-3 {
        target
    } else {
        next
    }
}

/// Follow time constant for hands; they track fast so gestures stay crisp.
const HAND_SMOOTH_TAU_SECONDS: f32 = 0.03;
/// Follow time constant for the torso; it moves slower so the spine does not jitter.
const TORSO_SMOOTH_TAU_SECONDS: f32 = 0.08;
/// Largest rotation a bone may take in one frame, in degrees.
const MAX_BODY_STEP_DEG: f32 = 15.0;

/// One Euro filter on a rotation (component-wise on the quaternion, sign-aligned): heavy smoothing
/// while the bone is nearly still, light smoothing while it moves, so tracker noise stops
/// scribbling the arms but gestures stay quick.
const BODY_FILTER_MIN_CUTOFF_HZ: f32 = 1.2;
const BODY_FILTER_BETA: f32 = 8.0;
const BODY_FILTER_DERIV_CUTOFF_HZ: f32 = 1.0;

#[derive(Debug, Clone, Copy, Default)]
pub struct QuatSmoother {
    out: Option<Quat>,
    prev: Quat,
    dx: bevy::math::Vec4,
}

fn lowpass_alpha(cutoff_hz: f32, dt: f32) -> f32 {
    let tau = 1.0 / (std::f32::consts::TAU * cutoff_hz);
    1.0 / (1.0 + tau / dt)
}

impl QuatSmoother {
    pub fn filter(&mut self, q: Quat, dt: f32) -> Quat {
        use bevy::math::Vec4;
        let Some(out) = self.out else {
            self.out = Some(q);
            self.prev = q;
            return q;
        };
        if dt <= 0.0 {
            return out;
        }
        let q = if q.dot(out) < 0.0 { -q } else { q };
        let dx = (Vec4::from(q) - Vec4::from(self.prev)) / dt;
        self.prev = q;
        self.dx += (dx - self.dx) * lowpass_alpha(BODY_FILTER_DERIV_CUTOFF_HZ, dt);
        let cutoff = BODY_FILTER_MIN_CUTOFF_HZ + BODY_FILTER_BETA * self.dx.length();
        let o = Vec4::from(out) + (Vec4::from(q) - Vec4::from(out)) * lowpass_alpha(cutoff, dt);
        let next = Quat::from_vec4(o).normalize();
        self.out = Some(next);
        next
    }
}

/// Eases each solved bone rotation towards its target. Hands use a shorter time constant than the
/// torso. No bone moves more than `MAX_BODY_STEP_DEG` in one call.
pub fn smooth_body(current: &mut [Quat], target: &[Quat], is_hand: &[bool], dt_seconds: f32) {
    let max_step = MAX_BODY_STEP_DEG.to_radians();
    for ((cur, goal), hand) in current.iter_mut().zip(target).zip(is_hand) {
        let tau = if *hand { HAND_SMOOTH_TAU_SECONDS } else { TORSO_SMOOTH_TAU_SECONDS };
        let factor = follow_factor(dt_seconds, tau);
        let mut next = cur.slerp(*goal, factor);
        let step = cur.angle_between(next);
        if step > max_step {
            next = cur.slerp(next, max_step / step);
        }
        *cur = next;
    }
}

/// Feeds the tracked eyebrow shapes to the direct brow morph control (see `brows.rs`).
fn update_brows(raw: Res<RawTarget>, mut brows: ResMut<crate::brows::BrowInput>, time: Res<Time>) {
    if !raw.active {
        return;
    }
    let goal = raw.brows;
    let k = follow_factor(time.delta_secs(), 0.05);
    brows.down_left += (goal.down_left - brows.down_left) * k;
    brows.down_right += (goal.down_right - brows.down_right) * k;
    brows.up_left += (goal.up_left - brows.up_left) * k;
    brows.up_right += (goal.up_right - brows.up_right) * k;
    brows.wide += (goal.wide - brows.wide) * k;
    brows.funnel += (goal.funnel - brows.funnel) * k;
    brows.pucker += (goal.pucker - brows.pucker) * k;
    brows.lip_up += (goal.lip_up - brows.lip_up) * k;
    brows.lip_down += (goal.lip_down - brows.lip_down) * k;
}

/// Feeds the tracked look expressions to the bone-type gaze target (see `gaze.rs`).
fn update_gaze(raw: Res<RawTarget>, mut gaze: ResMut<crate::gaze::GazeInput>, time: Res<Time>) {
    if !raw.active {
        return;
    }
    let goal = crate::gaze::gaze_from_expressions(|n| raw.expressions.get(n).copied().unwrap_or(0.0));
    let k = follow_factor(time.delta_secs(), 0.06);
    gaze.x += (goal.x - gaze.x) * k;
    gaze.y += (goal.y - gaze.y) * k;
}

fn apply_tracking(
    slot: Res<SharedSlot>,
    mapping: Res<MappingRes>,
    mut raw: ResMut<RawTarget>,
    live: Option<Res<LiveRes>>,
) {
    let Ok(mut guard) = slot.0.try_lock() else {
        return;
    };
    let taken = guard.take();
    drop(guard);
    let Some(out) = taken else {
        return;
    };
    if let Some(live) = live
        && let Ok(mut samples) = live.samples.lock()
    {
        samples.push_handoff(out.ready.elapsed().as_secs_f32() * 1000.0);
    }
    let (rotation, weights) = tracking_to_targets(&out, &mapping.0);
    raw.active = true;
    raw.head = rotation;
    raw.expressions = weights;
    raw.brows = crate::brows::brows_from_weights(|n| vt_core::names::index_of(n).map_or(0.0, |i| out.weights[i]));
}

fn smooth_targets(
    time: Res<Time>,
    raw: Res<RawTarget>,
    mut head: ResMut<HeadTarget>,
    mut expressions: ResMut<ExpressionTarget>,
) {
    if !raw.active {
        return;
    }
    let factor = follow_factor(time.delta_secs(), SMOOTH_TAU_SECONDS);
    let rotation = head.0.slerp(raw.head, factor);
    if head.0 != rotation {
        head.0 = rotation;
    }
    let mut next = HashMap::new();
    for (name, target) in &raw.expressions {
        let current = expressions.0.get(name).copied().unwrap_or(0.0);
        next.insert(name.clone(), follow_weight(current, *target, factor));
    }
    if expressions.0 != next {
        expressions.0 = next;
    }
}

/// Debug harness: with AVATAR_SHOT=path set, waits until the model has loaded plus a settling
/// period, saves a screenshot, and exits.
fn shot_and_exit(
    mut commands: Commands,
    mut frames: Local<u32>,
    mut loaded_at: Local<Option<u32>>,
    models: Query<(), With<ExpressionEntityMap>>,
    mut exit: MessageWriter<AppExit>,
) {
    let Some(path) = std::env::var_os("AVATAR_SHOT") else {
        return;
    };
    *frames += 1;
    if loaded_at.is_none() && !models.is_empty() {
        *loaded_at = Some(*frames);
    }
    let Some(start) = *loaded_at else {
        // The model never appeared; give up instead of hanging.
        if *frames > 3000 {
            exit.write(AppExit::error());
        }
        return;
    };
    if *frames == start + 400 {
        commands
            .spawn(bevy::render::view::screenshot::Screenshot::primary_window())
            .observe(bevy::render::view::screenshot::save_to_disk(PathBuf::from(path)));
    } else if *frames == start + 460 {
        exit.write(AppExit::Success);
    }
}

/// Debug harness: with APP_REPLAY=dir, plays dir/track.txt at its recorded timestamps on a fixed
/// 30 frames per second virtual clock, the way the live app sees it (each tracker output is held
/// until the next one arrives, then followed by `smooth_targets`). Saves dir/av_NNNN.png for
/// every frame, then exits.
/// One recorded tracker output: timestamp in seconds, head quaternion, 52 weights.
type ReplayRow = (f32, [f32; 4], [f32; 52]);

#[derive(Default)]
struct ReplayState {
    data: Option<Vec<ReplayRow>>,
    frames: u32,
    ready_at: Option<u32>,
}

const REPLAY_FPS: f32 = 30.0;
const REPLAY_SETTLE_FRAMES: u32 = 150;

fn replay(
    mut st: Local<ReplayState>,
    mapping: Res<MappingRes>,
    mut raw: ResMut<RawTarget>,
    models: Query<(), With<ExpressionEntityMap>>,
    mut commands: Commands,
    mut exit: MessageWriter<AppExit>,
) {
    let Some(dir) = std::env::var_os("APP_REPLAY") else {
        return;
    };
    let dir = PathBuf::from(dir);
    if st.data.is_none() {
        let text = std::fs::read_to_string(dir.join("track.txt")).unwrap_or_default();
        let rows = text
            .lines()
            .filter_map(|line| {
                let v: Vec<f32> = line.split_whitespace().filter_map(|t| t.parse().ok()).collect();
                // index t found qx qy qz qw ms w0..w51
                if v.len() < 8 + 52 {
                    return None;
                }
                let mut w = [0.0f32; 52];
                let mut q = [0.0, 0.0, 0.0, 1.0];
                if v[2] > 0.5 {
                    w.copy_from_slice(&v[8..8 + 52]);
                    q.copy_from_slice(&v[3..7]);
                }
                Some((v[1], q, w))
            })
            .collect();
        st.data = Some(rows);
    }
    st.frames += 1;
    if st.ready_at.is_none() && !models.is_empty() {
        st.ready_at = Some(st.frames);
    }
    let Some(ready) = st.ready_at else {
        return;
    };
    let c = st.frames - ready;
    if c < REPLAY_SETTLE_FRAMES {
        return;
    }
    let k = c - REPLAY_SETTLE_FRAMES;
    let t = k as f32 / REPLAY_FPS;
    let data = st.data.as_ref().expect("loaded above");
    let last_t = data.last().map_or(0.0, |d| d.0);
    if t > last_t + 0.5 {
        if t > last_t + 1.0 {
            exit.write(AppExit::Success);
        }
        return;
    }
    // Hold the newest output whose timestamp has passed.
    let current = data.iter().rev().find(|d| d.0 <= t).or(data.first());
    if let Some(&(_, q, w)) = current {
        let (rotation, expr) = targets_from_parts(q, &w, &mapping.0);
        raw.active = true;
        raw.head = rotation;
        raw.expressions = expr;
    }
    commands
        .spawn(bevy::render::view::screenshot::Screenshot::primary_window())
        .observe(bevy::render::view::screenshot::save_to_disk(dir.join(format!("av_{k:04}.png"))));
}

#[derive(Resource)]
struct LiveRes {
    preview: PreviewShare,
    hud: Arc<Mutex<String>>,
    samples: Arc<Mutex<RenderSamples>>,
    seconds: f32,
    focus: Arc<AtomicBool>,
}

#[derive(Resource)]
struct PreviewImage(Handle<Image>);

#[derive(Component)]
struct PreviewPanel;

/// Marks the avatar's root entity. The loader removes `VrmHandle` once a model is ready, so this is
/// what the focus toggle, model replacement and size fit look the avatar up by.
#[derive(Component)]
struct AvatarRoot;

/// The scene (camera distance and height, preview panel, the avatar's side offset) is laid out for a
/// person this tall, in metres.
const HUMAN_HEIGHT: f32 = 1.65;

/// The camera framing, scaled by the character's height so a small mascot or a giant fills the frame
/// the way a person does. `scale` is the character's height over `HUMAN_HEIGHT`.
#[derive(Resource)]
struct Stage {
    scale: f32,
    camera_pos: Vec3,
    camera_target: Vec3,
    panel_pos: Vec3,
    /// Panel height over width, relative to the panel's own default shape.
    panel_aspect: f32,
}

impl Stage {
    fn new(live_layout: bool) -> Self {
        let (camera_pos, camera_target) = if live_layout {
            (Vec3::new(0.0, 1.45, 1.05), Vec3::new(0.0, 1.4, 0.0))
        } else {
            (Vec3::new(0.0, 1.35, 1.8), Vec3::new(0.0, 1.2, 0.0))
        };
        Stage { scale: 1.0, camera_pos, camera_target, panel_pos: Vec3::new(-0.52, 1.55, -0.4), panel_aspect: 1.0 }
    }
}

/// Scene scale for a character `height` metres tall, or `None` when it is within 10 percent of a
/// person (or the height is unusable), so ordinary models keep the tuned framing exactly.
pub fn stage_scale(height: f32) -> Option<f32> {
    if !height.is_finite() || height < 0.05 {
        return None;
    }
    let k = (height / HUMAN_HEIGHT).clamp(0.15, 4.0);
    (!(0.9..=1.1).contains(&k)).then_some(k)
}

/// Mascots have big heads and short bodies, so scaling the camera distance all the way down fills
/// the frame with the head. Small characters keep at least this much of a person's distance.
const MIN_DISTANCE_SCALE: f32 = 0.8;

/// Scale for the camera's distance from the character (the height and the target follow `scale`).
fn camera_distance_scale(scale: f32) -> f32 {
    scale.max(MIN_DISTANCE_SCALE)
}

/// A mesh bounding box can be far larger than the visible character (the bunny mascot measures 1.13 m
/// with its head bone at 0.47 m), so the height is capped at this multiple of the head bone's height.
const MAX_HEIGHT_OVER_HEAD: f32 = 1.6;

/// World-space top of a mesh's bounding box.
fn aabb_top(aabb: &Aabb, global: &GlobalTransform) -> f32 {
    let m = global.affine().matrix3;
    let h = aabb.half_extents;
    let centre = global.transform_point(Vec3::from(aabb.center));
    centre.y + m.x_axis.y.abs() * h.x + m.y_axis.y.abs() * h.y + m.z_axis.y.abs() * h.z
}

/// Measures each newly loaded model once and scales the stage to its height. The height is the top of
/// its meshes' bounding boxes, with the head bone as a fallback while the boxes are not ready.
fn fit_stage(
    roots: Query<(Entity, &HeadBoneEntity), With<AvatarRoot>>,
    rest: Query<&RestGlobalTransform>,
    bounds: Query<(&Aabb, &GlobalTransform), Without<PreviewPanel>>,
    mut stage: ResMut<Stage>,
    mut fitted: Local<std::collections::HashSet<Entity>>,
    mut waited: Local<u32>,
) {
    for (entity, head) in &roots {
        if fitted.contains(&entity) {
            continue;
        }
        let Ok(rest) = rest.get(head.0) else { continue };
        let head_y = rest.0.translation().y;
        let top = bounds.iter().map(|(a, g)| aabb_top(a, g)).fold(f32::MIN, f32::max);
        *waited += 1;
        if top < head_y && *waited < 90 {
            continue; // bounding boxes not computed yet
        }
        let height = if top >= head_y { top } else { head_y * 1.12 };
        fitted.insert(entity);
        *waited = 0;
        stage.scale = stage_scale(height.min(head_y * MAX_HEIGHT_OVER_HEAD)).unwrap_or(1.0);
    }
}

/// Moves the camera and the preview panel to match the stage scale and panel shape.
fn apply_stage(
    stage: Res<Stage>,
    mut camera: Query<&mut Transform, (With<Camera3d>, Without<PreviewPanel>)>,
    mut panel: Query<&mut Transform, (With<PreviewPanel>, Without<Camera3d>)>,
) {
    if !stage.is_changed() {
        return;
    }
    let s = stage.scale;
    let pos = Vec3::new(stage.camera_pos.x * s, stage.camera_pos.y * s, stage.camera_pos.z * camera_distance_scale(s));
    for mut t in &mut camera {
        *t = Transform::from_translation(pos).looking_at(stage.camera_target * s, Vec3::Y);
    }
    for mut t in &mut panel {
        t.translation = stage.panel_pos * s;
        t.scale = Vec3::new(s, stage.panel_aspect * s, s);
    }
}

/// Avatar x position beside the preview panel (at stage scale 1), and centred in focus mode.
const AVATAR_X_WITH_PANEL: f32 = 0.38;

/// F toggles focus mode: the preview panel is hidden, the avatar centres, and the tracker threads
/// stop building and uploading the preview and the debug drawing.
fn toggle_focus(
    keys: Res<ButtonInput<KeyCode>>,
    live: Res<LiveRes>,
    mut panel: Query<&mut Visibility, With<PreviewPanel>>,
    mut model: Query<&mut Transform, With<AvatarRoot>>,
    mut hud: ResMut<crate::hud::HudHidden>,
    settings: Res<SettingsRes>,
    stage: Res<Stage>,
    mut shown: Local<Option<(bool, bool, u32)>>,
) {
    if keys.just_pressed(KeyCode::KeyF) {
        live.focus.fetch_xor(true, Ordering::Relaxed);
    }
    let focus = live.focus.load(Ordering::Relaxed);
    let show_preview = settings.value.show_preview;
    let key = (focus, show_preview, stage.scale.to_bits());
    if *shown == Some(key) {
        return;
    }
    // Focus mode hides the on-screen help and messages as well.
    hud.0 = focus;
    for mut v in &mut panel {
        *v = if focus || !show_preview { Visibility::Hidden } else { Visibility::Inherited };
    }
    for mut t in &mut model {
        t.translation.x = if focus || !show_preview { 0.0 } else { AVATAR_X_WITH_PANEL * stage.scale };
    }
    *shown = Some(key);
}

/// Keeps the saved lighting name equal to the preset the L key chose, so a later settings change
/// does not switch the preset back.
fn sync_lighting_setting(preset: Res<LightingPreset>, mut settings: ResMut<SettingsRes>) {
    if preset.is_changed() && settings.value.lighting != preset.0.name() {
        settings.value.lighting = preset.0.name().to_string();
    }
}

/// Shows the name of the lighting preset in a toast when it changes (not at start-up).
fn announce_lighting(preset: Res<LightingPreset>, mut toasts: MessageWriter<Toast>, mut first: Local<bool>) {
    if !preset.is_changed() {
        return;
    }
    if !*first {
        *first = true;
        return;
    }
    toasts.write(Toast { text: crate::lighting::preset_label(preset.0), level: ToastLevel::Info });
}

/// Start-up notes (model warnings, settings file problems) shown as toasts once the HUD exists.
#[derive(Resource)]
struct StartupNotes(Vec<Toast>);

fn send_startup_notes(mut notes: ResMut<StartupNotes>, mut toasts: MessageWriter<Toast>) {
    for t in notes.0.drain(..) {
        toasts.write(t);
    }
}

/// Loads a dropped .vrm file in place of the current model after the same checks as at startup.
/// Problems go to stderr and the window title; a model that fails the check is never loaded.
fn load_dropped(
    mut drops: MessageReader<bevy::window::FileDragAndDrop>,
    mut opened: MessageReader<crate::open_dialog::OpenVrm>,
    mut commands: Commands,
    assets: Res<AssetServer>,
    live: Option<Res<LiveRes>>,
    old: Query<Entity, With<AvatarRoot>>,
    stage: Res<Stage>,
    mut settings: ResMut<SettingsRes>,
    mut toasts: MessageWriter<Toast>,
) {
    let dropped = drops.read().filter_map(|d| match d {
        bevy::window::FileDragAndDrop::DroppedFile { path_buf, .. } => Some(path_buf.clone()),
        _ => None,
    });
    let picked = opened.read().map(|o| o.0.clone());
    let paths: Vec<PathBuf> = dropped.chain(picked).collect();
    for path_buf in &paths {
        let mut say = |text: String, level: ToastLevel| {
            eprintln!("{text}");
            toasts.write(Toast { text, level });
        };
        // A VRM 0.x file is converted to a 1.0 copy first; `model.path` is what gets loaded.
        let model = match open_model(path_buf) {
            Ok(m) => m,
            Err(e) => {
                say(format!("Model refused: {e}"), ToastLevel::Error);
                continue;
            }
        };
        for note in &model.notes {
            say(note.clone(), ToastLevel::Info);
        }
        for w in crate::errors::vrm_warnings(&model.path) {
            say(w, ToastLevel::Warn);
        }
        let x = match &live {
            Some(l) if !l.focus.load(Ordering::Relaxed) => AVATAR_X_WITH_PANEL * stage.scale,
            _ => 0.0,
        };
        for e in &old {
            commands.entity(e).despawn();
        }
        commands.spawn((AvatarRoot, VrmHandle(assets.load(model.path.clone())), Transform::from_xyz(x, 0.0, 0.0)));
        let name = path_buf.file_name().unwrap_or_default().to_string_lossy();
        say(format!("Loaded {name}"), ToastLevel::Info);
        // Remember the model so the next start opens it without a flag.
        let remembered = Some(path_buf.to_string_lossy().into_owned());
        if settings.value.last_model != remembered {
            settings.value.last_model = remembered;
            if let Err(e) = settings.value.save(&settings.path) {
                eprintln!("{e}");
            }
        }
    }
}

const PREVIEW_W: u32 = 640;
const PREVIEW_H: u32 = 480;
/// Preview panel size in world units; its height is scaled to the camera's aspect ratio.
const PANEL_W: f32 = 1.05;
const PANEL_H: f32 = 0.79;

fn preview_image(width: u32, height: u32, fill: &[u8; 4]) -> Image {
    Image::new_fill(
        bevy::render::render_resource::Extent3d { width, height, depth_or_array_layers: 1 },
        bevy::render::render_resource::TextureDimension::D2,
        fill,
        bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
        bevy::asset::RenderAssetUsages::default(),
    )
}

/// Copies the newest camera preview into the texture shown beside the avatar.
fn update_preview(
    live: Res<LiveRes>,
    handle: Res<PreviewImage>,
    mut images: ResMut<Assets<Image>>,
    mut stage: ResMut<Stage>,
) {
    if live.focus.load(Ordering::Relaxed) {
        return;
    }
    let taken = live.preview.try_lock().ok().and_then(|mut g| g.take());
    let Some(p) = taken else {
        return;
    };
    let started = std::time::Instant::now();
    if let Some(mut image) = images.get_mut(&handle.0) {
        if image.width() != p.width || image.height() != p.height {
            // The camera delivers a size other than the placeholder: replace the texture and give
            // the panel the camera's aspect ratio.
            *image = preview_image(p.width, p.height, &[40, 40, 40, 255]);
            stage.panel_aspect = (p.height as f32 / p.width as f32) / (PANEL_H / PANEL_W);
        }
        image.data = Some(p.rgba);
    }
    if let Ok(mut samples) = live.samples.lock() {
        samples.push_upload(started.elapsed().as_secs_f32() * 1000.0);
    }
}

/// Shows the monitor thread's stats line in the window title.
fn update_title(
    live: Res<LiveRes>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
    mut last: Local<Option<String>>,
) {
    let Ok(text) = live.hud.try_lock().map(|g| g.clone()) else {
        return;
    };
    if text.is_empty() || last.as_deref() == Some(text.as_str()) {
        return;
    }
    for mut window in &mut windows {
        window.title = text.clone();
    }
    *last = Some(text);
}

/// Counts render frames and, when `seconds` is above zero, closes the window that long after the model\n/// became ready. Zero (the default) runs until the window is closed.
fn live_clock(
    live: Res<LiveRes>,
    time: Res<Time<Real>>,
    models: Query<(), With<ExpressionEntityMap>>,
    mut ready_at: Local<Option<f32>>,
    mut exit: MessageWriter<AppExit>,
) {
    if ready_at.is_none() && !models.is_empty() {
        *ready_at = Some(time.elapsed_secs());
    }
    let Some(start) = *ready_at else {
        return;
    };
    if let Ok(mut samples) = live.samples.lock() {
        samples.push_frame(time.delta_secs() * 1000.0);
    }
    if live.seconds > 0.0 && time.elapsed_secs() - start >= live.seconds {
        exit.write(AppExit::Success);
    }
}

/// Checks the VRM (converting a 0.x file to 1.0) and loads the mapping table. Runs before any Bevy
/// App is built. The returned model's path is the file to load.
fn prepare(cfg: &AvatarConfig) -> Result<(MapTable, Option<OpenedModel>), RunError> {
    let model = if cfg.vrm_path.as_os_str().is_empty() {
        None
    } else {
        let model = open_model(&cfg.vrm_path).map_err(RunError::Avatar)?;
        for note in &model.notes {
            eprintln!("note: {}: {note}", cfg.vrm_path.display());
        }
        for w in crate::errors::vrm_warnings(&model.path) {
            eprintln!("warning: {}: {w}", cfg.vrm_path.display());
        }
        Some(model)
    };
    Ok((map_table::load(&cfg.map_path).map_err(RunError::Map)?, model))
}

/// Shown when the program starts with no model.
const NO_MODEL_HINT: &str = "Drop a .vrm file on the window or press Ctrl+O to open a model";

/// `file_name` is `None` when the program starts without a model; one is loaded later by a drop or
/// Ctrl+O.
fn build_app(
    root: String,
    file_name: Option<String>,
    share: TrackingShare,
    mapping: MapTable,
    live: Option<LiveConfig>,
    body: Option<BodyPose>,
    mut notes: Vec<Toast>,
) -> App {
    let live_layout = live.is_some();
    let settings_path = crate::settings::default_path();
    let (settings, settings_error) = Settings::load(&settings_path);
    if let Some(text) = settings_error {
        eprintln!("{text}");
        notes.push(Toast { text, level: ToastLevel::Warn });
    }
    // The tracker threads share this with the panel; without a live run nothing reads it.
    let tuning = live.as_ref().map_or_else(|| Arc::new(LiveTuning::new(&settings)), |l| Arc::clone(&l.tuning));
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            // Dropped files live anywhere; they are checked by `validate_vrm1` before loading.
            .set(AssetPlugin { file_path: root, unapproved_path_mode: UnapprovedPathMode::Allow, ..default() })
            .set(WindowPlugin {
                primary_window: Some(Window { title: "Avatar spike".into(), ..default() }),
                ..default()
            }),
    )
    .add_plugins((VrmPlugin, HeadPlugin, ExpressionPlugin, RenderReportPlugin, crate::gaze::GazePlugin, crate::brows::BrowPlugin))
    .add_plugins((HudPlugin, LightingPlugin, crate::open_dialog::OpenDialogPlugin))
    .add_plugins((SettingsApplyPlugin, SettingsUiPlugin))
    .insert_resource(SettingsRes { value: settings.clone(), path: settings_path })
    .insert_resource(LiveTuningRes(tuning))
    .insert_resource(StartupNotes(notes))
    .add_systems(Update, (sync_lighting_setting, announce_lighting))
    .add_systems(PostStartup, send_startup_notes)
    .insert_resource(ClearColor(crate::settings_apply::clear_color(settings.background)))
    .insert_resource(GlobalAmbientLight { brightness: 200.0, ..default() })
    .insert_resource(SharedSlot(share))
    .insert_resource(MappingRes(mapping))
    .init_resource::<RawTarget>()
    .add_systems(Update, (apply_tracking, smooth_targets, update_gaze, update_brows).chain())
    .insert_resource(Stage::new(live_layout))
    .add_systems(Update, (shot_and_exit, load_dropped, fit_stage, apply_stage).chain())
    .add_systems(Update, replay.before(smooth_targets))
    .add_systems(
        Startup,
        move |mut commands: Commands,
              assets: Res<AssetServer>,
              mut meshes: ResMut<Assets<Mesh>>,
              mut materials: ResMut<Assets<StandardMaterial>>,
              mut images: ResMut<Assets<Image>>| {
            // VRM 1.0 models face +z. Camera in front at head height, looking at the upper body.
            let (camera, avatar_x) = if live_layout {
                (
                    Transform::from_xyz(0.0, 1.45, 1.05).looking_at(Vec3::new(0.0, 1.4, 0.0), Vec3::Y),
                    0.38,
                )
            } else {
                (
                    Transform::from_xyz(0.0, 1.35, 1.8).looking_at(Vec3::new(0.0, 1.2, 0.0), Vec3::Y),
                    0.0,
                )
            };
            commands.spawn((Camera3d::default(), camera));
            // Key light from the front right, a dim cool fill from the left, a little ambient so the
            // shaded side keeps its colour. No shadows: they cost a render pass for little gain here.
            commands.spawn((
                LightRole::Key,
                DirectionalLight { illuminance: 9_000.0, color: Color::srgb(1.0, 0.96, 0.9), ..default() },
                Transform::from_xyz(1.0, 2.0, 2.0).looking_at(Vec3::ZERO, Vec3::Y),
            ));
            commands.spawn((
                LightRole::Fill,
                DirectionalLight { illuminance: 3_000.0, color: Color::srgb(0.85, 0.92, 1.0), ..default() },
                Transform::from_xyz(-2.0, 1.0, 1.5).looking_at(Vec3::ZERO, Vec3::Y),
            ));
            if let Some(name) = &file_name {
                commands.spawn((
                    AvatarRoot,
                    VrmHandle(assets.load(name.clone())),
                    Transform::from_xyz(avatar_x, 0.0, 0.0),
                ));
            }
            if live_layout {
                let handle = images.add(preview_image(PREVIEW_W, PREVIEW_H, &[40, 40, 40, 255]));
                commands.insert_resource(PreviewImage(handle.clone()));
                // The panel sits behind the avatar so the T-pose arms do not hide it all.
                let material = materials.add(StandardMaterial {
                    base_color_texture: Some(handle),
                    unlit: true,
                    ..default()
                });
                commands.spawn((
                    PreviewPanel,
                    Mesh3d(meshes.add(Rectangle::new(PANEL_W, PANEL_H))),
                    MeshMaterial3d(material),
                    Transform::from_xyz(-0.52, 1.55, -0.4),
                ));
            }
        },
    );
    let live_body = live.as_ref().and_then(|l| l.body.clone());
    if body.is_some() || live_body.is_some() {
        app.add_plugins(BodyPlugin);
    }
    if let Some(pose) = body {
        app.insert_resource(BodyStill(pose));
    }
    if let Some(share) = live_body {
        app.insert_resource(LiveBody(share));
    }
    if let Some(live) = live {
        app.insert_resource(LiveRes {
            preview: live.preview,
            hud: live.hud,
            samples: live.samples,
            seconds: live.seconds,
            focus: live.focus,
        })
        .add_systems(Update, (update_preview, update_title, live_clock, toggle_focus));
    }
    // The render frame cap (default about 30 fps: the avatar only changes at camera rate, and the cap
    // saves about 25 points of CPU, docs/subtasks/debate/08.md variant V1) is set from the settings by
    // `SettingsApplyPlugin` on the first frame.
    app
}

/// Validates the VRM and mapping table, then opens the window and runs until it closes.
/// A graphics adapter panic becomes `RunError::Graphics`; other panics become `RunError::Panic`.
pub fn run(cfg: AvatarConfig) -> Result<RunSummary, RunError> {
    run_with_body(cfg, None)
}

/// Like [`run`], and poses the upper body from a still-image `BodyPose` when given.
pub fn run_with_body(cfg: AvatarConfig, body: Option<BodyPose>) -> Result<RunSummary, RunError> {
    let (mapping, model) = prepare(&cfg)?;

    let root = model
        .as_ref()
        .and_then(|m| m.path.parent())
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
        .to_string_lossy()
        .into_owned();
    let file_name = model.as_ref().map(|m| m.path.file_name().unwrap_or_default().to_string_lossy().into_owned());

    let mut notes: Vec<Toast> = Vec::new();
    match &model {
        Some(m) => {
            notes.extend(m.notes.iter().map(|text| Toast { text: text.clone(), level: ToastLevel::Info }));
            notes.extend(
                crate::errors::vrm_warnings(&m.path).into_iter().map(|text| Toast { text, level: ToastLevel::Warn }),
            );
        }
        None => notes.push(Toast { text: NO_MODEL_HINT.into(), level: ToastLevel::Info }),
    }
    let mut app = build_app(root, file_name, cfg.tracking, mapping, cfg.live, body, notes);
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        app.run();
    }));
    if let Err(payload) = outcome {
        let msg = payload_text(payload.as_ref());
        return Err(if graphics_panic_message(&msg) {
            RunError::Graphics(describe_graphics_failure(&msg))
        } else {
            RunError::Panic(msg)
        });
    }

    let (ticks, average_fps) = app
        .world()
        .get_resource::<FpsMeter>()
        .map_or((0, None), |m| (m.ticks(), m.average_fps()));
    Ok(RunSummary {
        ticks,
        average_fps,
        memory: query_dxgi_memory(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_model_path_starts_with_no_model() {
        let map = std::env::temp_dir().join(format!("avatar_empty_model_map_{}.json", std::process::id()));
        std::fs::write(&map, "{}").unwrap();
        let cfg = AvatarConfig { vrm_path: PathBuf::new(), map_path: map.clone(), tracking: Default::default(), live: None };
        let prepared = prepare(&cfg);
        let _ = std::fs::remove_file(&map);
        let (_, model) = prepared.ok().expect("an empty path is not an error");
        assert!(model.is_none());
    }

    #[test]
    fn the_camera_scales_with_the_characters_height() {
        assert_eq!(stage_scale(1.65), None, "a person keeps the tuned framing");
        assert_eq!(stage_scale(1.5), None, "within 10 percent is left alone");
        let mascot = stage_scale(0.66).unwrap();
        assert!((mascot - 0.4).abs() < 1e-5, "a 0.66 m mascot gets a 0.4 scale: {mascot}");
        let giant = stage_scale(3.3).unwrap();
        assert!((giant - 2.0).abs() < 1e-5, "{giant}");
        assert_eq!(stage_scale(0.0), None);
        assert_eq!(stage_scale(f32::NAN), None);
        assert_eq!(stage_scale(0.1).unwrap(), 0.15, "the scale is bounded");
        assert_eq!(stage_scale(100.0).unwrap(), 4.0);
    }

    #[test]
    fn a_small_mascot_is_not_framed_as_close_as_its_height_alone_says() {
        assert_eq!(camera_distance_scale(0.4), MIN_DISTANCE_SCALE);
        assert_eq!(camera_distance_scale(0.15), MIN_DISTANCE_SCALE);
        assert_eq!(camera_distance_scale(0.8), 0.8, "bigger characters follow their height");
        assert_eq!(camera_distance_scale(2.0), 2.0);
    }
    use crate::map_table::parse;

    const REAL_TABLE: &str = include_str!("../../../data/blendshape_map.json");

    fn temp_file(name: &str, bytes: &[u8]) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "avatar_window_{name}_{}.vrm",
            std::process::id()
        ));
        std::fs::write(&path, bytes).unwrap();
        path
    }

    fn glb(json: &str) -> Vec<u8> {
        let mut body = json.as_bytes().to_vec();
        while !body.len().is_multiple_of(4) {
            body.push(b' ');
        }
        let total = 12 + 8 + body.len();
        let mut out = b"glTF".to_vec();
        out.extend(2u32.to_le_bytes());
        out.extend((total as u32).to_le_bytes());
        out.extend((body.len() as u32).to_le_bytes());
        out.extend(0x4E4F_534Au32.to_le_bytes());
        out.extend(body);
        out
    }

    const VRM1_JSON: &str =
        r#"{"asset":{"version":"2.0"},"extensions":{"VRMC_vrm":{"specVersion":"1.0"}}}"#;

    fn config(vrm: PathBuf, map: PathBuf) -> AvatarConfig {
        AvatarConfig {
            vrm_path: vrm,
            map_path: map,
            tracking: Arc::new(Mutex::new(None)),
            live: None,
        }
    }

    #[test]
    fn non_vrm_file_returns_not_vrm1_before_any_app() {
        let vrm = temp_file("text", b"hello, this is not a model");
        let map = PathBuf::from("data/blendshape_map.json");
        match run(config(vrm.clone(), map)) {
            Err(RunError::Avatar(AvatarError::NotVrm1 { path, .. })) => assert_eq!(path, vrm),
            other => panic!("expected NotVrm1, got {other:?}"),
        }
        let _ = std::fs::remove_file(&vrm);
    }

    #[test]
    fn missing_vrm_file_returns_file_missing() {
        let vrm = std::env::temp_dir().join("avatar_window_no_such_file.vrm");
        let _ = std::fs::remove_file(&vrm);
        let map = PathBuf::from("data/blendshape_map.json");
        assert!(matches!(
            run(config(vrm, map)),
            Err(RunError::Avatar(AvatarError::FileMissing(_)))
        ));
    }

    #[test]
    fn missing_map_returns_map_error_before_any_app() {
        let vrm = temp_file("valid", &glb(VRM1_JSON));
        let map = std::env::temp_dir().join("avatar_window_no_such_map.json");
        let _ = std::fs::remove_file(&map);
        match run(config(vrm.clone(), map)) {
            Err(RunError::Map(MapError::Missing(_))) => {}
            other => panic!("expected Map Missing, got {other:?}"),
        }
        let _ = std::fs::remove_file(&vrm);
    }

    #[test]
    fn yaw_20_degrees_converts_to_bevy_rotation() {
        // nalgebra coords are [i, j, k, w]. Yaw 20 degrees is a half-angle of 10 degrees about +y.
        let half = 10f32.to_radians();
        let (rotation, _) = targets_from_parts(
            [0.0, half.sin(), 0.0, half.cos()],
            &[0.0; 52],
            &MapTable::default(),
        );
        let expected = Quat::from_rotation_y(20f32.to_radians());
        let rotated = rotation * Vec3::X;
        assert!(rotated.distance(expected * Vec3::X) < 1e-5, "rotated to {rotated:?}");
    }

    #[test]
    fn jaw_open_maps_to_aa_one() {
        let table = parse(REAL_TABLE, "data/blendshape_map.json").expect("real table parses");
        let mut weights = [0.0f32; 52];
        weights[vt_core::names::index_of("jawOpen").expect("known name")] = 1.0;
        let (_, expressions) = targets_from_parts([0.0, 0.0, 0.0, 1.0], &weights, &table);
        assert_eq!(expressions.get("aa").copied(), Some(1.0));
    }

    #[test]
    fn graphics_panic_text_is_classified() {
        assert!(graphics_panic_message("Unable to find a GPU!"));
        assert!(graphics_panic_message("no suitable adapter found"));
        assert!(!graphics_panic_message("index out of bounds"));
    }

    #[test]
    fn run_error_display_names_the_file() {
        let e = RunError::Map(MapError::Missing("data/blendshape_map.json".into()));
        assert!(e.to_string().contains("data/blendshape_map.json"));
        let g = RunError::Graphics(describe_graphics_failure("Unable to find a GPU!"));
        assert!(g.to_string().contains("no usable graphics API"));
    }

    #[test]
    fn follow_factor_is_zero_without_time_and_approaches_one() {
        assert_eq!(follow_factor(0.0, 0.03), 0.0);
        let one_tau = follow_factor(0.03, 0.03);
        assert!((one_tau - 0.632).abs() < 0.001, "got {one_tau}");
        assert!(follow_factor(1.0, 0.03) > 0.999);
    }

    #[test]
    fn follow_weight_converges_and_snaps() {
        let mut v = 0.0;
        let mut steps = 0;
        while v != 1.0 && steps < 1000 {
            v = follow_weight(v, 1.0, 0.3);
            steps += 1;
        }
        assert_eq!(v, 1.0);
        assert!(steps < 30, "took {steps} steps");
        assert_eq!(follow_weight(0.2, 0.2, 0.5), 0.2);
    }

    #[test]
    fn smooth_body_converges_to_target() {
        let target = [Quat::from_rotation_y(0.8)];
        let mut current = [Quat::IDENTITY];
        for _ in 0..600 {
            smooth_body(&mut current, &target, &[true], 1.0 / 60.0);
        }
        assert!(current[0].angle_between(target[0]) < 1e-3);
    }

    #[test]
    fn smooth_body_step_is_clamped_to_15_degrees() {
        let target = [Quat::from_rotation_y(2.0)];
        let mut current = [Quat::IDENTITY];
        smooth_body(&mut current, &target, &[true], 1.0);
        let step = Quat::IDENTITY.angle_between(current[0]);
        assert!(step <= 15f32.to_radians() + 1e-4, "step {}", step.to_degrees());
        assert!(step > 0.0);
    }

    #[test]
    fn smooth_body_hand_moves_faster_than_torso() {
        let target = [Quat::from_rotation_y(0.1), Quat::from_rotation_y(0.1)];
        let mut current = [Quat::IDENTITY, Quat::IDENTITY];
        smooth_body(&mut current, &target, &[true, false], 1.0 / 60.0);
        let hand = current[0].angle_between(Quat::IDENTITY);
        let torso = current[1].angle_between(Quat::IDENTITY);
        assert!(hand > torso, "hand {hand} torso {torso}");
    }
}

