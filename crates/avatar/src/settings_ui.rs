//! Settings panel: a right-side bevy_ui panel with BASIC and ADVANCED tabs.
//! S opens and closes it, Tab switches tabs, Esc closes it. It is hidden in focus mode.
//!
//! The panel edits only `SettingsRes`, always through `Settings::clamped()`. Edits save to
//! `settings.json` at most once per second, and at once on close. Save errors show as an Error toast.
//!
//! Closed, the panel costs one key check per frame. Its content is rebuilt only when the panel
//! state or the settings change while it is open.

use bevy::prelude::*;

use crate::hud::{toast_rgb, HudHidden, Toast, ToastLevel};
use crate::lighting::Preset;
use crate::settings::{Antialiasing, Background, Settings, SettingsRes};

const PANEL_WIDTH: f32 = 360.0;
const PANEL_OPACITY: f32 = 0.9;
const ROW_HEIGHT: f32 = 44.0;
const TEXT_SIZE: f32 = 16.0;
const BUTTON_HEIGHT: f32 = 32.0;
const STEP_BUTTON_WIDTH: f32 = 36.0;
const WIDE_BUTTON_WIDTH: f32 = 120.0;
/// Same dark panel colour as hud.rs.
const PANEL_BG: [f32; 3] = [0.06, 0.06, 0.08];
const TEXT_RGB: [f32; 3] = [0.92, 0.93, 0.95];
const BUTTON_BG: [f32; 3] = [0.18, 0.19, 0.23];
const BUTTON_HOVER_BG: [f32; 3] = [0.28, 0.30, 0.36];
const BUTTON_PRESS_BG: [f32; 3] = [0.36, 0.40, 0.48];
/// Seconds between saves while edits keep coming.
const SAVE_INTERVAL_SECS: f32 = 1.0;
const CONFIDENCE_STEP: f32 = 0.05;
const SCALE_STEP: f32 = 0.1;
const GAIN_STEP: f32 = 0.1;
const FRAME_CAP_STEP: i32 = 5;

/// Which group of options the panel shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Basic,
    Advanced,
}

impl Tab {
    /// The other tab. Tab key and tab buttons both use it.
    pub fn next(self) -> Tab {
        match self {
            Tab::Basic => Tab::Advanced,
            Tab::Advanced => Tab::Basic,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Tab::Basic => "BASIC",
            Tab::Advanced => "ADVANCED",
        }
    }
}

/// One option in the panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowId {
    Background,
    Lighting,
    Confidence,
    ShowPreview,
    ShowDebugDrawing,
    AutoBrightness,
    FaceConfidence,
    BodyConfidence,
    HandConfidence,
    KeyScale,
    FillScale,
    AmbientScale,
    Antialiasing,
    FrameCap,
    BrowGain,
    GazeGain,
    CameraSize,
    Mirror,
}

/// How a row is edited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowKind {
    /// `[-] value [+]`.
    Stepper,
    /// One On/Off button.
    Toggle,
    /// One button that moves to the next choice.
    Cycle,
    /// Shown only. Takes effect after a restart.
    ReadOnly,
}

/// A row in the panel: its label and how it is edited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RowSpec {
    pub id: RowId,
    pub label: &'static str,
    pub kind: RowKind,
}

/// The rows of one tab, in display order.
pub fn rows(tab: Tab) -> Vec<RowSpec> {
    use RowKind::*;
    let r = |id: RowId, label: &'static str, kind: RowKind| RowSpec { id, label, kind };
    match tab {
        Tab::Basic => vec![
            r(RowId::Background, "Background", Cycle),
            r(RowId::Lighting, "Lighting preset", Cycle),
            r(RowId::Confidence, "Confidence", Stepper),
            r(RowId::ShowPreview, "Show camera preview", Toggle),
            r(RowId::ShowDebugDrawing, "Show debug drawing", Toggle),
            r(RowId::AutoBrightness, "Auto brightness", Toggle),
        ],
        Tab::Advanced => vec![
            r(RowId::FaceConfidence, "Face confidence", Stepper),
            r(RowId::BodyConfidence, "Body confidence", Stepper),
            r(RowId::HandConfidence, "Hand confidence", Stepper),
            r(RowId::KeyScale, "Key light", Stepper),
            r(RowId::FillScale, "Fill light", Stepper),
            r(RowId::AmbientScale, "Ambient light", Stepper),
            r(RowId::Antialiasing, "Anti-aliasing", Cycle),
            r(RowId::FrameCap, "Frame cap (fps)", Stepper),
            r(RowId::BrowGain, "Brow gain", Stepper),
            r(RowId::GazeGain, "Gaze gain", Stepper),
            r(RowId::CameraSize, "Camera size (after restart)", ReadOnly),
            r(RowId::Mirror, "Mirror (after restart)", ReadOnly),
        ],
    }
}

/// The preset whose name matches. Unknown names give Studio.
pub fn preset_from_name(name: &str) -> Preset {
    Preset::from_name(name).unwrap_or_default()
}

/// Rounds to `decimals` places, so repeated steps do not drift (0.8 + 0.05 stays 0.85).
fn snap(v: f32, decimals: i32) -> f32 {
    let f = 10f32.powi(decimals);
    (v * f).round() / f
}

fn cycle_background(b: Background, direction: i32) -> Background {
    let steps = if direction < 0 { Background::ALL.len() - 1 } else { 1 };
    (0..steps).fold(b, |acc, _| acc.next())
}

fn cycle_antialiasing(a: Antialiasing, direction: i32) -> Antialiasing {
    let steps = if direction < 0 { 2 } else { 1 };
    (0..steps).fold(a, |acc, _| acc.next())
}

fn cycle_lighting(name: &str, direction: i32) -> String {
    let steps = if direction < 0 { 3 } else { 1 };
    (0..steps).fold(preset_from_name(name), |p, _| p.next()).name().to_string()
}

/// Changes one option by one step. `direction` is +1 or -1 for steppers and cycles; toggles flip either way.
/// Read-only rows do nothing. The result always passes through `Settings::clamped()`.
pub fn apply_step(settings: &mut Settings, row: RowId, direction: i32) {
    let dir = direction.signum() as f32;
    match row {
        RowId::Background => settings.background = cycle_background(settings.background, direction),
        RowId::Lighting => settings.lighting = cycle_lighting(&settings.lighting, direction),
        RowId::Confidence => {
            let v = snap(settings.confidence + CONFIDENCE_STEP * dir, 2);
            settings.set_all_confidence(v);
        }
        RowId::ShowPreview => settings.show_preview = !settings.show_preview,
        RowId::ShowDebugDrawing => settings.show_debug_drawing = !settings.show_debug_drawing,
        RowId::AutoBrightness => settings.auto_brightness = !settings.auto_brightness,
        RowId::FaceConfidence => {
            settings.face_confidence = snap(settings.face_confidence + CONFIDENCE_STEP * dir, 2)
        }
        RowId::BodyConfidence => {
            settings.body_confidence = snap(settings.body_confidence + CONFIDENCE_STEP * dir, 2)
        }
        RowId::HandConfidence => {
            settings.hand_confidence = snap(settings.hand_confidence + CONFIDENCE_STEP * dir, 2)
        }
        RowId::KeyScale => settings.key_scale = snap(settings.key_scale + SCALE_STEP * dir, 1),
        RowId::FillScale => settings.fill_scale = snap(settings.fill_scale + SCALE_STEP * dir, 1),
        RowId::AmbientScale => settings.ambient_scale = snap(settings.ambient_scale + SCALE_STEP * dir, 1),
        RowId::Antialiasing => settings.antialiasing = cycle_antialiasing(settings.antialiasing, direction),
        RowId::FrameCap => {
            settings.frame_cap = (settings.frame_cap as i32 + FRAME_CAP_STEP * direction.signum()).max(0) as u32
        }
        RowId::BrowGain => settings.brow_gain = snap(settings.brow_gain + GAIN_STEP * dir, 1),
        RowId::GazeGain => settings.gaze_gain = snap(settings.gaze_gain + GAIN_STEP * dir, 1),
        RowId::CameraSize | RowId::Mirror => {}
    }
    *settings = settings.clone().clamped();
}

fn percent(v: f32) -> String {
    format!("{:.0}%", v * 100.0)
}

fn on_off(b: bool) -> String {
    if b { "On" } else { "Off" }.to_string()
}

/// Text shown on a row's control.
pub fn value_text(s: &Settings, row: RowId) -> String {
    match row {
        RowId::Background => s.background.name().to_string(),
        RowId::Lighting => s.lighting.clone(),
        RowId::Confidence => percent(s.confidence),
        RowId::ShowPreview => on_off(s.show_preview),
        RowId::ShowDebugDrawing => on_off(s.show_debug_drawing),
        RowId::AutoBrightness => on_off(s.auto_brightness),
        RowId::FaceConfidence => percent(s.face_confidence),
        RowId::BodyConfidence => percent(s.body_confidence),
        RowId::HandConfidence => percent(s.hand_confidence),
        RowId::KeyScale => format!("{:.1}", s.key_scale),
        RowId::FillScale => format!("{:.1}", s.fill_scale),
        RowId::AmbientScale => format!("{:.1}", s.ambient_scale),
        RowId::Antialiasing => s.antialiasing.name().to_string(),
        RowId::FrameCap => format!("{} fps", s.frame_cap),
        RowId::BrowGain => format!("{:.1}", s.brow_gain),
        RowId::GazeGain => format!("{:.1}", s.gaze_gain),
        RowId::CameraSize => format!("{} x {}", s.camera_width, s.camera_height),
        RowId::Mirror => on_off(s.mirror),
    }
}

/// What a button does when pressed.
#[derive(Debug, Clone, Copy)]
enum Action {
    SelectTab(Tab),
    Step(RowId, i32),
    Activate(RowId),
    Reset,
}

/// Panel state. `dirty` means a change has not been written to disk yet.
#[derive(Resource, Debug, Default)]
struct PanelState {
    open: bool,
    tab: Tab,
    dirty: bool,
    since_save: f32,
}

/// Marks the panel root. Its children are rebuilt when the tab, open state or settings change.
#[derive(Component)]
struct PanelRoot;

/// Attached to every button.
#[derive(Component, Clone, Copy)]
struct UiAction(Action);

fn rgb(c: [f32; 3]) -> Color {
    Color::srgb(c[0], c[1], c[2])
}

fn spawn_panel(mut commands: Commands) {
    commands.spawn((
        PanelRoot,
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(0.0),
            bottom: Val::Px(0.0),
            right: Val::Px(0.0),
            width: Val::Px(PANEL_WIDTH),
            padding: UiRect::all(Val::Px(16.0)),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(6.0),
            display: Display::None,
            ..default()
        },
        BackgroundColor(Color::srgba(PANEL_BG[0], PANEL_BG[1], PANEL_BG[2], PANEL_OPACITY)),
        GlobalZIndex(25),
    ));
}

fn spawn_text(commands: &mut Commands, parent: Entity, text: String, color: [f32; 3]) {
    commands.spawn((
        ChildOf(parent),
        Text::new(text),
        TextFont::from_font_size(TEXT_SIZE),
        TextColor(rgb(color)),
    ));
}

fn spawn_button(commands: &mut Commands, parent: Entity, label: String, action: Action, width: f32) {
    commands.spawn((
        ChildOf(parent),
        Button,
        Node {
            width: Val::Px(width),
            height: Val::Px(BUTTON_HEIGHT),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(rgb(BUTTON_BG)),
        UiAction(action),
        children![(Text::new(label), TextFont::from_font_size(TEXT_SIZE), TextColor(rgb(TEXT_RGB)))],
    ));
}

fn spawn_row(commands: &mut Commands, root: Entity, spec: RowSpec, s: &Settings) {
    let row = commands
        .spawn((
            ChildOf(root),
            Node {
                width: Val::Percent(100.0),
                height: Val::Px(ROW_HEIGHT),
                flex_direction: FlexDirection::Row,
                justify_content: JustifyContent::SpaceBetween,
                align_items: AlignItems::Center,
                ..default()
            },
        ))
        .id();
    spawn_text(commands, row, spec.label.to_string(), TEXT_RGB);

    let control = commands
        .spawn((
            ChildOf(row),
            Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(6.0),
                ..default()
            },
        ))
        .id();
    match spec.kind {
        RowKind::Stepper => {
            spawn_button(commands, control, "-".into(), Action::Step(spec.id, -1), STEP_BUTTON_WIDTH);
            spawn_text(commands, control, value_text(s, spec.id), TEXT_RGB);
            spawn_button(commands, control, "+".into(), Action::Step(spec.id, 1), STEP_BUTTON_WIDTH);
        }
        RowKind::Toggle | RowKind::Cycle => spawn_button(
            commands,
            control,
            value_text(s, spec.id),
            Action::Activate(spec.id),
            WIDE_BUTTON_WIDTH,
        ),
        RowKind::ReadOnly => spawn_text(commands, control, value_text(s, spec.id), toast_rgb(ToastLevel::Warn)),
    }
}

fn spawn_content(commands: &mut Commands, root: Entity, panel: &PanelState, s: &Settings) {
    let tabs = commands
        .spawn((
            ChildOf(root),
            Node {
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(6.0),
                ..default()
            },
        ))
        .id();
    for tab in [Tab::Basic, Tab::Advanced] {
        let label = if tab == panel.tab { format!("[{}]", tab.label()) } else { tab.label().to_string() };
        spawn_button(commands, tabs, label, Action::SelectTab(tab), WIDE_BUTTON_WIDTH);
    }

    for spec in rows(panel.tab) {
        spawn_row(commands, root, spec, s);
    }

    let footer = commands
        .spawn((
            ChildOf(root),
            Node {
                width: Val::Percent(100.0),
                height: Val::Px(ROW_HEIGHT),
                flex_direction: FlexDirection::Row,
                justify_content: JustifyContent::SpaceBetween,
                align_items: AlignItems::Center,
                ..default()
            },
        ))
        .id();
    spawn_button(commands, footer, "Reset".into(), Action::Reset, WIDE_BUTTON_WIDTH);
    spawn_text(commands, footer, "saved automatically".into(), TEXT_RGB);
}

/// S opens and closes the panel. Esc closes it. Tab switches tabs while it is open.
/// Keys do nothing in focus mode.
fn panel_keys(
    keys: Res<ButtonInput<KeyCode>>,
    hidden: Res<HudHidden>,
    mut panel: ResMut<PanelState>,
    res: Res<SettingsRes>,
    mut toasts: MessageWriter<Toast>,
) {
    if hidden.0 {
        return;
    }
    if keys.just_pressed(KeyCode::KeyS) {
        if panel.open {
            close_panel(&mut panel, &res, &mut toasts);
        } else {
            panel.open = true;
        }
    } else if panel.open {
        if keys.just_pressed(KeyCode::Escape) {
            close_panel(&mut panel, &res, &mut toasts);
        } else if keys.just_pressed(KeyCode::Tab) {
            panel.tab = panel.tab.next();
        }
    }
}

fn close_panel(panel: &mut PanelState, res: &SettingsRes, toasts: &mut MessageWriter<Toast>) {
    panel.open = false;
    if panel.dirty {
        write_now(panel, res, toasts);
    }
}

fn write_now(panel: &mut PanelState, res: &SettingsRes, toasts: &mut MessageWriter<Toast>) {
    panel.dirty = false;
    panel.since_save = 0.0;
    if let Err(e) = res.value.save(&res.path) {
        toasts.write(Toast {
            text: format!("Settings not saved: {e}"),
            level: ToastLevel::Error,
        });
    }
}

/// Applies a change through `clamped()` and marks the panel dirty only when the value really changed.
fn edit(res: &mut SettingsRes, panel: &mut PanelState, change: impl FnOnce(&mut Settings)) {
    let mut next = res.value.clone();
    change(&mut next);
    let next = next.clamped();
    if next != res.value {
        res.value = next;
        panel.dirty = true;
    }
}

/// Runs button presses and sets button colours for hover and press.
fn handle_buttons(
    mut buttons: Query<(&Interaction, &UiAction, &mut BackgroundColor), Changed<Interaction>>,
    mut panel: ResMut<PanelState>,
    mut res: ResMut<SettingsRes>,
) {
    for (interaction, action, mut bg) in &mut buttons {
        let color = match interaction {
            Interaction::Pressed => BUTTON_PRESS_BG,
            Interaction::Hovered => BUTTON_HOVER_BG,
            Interaction::None => BUTTON_BG,
        };
        bg.0 = rgb(color);
        if !matches!(interaction, Interaction::Pressed) {
            continue;
        }
        match action.0 {
            Action::SelectTab(tab) => panel.tab = tab,
            Action::Step(row, dir) => edit(&mut res, &mut panel, |s| apply_step(s, row, dir)),
            Action::Activate(row) => edit(&mut res, &mut panel, |s| apply_step(s, row, 1)),
            Action::Reset => edit(&mut res, &mut panel, |s| *s = Settings::default()),
        }
    }
}

/// Writes pending edits at most once per `SAVE_INTERVAL_SECS`. Close also writes at once (see `panel_keys`).
fn save_settings(
    time: Res<Time>,
    mut panel: ResMut<PanelState>,
    res: Res<SettingsRes>,
    mut toasts: MessageWriter<Toast>,
) {
    if !panel.dirty {
        return;
    }
    panel.since_save += time.delta_secs();
    if panel.since_save < SAVE_INTERVAL_SECS {
        return;
    }
    write_now(&mut panel, &res, &mut toasts);
}

/// Shows or hides the panel. Rebuilds its content only when the tab, open state or settings change.
fn rebuild_panel(
    mut commands: Commands,
    panel: Res<PanelState>,
    res: Res<SettingsRes>,
    hidden: Res<HudHidden>,
    mut last_key: Local<Option<(bool, Tab)>>,
    mut roots: Query<(Entity, &mut Node), With<PanelRoot>>,
) {
    let key = (panel.open, panel.tab);
    let key_changed = *last_key != Some(key);
    let settings_changed = res.is_changed() && panel.open;
    if !key_changed && !settings_changed && !hidden.is_changed() {
        return;
    }
    *last_key = Some(key);

    let show = panel.open && !hidden.0;
    for (root, mut node) in &mut roots {
        let display = if show { Display::Flex } else { Display::None };
        if node.display != display {
            node.display = display;
        }
        if show && (key_changed || settings_changed) {
            commands.entity(root).despawn_related::<Children>();
            spawn_content(&mut commands, root, &panel, &res.value);
        }
    }
}

/// Registers the S-key toggle, the buttons, debounced saving and the panel rebuild.
/// Needs `SettingsRes` inserted and `HudPlugin` added (for `Toast` and `HudHidden`).
pub struct SettingsUiPlugin;

impl Plugin for SettingsUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PanelState>().add_systems(Startup, spawn_panel).add_systems(
            Update,
            (panel_keys, handle_buttons, save_settings, rebuild_panel).chain(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(tab: Tab) -> Vec<RowId> {
        rows(tab).iter().map(|r| r.id).collect()
    }

    #[test]
    fn basic_tab_holds_the_everyday_rows_in_order() {
        assert_eq!(
            ids(Tab::Basic),
            vec![
                RowId::Background,
                RowId::Lighting,
                RowId::Confidence,
                RowId::ShowPreview,
                RowId::ShowDebugDrawing,
                RowId::AutoBrightness
            ]
        );
    }

    #[test]
    fn advanced_tab_holds_tuning_rows_and_camera_is_read_only() {
        let adv = rows(Tab::Advanced);
        assert_eq!(adv.len(), 12);
        for spec in adv.iter().filter(|r| matches!(r.id, RowId::CameraSize | RowId::Mirror)) {
            assert_eq!(spec.kind, RowKind::ReadOnly);
        }
    }

    #[test]
    fn tab_key_switches_between_the_two_tabs() {
        assert_eq!(Tab::Basic.next(), Tab::Advanced);
        assert_eq!(Tab::Advanced.next(), Tab::Basic);
    }

    #[test]
    fn basic_confidence_steps_all_three_thresholds_by_five_percent() {
        let mut s = Settings::default();
        apply_step(&mut s, RowId::Confidence, 1);
        assert_eq!((s.face_confidence, s.body_confidence, s.hand_confidence), (0.85, 0.85, 0.85));
        assert_eq!(s.confidence, 0.85);
        apply_step(&mut s, RowId::Confidence, -1);
        assert_eq!(s.confidence, 0.8);
    }

    #[test]
    fn basic_confidence_stays_inside_its_range() {
        let mut s = Settings::default();
        s.set_all_confidence(0.99);
        apply_step(&mut s, RowId::Confidence, 1);
        assert_eq!(s.confidence, 0.99);
        s.set_all_confidence(0.3);
        apply_step(&mut s, RowId::Confidence, -1);
        assert_eq!(s.face_confidence, 0.3);
    }

    #[test]
    fn advanced_confidences_move_independently() {
        let mut s = Settings::default();
        apply_step(&mut s, RowId::BodyConfidence, 1);
        assert_eq!(s.body_confidence, 0.85);
        assert_eq!(s.face_confidence, 0.8);
        assert_eq!(s.hand_confidence, 0.8);
    }

    #[test]
    fn light_scales_step_by_tenth_and_clamp() {
        let mut s = Settings::default();
        apply_step(&mut s, RowId::KeyScale, 1);
        assert_eq!(s.key_scale, 1.1);
        s.fill_scale = 2.0;
        apply_step(&mut s, RowId::FillScale, 1);
        assert_eq!(s.fill_scale, 2.0);
        s.ambient_scale = 0.2;
        apply_step(&mut s, RowId::AmbientScale, -1);
        assert_eq!(s.ambient_scale, 0.2);
    }

    #[test]
    fn frame_cap_steps_by_five_and_clamps() {
        let mut s = Settings::default();
        apply_step(&mut s, RowId::FrameCap, 1);
        assert_eq!(s.frame_cap, 35);
        apply_step(&mut s, RowId::FrameCap, -1);
        apply_step(&mut s, RowId::FrameCap, -1);
        assert_eq!(s.frame_cap, 25);
        s.frame_cap = 120;
        apply_step(&mut s, RowId::FrameCap, 1);
        assert_eq!(s.frame_cap, 120);
        s.frame_cap = 15;
        apply_step(&mut s, RowId::FrameCap, -1);
        assert_eq!(s.frame_cap, 15);
    }

    #[test]
    fn gains_step_by_tenth_and_stop_at_zero() {
        let mut s = Settings::default();
        apply_step(&mut s, RowId::BrowGain, -1);
        assert_eq!(s.brow_gain, 0.9);
        s.gaze_gain = 0.0;
        apply_step(&mut s, RowId::GazeGain, -1);
        assert_eq!(s.gaze_gain, 0.0);
    }

    #[test]
    fn toggles_flip_and_read_only_rows_do_nothing() {
        let mut s = Settings::default();
        apply_step(&mut s, RowId::ShowPreview, 1);
        assert!(!s.show_preview);
        apply_step(&mut s, RowId::ShowPreview, 1);
        assert!(s.show_preview);
        apply_step(&mut s, RowId::Mirror, 1);
        apply_step(&mut s, RowId::CameraSize, 1);
        assert!(s.mirror);
        assert_eq!((s.camera_width, s.camera_height), (1280, 720));
    }

    #[test]
    fn background_cycles_forward_and_backward() {
        let mut s = Settings::default();
        apply_step(&mut s, RowId::Background, 1);
        assert_eq!(s.background, Background::Grey);
        s.background = Background::Dark;
        apply_step(&mut s, RowId::Background, -1);
        assert_eq!(s.background, Background::White);
    }

    #[test]
    fn antialiasing_cycles_forward_and_backward() {
        let mut s = Settings::default();
        apply_step(&mut s, RowId::Antialiasing, 1);
        assert_eq!(s.antialiasing, Antialiasing::X2);
        s.antialiasing = Antialiasing::Off;
        apply_step(&mut s, RowId::Antialiasing, -1);
        assert_eq!(s.antialiasing, Antialiasing::X4);
    }

    #[test]
    fn lighting_cycles_by_name_and_unknown_name_starts_at_studio() {
        let mut s = Settings::default();
        apply_step(&mut s, RowId::Lighting, 1);
        assert_eq!(s.lighting, "Soft");
        s.lighting = "Studio".into();
        apply_step(&mut s, RowId::Lighting, -1);
        assert_eq!(s.lighting, "Dramatic");
        s.lighting = "Bogus".into();
        apply_step(&mut s, RowId::Lighting, 1);
        assert_eq!(s.lighting, "Soft");
    }

    #[test]
    fn every_step_leaves_settings_clamped() {
        let rows_all: Vec<RowId> = ids(Tab::Basic).into_iter().chain(ids(Tab::Advanced)).collect();
        let mut s = Settings::default();
        for _ in 0..200 {
            for row in &rows_all {
                apply_step(&mut s, *row, 1);
                assert_eq!(s.clone().clamped(), s);
            }
        }
        for _ in 0..200 {
            for row in &rows_all {
                apply_step(&mut s, *row, -1);
                assert_eq!(s.clone().clamped(), s);
            }
        }
    }

    #[test]
    fn value_text_shows_percent_and_units() {
        let s = Settings::default();
        assert_eq!(value_text(&s, RowId::Confidence), "80%");
        assert_eq!(value_text(&s, RowId::FrameCap), "30 fps");
        assert_eq!(value_text(&s, RowId::KeyScale), "1.0");
        assert_eq!(value_text(&s, RowId::ShowPreview), "On");
        assert_eq!(value_text(&s, RowId::CameraSize), "1280 x 720");
    }

    #[test]
    fn preset_name_lookup_matches_names() {
        for p in [Preset::Studio, Preset::Soft, Preset::Flat, Preset::Dramatic] {
            assert_eq!(preset_from_name(p.name()), p);
        }
    }

    #[test]
    fn reset_gives_the_defaults() {
        let mut s = Settings::default();
        apply_step(&mut s, RowId::Background, 1);
        apply_step(&mut s, RowId::FrameCap, 1);
        s = Settings::default();
        assert_eq!(s, Settings::default());
    }
}
