//! Viewer HUD: bottom-centre toast messages, a top-left help overlay (H),
//! and a focus gate (`HudHidden`) that hides every HUD node.
//!
//! Nodes are spawned once at startup. Systems only write to components when
//! state changes, so an idle HUD does no per-frame work beyond the cheap
//! queries that find nothing to do.

use bevy::prelude::*;

/// Seconds a toast stays on screen.
const TOAST_LIFETIME_SECS: f32 = 4.0;
/// Seconds into a toast's life at which the fade-out starts.
const TOAST_FADE_START_SECS: f32 = 3.0;

const HELP_TEXT: &str = "Keys\n\
H  toggle this help\n\
F  focus mode (hide panel)\n\
L  lighting preset\n\
Ctrl+O  open model\n\
Drop a .vrm file on the window";

/// Severity of a toast. Controls text colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastLevel {
    /// Neutral status message.
    Info,
    /// Something was skipped or partly supported.
    Warn,
    /// Something failed.
    Error,
}

/// Short message shown bottom-centre for about 4 seconds.
///
/// Send with `MessageWriter<Toast>`.
#[derive(Message, Debug, Clone, PartialEq)]
pub struct Toast {
    /// Text to show.
    pub text: String,
    /// Severity, sets the colour.
    pub level: ToastLevel,
}

/// When true, every HUD node is hidden and HUD systems skip work.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq)]
pub struct HudHidden(pub bool);

/// Plugin that spawns the toast stack and help overlay.
pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HudHidden>()
            .init_resource::<HelpOpen>()
            .add_message::<Toast>()
            .add_systems(Startup, spawn_hud)
            .add_systems(
                Update,
                (toggle_help, apply_visibility, spawn_toasts, fade_toasts).chain(),
            );
    }
}

/// Whether the help overlay is open.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq)]
struct HelpOpen(bool);

/// Marks every HUD root node so the focus gate can hide them all.
#[derive(Component)]
struct HudRoot;

/// Marks the help overlay root.
#[derive(Component)]
struct HelpRoot;

/// Marks the container that toasts are parented to.
#[derive(Component)]
struct ToastRoot;

/// Remaining age of a toast in seconds.
#[derive(Component)]
struct ToastLifetime {
    age: f32,
    level: ToastLevel,
}

/// Opacity for a toast of the given age, from 1.0 down to 0.0.
///
/// Full opacity until `TOAST_FADE_START_SECS`, then linear fade to zero at
/// `TOAST_LIFETIME_SECS`.
pub fn toast_alpha(age_secs: f32) -> f32 {
    if age_secs >= TOAST_LIFETIME_SECS {
        0.0
    } else if age_secs <= TOAST_FADE_START_SECS {
        1.0
    } else {
        (TOAST_LIFETIME_SECS - age_secs) / (TOAST_LIFETIME_SECS - TOAST_FADE_START_SECS)
    }
}

/// Display value for the help overlay given focus and open state.
pub fn help_display(hidden: bool, open: bool) -> Display {
    if !hidden && open {
        Display::Flex
    } else {
        Display::None
    }
}

/// RGB for a toast level: error red, warn amber, info neutral.
pub fn toast_rgb(level: ToastLevel) -> [f32; 3] {
    match level {
        ToastLevel::Info => [0.88, 0.90, 0.93],
        ToastLevel::Warn => [1.00, 0.75, 0.25],
        ToastLevel::Error => [0.98, 0.36, 0.36],
    }
}

fn with_alpha(rgb: [f32; 3], alpha: f32) -> Color {
    Color::srgba(rgb[0], rgb[1], rgb[2], alpha)
}

const PANEL_BG: [f32; 3] = [0.06, 0.06, 0.08];

fn spawn_hud(mut commands: Commands) {
    commands.spawn((
        HudRoot,
        HelpRoot,
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(16.0),
            left: Val::Px(16.0),
            padding: UiRect::all(Val::Px(12.0)),
            display: Display::None,
            ..default()
        },
        BackgroundColor(with_alpha(PANEL_BG, 0.85)),
        GlobalZIndex(10),
        children![(
            Text::new(HELP_TEXT),
            TextFont::from_font_size(14.0),
            TextColor(with_alpha(toast_rgb(ToastLevel::Info), 1.0)),
        )],
    ));

    commands.spawn((
        HudRoot,
        ToastRoot,
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            right: Val::Px(0.0),
            bottom: Val::Px(24.0),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            ..default()
        },
        GlobalZIndex(20),
    ));
}

fn toggle_help(keys: Res<ButtonInput<KeyCode>>, mut open: ResMut<HelpOpen>) {
    if keys.just_pressed(KeyCode::KeyH) {
        open.0 = !open.0;
    }
}

fn apply_visibility(
    hidden: Res<HudHidden>,
    open: Res<HelpOpen>,
    mut roots: Query<(&mut Node, Has<HelpRoot>), With<HudRoot>>,
) {
    if !hidden.is_changed() && !open.is_changed() {
        return;
    }
    for (mut node, is_help) in &mut roots {
        let display = if is_help {
            help_display(hidden.0, open.0)
        } else if hidden.0 {
            Display::None
        } else {
            Display::Flex
        };
        if node.display != display {
            node.display = display;
        }
    }
}

fn spawn_toasts(
    mut commands: Commands,
    hidden: Res<HudHidden>,
    mut toasts: MessageReader<Toast>,
    roots: Query<Entity, With<ToastRoot>>,
) {
    if hidden.0 {
        return;
    }
    let Ok(root) = roots.single() else {
        return;
    };
    for toast in toasts.read() {
        let rgb = toast_rgb(toast.level);
        commands.spawn((
            ChildOf(root),
            Node {
                padding: UiRect::axes(Val::Px(14.0), Val::Px(8.0)),
                margin: UiRect::top(Val::Px(6.0)),
                ..default()
            },
            BackgroundColor(with_alpha(PANEL_BG, 0.85)),
            Text::new(toast.text.clone()),
            TextFont::from_font_size(16.0),
            TextColor(with_alpha(rgb, 1.0)),
            TextLayout::new(Justify::Center, LineBreak::WordBoundary),
            ToastLifetime {
                age: 0.0,
                level: toast.level,
            },
        ));
    }
}

fn fade_toasts(
    mut commands: Commands,
    hidden: Res<HudHidden>,
    time: Res<Time>,
    mut toasts: Query<(
        Entity,
        &mut ToastLifetime,
        &mut BackgroundColor,
        &mut TextColor,
    )>,
) {
    if hidden.0 {
        return;
    }
    let dt = time.delta_secs();
    for (entity, mut life, mut bg, mut text) in &mut toasts {
        life.age += dt;
        if life.age >= TOAST_LIFETIME_SECS {
            commands.entity(entity).despawn();
            continue;
        }
        let alpha = toast_alpha(life.age);
        let bg_color = with_alpha(PANEL_BG, 0.85 * alpha);
        let text_color = with_alpha(toast_rgb(life.level), alpha);
        if bg.0 != bg_color {
            bg.0 = bg_color;
        }
        if text.0 != text_color {
            text.0 = text_color;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toast_alpha_full_before_fade() {
        assert_eq!(toast_alpha(0.0), 1.0);
        assert_eq!(toast_alpha(TOAST_FADE_START_SECS), 1.0);
    }

    #[test]
    fn toast_alpha_fades_linearly() {
        let mid = (TOAST_FADE_START_SECS + TOAST_LIFETIME_SECS) / 2.0;
        assert!((toast_alpha(mid) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn toast_alpha_zero_at_and_after_lifetime() {
        assert_eq!(toast_alpha(TOAST_LIFETIME_SECS), 0.0);
        assert_eq!(toast_alpha(TOAST_LIFETIME_SECS + 1.0), 0.0);
    }

    #[test]
    fn help_hidden_when_focus_or_closed() {
        assert_eq!(help_display(false, true), Display::Flex);
        assert_eq!(help_display(false, false), Display::None);
        assert_eq!(help_display(true, true), Display::None);
    }

    #[test]
    fn error_is_red_and_distinct_from_warn() {
        let [r, g, _] = toast_rgb(ToastLevel::Error);
        assert!(r > 0.8 && g < 0.5);
        assert_ne!(toast_rgb(ToastLevel::Error), toast_rgb(ToastLevel::Warn));
        assert_ne!(toast_rgb(ToastLevel::Info), toast_rgb(ToastLevel::Warn));
    }

    #[test]
    fn hud_hidden_defaults_to_visible() {
        assert_eq!(HudHidden::default(), HudHidden(false));
    }
}
