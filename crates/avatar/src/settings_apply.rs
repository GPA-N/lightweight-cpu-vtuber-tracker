//! Applies the live options from `SettingsRes` to the running window. Every apply system runs only
//! when `SettingsRes` changed (`resource_changed`), so an idle frame does no work. Restart-only
//! options (camera size, mirror) are not read here.
//!
//! Brow and gaze gains live in `Gains`, which brows.rs and gaze.rs read. Each of those plugins
//! also registers `Gains`, so they run without this plugin (at the default 1.0).

use std::sync::Arc;
use std::time::Duration;

use bevy::camera::ClearColor;
use bevy::prelude::*;
use bevy::render::view::Msaa;
use bevy::winit::{UpdateMode, WinitSettings};

use crate::lighting::{LightScales, LightingPreset, Preset};
use crate::settings::{Antialiasing, Background, LiveTuning, Settings, SettingsRes, FRAME_CAP_RANGE};

/// Multipliers read by the brow and gaze systems. 1.0 keeps the tracked value.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct Gains {
    pub brow: f32,
    pub gaze: f32,
}

impl Default for Gains {
    fn default() -> Self {
        Self { brow: 1.0, gaze: 1.0 }
    }
}

/// The tracker threads' settings, shared by `Arc`. The window inserts it at startup.
#[derive(Resource, Clone)]
pub struct LiveTuningRes(pub Arc<LiveTuning>);

/// Time between frames when idle: 1000 / frame_cap ms, with the cap held to `FRAME_CAP_RANGE`.
pub fn frame_wait(cap: u32) -> Duration {
    let cap = cap.clamp(FRAME_CAP_RANGE.0, FRAME_CAP_RANGE.1);
    Duration::from_micros(1_000_000 / cap as u64)
}

/// Clear colour behind the avatar.
pub fn clear_color(background: Background) -> Color {
    let [r, g, b] = background.rgb();
    Color::srgb(r, g, b)
}

/// Multisample level for the camera.
pub fn msaa_for(antialiasing: Antialiasing) -> Msaa {
    match antialiasing {
        Antialiasing::Off => Msaa::Off,
        Antialiasing::X2 => Msaa::Sample2,
        Antialiasing::X4 => Msaa::Sample4,
    }
}

/// Light multipliers from the settings.
pub fn light_scales(s: &Settings) -> LightScales {
    LightScales { key: s.key_scale, fill: s.fill_scale, ambient: s.ambient_scale }
}

/// Brow and gaze gains from the settings.
pub fn gains(s: &Settings) -> Gains {
    Gains { brow: s.brow_gain, gaze: s.gaze_gain }
}

fn apply_background(settings: Res<SettingsRes>, mut clear: ResMut<ClearColor>) {
    clear.0 = clear_color(settings.value.background);
}

fn apply_lighting(settings: Res<SettingsRes>, mut preset: ResMut<LightingPreset>, mut scales: ResMut<LightScales>) {
    let want_preset = Preset::from_name(&settings.value.lighting).unwrap_or_default();
    if preset.0 != want_preset {
        preset.0 = want_preset;
    }
    let want_scales = light_scales(&settings.value);
    if *scales != want_scales {
        *scales = want_scales;
    }
}

fn apply_antialiasing(
    settings: Res<SettingsRes>,
    mut commands: Commands,
    cameras: Query<(Entity, Option<&Msaa>), With<Camera3d>>,
) {
    let want = msaa_for(settings.value.antialiasing);
    for (camera, current) in &cameras {
        if current.copied() != Some(want) {
            commands.entity(camera).insert(want);
        }
    }
}

fn apply_frame_cap(settings: Res<SettingsRes>, mut winit: ResMut<WinitSettings>) {
    let wait = frame_wait(settings.value.frame_cap);
    winit.focused_mode = UpdateMode::reactive(wait);
    winit.unfocused_mode = UpdateMode::reactive(wait);
}

fn apply_gains(settings: Res<SettingsRes>, mut gains_res: ResMut<Gains>) {
    let want = gains(&settings.value);
    if *gains_res != want {
        *gains_res = want;
    }
}

fn apply_live_tuning(settings: Res<SettingsRes>, tuning: Option<Res<LiveTuningRes>>) {
    if let Some(tuning) = tuning {
        tuning.0.apply(&settings.value);
    }
}

pub struct SettingsApplyPlugin;

impl Plugin for SettingsApplyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Gains>().add_systems(
            Update,
            (
                apply_background,
                apply_lighting,
                apply_antialiasing,
                apply_frame_cap,
                apply_gains,
                apply_live_tuning,
            )
                .run_if(resource_changed::<SettingsRes>),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{GAIN_RANGE, LIGHT_SCALE_RANGE};

    #[test]
    fn frame_wait_is_one_second_over_cap() {
        assert_eq!(frame_wait(30), Duration::from_micros(33_333));
        assert_eq!(frame_wait(60), Duration::from_micros(16_666));
        assert_eq!(frame_wait(120), Duration::from_micros(8_333));
    }

    #[test]
    fn frame_wait_clamps_cap_to_range() {
        assert_eq!(frame_wait(0), frame_wait(FRAME_CAP_RANGE.0));
        assert_eq!(frame_wait(10_000), frame_wait(FRAME_CAP_RANGE.1));
    }

    #[test]
    fn background_maps_to_its_rgb() {
        for b in Background::ALL {
            let [r, g, bl] = b.rgb();
            assert_eq!(clear_color(b), Color::srgb(r, g, bl));
        }
    }

    #[test]
    fn antialiasing_maps_to_sample_counts() {
        assert_eq!(msaa_for(Antialiasing::Off), Msaa::Off);
        assert_eq!(msaa_for(Antialiasing::X2), Msaa::Sample2);
        assert_eq!(msaa_for(Antialiasing::X4), Msaa::Sample4);
    }

    #[test]
    fn default_settings_give_unit_scales_and_gains() {
        let s = Settings::default();
        assert_eq!(light_scales(&s), LightScales::default());
        assert_eq!(gains(&s), Gains::default());
        assert_eq!(Gains::default(), Gains { brow: 1.0, gaze: 1.0 });
    }

    #[test]
    fn scales_and_gains_copy_their_settings() {
        let mut s = Settings::default();
        s.key_scale = 0.5;
        s.fill_scale = 1.5;
        s.ambient_scale = 2.0;
        s.brow_gain = 0.25;
        s.gaze_gain = 1.75;
        assert_eq!(light_scales(&s), LightScales { key: 0.5, fill: 1.5, ambient: 2.0 });
        assert_eq!(gains(&s), Gains { brow: 0.25, gaze: 1.75 });
    }

    #[test]
    fn default_gain_and_scale_ranges_include_unit() {
        assert!(GAIN_RANGE.0 <= 1.0 && 1.0 <= GAIN_RANGE.1);
        assert!(LIGHT_SCALE_RANGE.0 <= 1.0 && 1.0 <= LIGHT_SCALE_RANGE.1);
    }
}
