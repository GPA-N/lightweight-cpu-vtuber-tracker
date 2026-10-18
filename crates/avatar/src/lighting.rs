//! Lighting presets for the avatar window, cycled with the L key. Each preset sets the key and fill
//! directional lights and the global ambient brightness. The values are pure data (`preset_values`)
//! so they can be unit-tested; the systems only write them when the preset changes.
//!
//! Shadows stay off in every preset. Shadow maps re-render the scene once per cascade, which costs
//! CPU and GPU time every frame, and the avatar on its plain backdrop gains little from a cast shadow.
//! Presets therefore vary light level, colour and balance only.
//!
//! Verified in Bevy 0.19.1 source but NOT added (each is a choice for the lead or the user):
//! - Tonemapping: `bevy_core_pipeline::tonemapping::Tonemapping` is a component on the `Camera`
//!   entity (`Camera3d` spawn in window.rs). Changing it shifts every preset's look at once, so it
//!   needs live judgement first.
//! - SMAA: `bevy_anti_alias::smaa::Smaa`, and FXAA: `bevy_anti_alias::fxaa::Fxaa`. Both are camera
//!   components that add a full-screen pass. Not verified: any cargo feature they need.

use bevy::prelude::*;

/// Which light a directional light entity is. window.rs adds one to each existing light.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum LightRole {
    Key,
    Fill,
}

/// The lighting look shown in the window. Studio is the default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Preset {
    #[default]
    Studio,
    Soft,
    Flat,
    Dramatic,
}

impl Preset {
    /// The next preset in the L-key cycle. Dramatic wraps back to Studio.
    pub fn next(self) -> Preset {
        match self {
            Preset::Studio => Preset::Soft,
            Preset::Soft => Preset::Flat,
            Preset::Flat => Preset::Dramatic,
            Preset::Dramatic => Preset::Studio,
        }
    }

    /// Short display name, used in the toast text.
    pub fn name(self) -> &'static str {
        match self {
            Preset::Studio => "Studio",
            Preset::Soft => "Soft",
            Preset::Flat => "Flat",
            Preset::Dramatic => "Dramatic",
        }
    }

    /// Preset for a saved name (as written by `name`). None for unknown names.
    pub fn from_name(name: &str) -> Option<Preset> {
        match name {
            "Studio" => Some(Preset::Studio),
            "Soft" => Some(Preset::Soft),
            "Flat" => Some(Preset::Flat),
            "Dramatic" => Some(Preset::Dramatic),
            _ => None,
        }
    }
}

/// The active preset. A change to it re-applies the lights.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct LightingPreset(pub Preset);

/// Multipliers on the preset values, from the settings panel. 1.0 keeps the preset as it is.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct LightScales {
    pub key: f32,
    pub fill: f32,
    pub ambient: f32,
}

impl Default for LightScales {
    fn default() -> Self {
        Self { key: 1.0, fill: 1.0, ambient: 1.0 }
    }
}

/// Multiplies each light value of a preset by its scale.
pub fn scale_values(v: LightValues, s: LightScales) -> LightValues {
    LightValues {
        key_lux: v.key_lux * s.key,
        key_color: v.key_color,
        fill_lux: v.fill_lux * s.fill,
        fill_color: v.fill_color,
        ambient_brightness: v.ambient_brightness * s.ambient,
    }
}

/// Values one preset writes to the lights. Lux for the directional lights, brightness for ambient.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LightValues {
    pub key_lux: f32,
    pub key_color: Color,
    pub fill_lux: f32,
    pub fill_color: Color,
    pub ambient_brightness: f32,
}

/// The light values for a preset. Studio keeps the values window.rs used before this module existed.
/// Soft lowers the key and raises fill and ambient. Flat makes key and fill equal and near-neutral.
/// Dramatic keeps the key at 10,000 lux, the low end of full daylight, with a weak cool fill.
/// All values are first guesses; the user judges them live.
pub fn preset_values(preset: Preset) -> LightValues {
    let warm = Color::srgb(1.0, 0.96, 0.9);
    let cool = Color::srgb(0.85, 0.92, 1.0);
    match preset {
        Preset::Studio => LightValues {
            key_lux: 9_000.0,
            key_color: warm,
            fill_lux: 3_000.0,
            fill_color: cool,
            ambient_brightness: 200.0,
        },
        Preset::Soft => LightValues {
            key_lux: 5_000.0,
            key_color: warm,
            fill_lux: 4_000.0,
            fill_color: Color::srgb(0.9, 0.94, 1.0),
            ambient_brightness: 400.0,
        },
        Preset::Flat => LightValues {
            key_lux: 800.0,
            key_color: Color::srgb(1.0, 0.98, 0.95),
            fill_lux: 800.0,
            fill_color: Color::srgb(1.0, 1.0, 1.0),
            ambient_brightness: 600.0,
        },
        Preset::Dramatic => LightValues {
            key_lux: 10_000.0,
            key_color: warm,
            fill_lux: 900.0,
            fill_color: cool,
            ambient_brightness: 80.0,
        },
    }
}

/// Label for the toast when the preset changes. The lead wires it to the HUD toast.
pub fn preset_label(preset: Preset) -> String {
    format!("Lighting: {}", preset.name())
}

/// L press moves to the next preset. Only writes the resource on a press, so `is_changed` stays false otherwise.
fn cycle_preset(keys: Res<ButtonInput<KeyCode>>, mut preset: ResMut<LightingPreset>) {
    if keys.just_pressed(KeyCode::KeyL) {
        preset.0 = preset.0.next();
    }
}

/// Writes the preset to the key, fill and ambient lights. Runs only when the preset or the scales
/// changed or a new role light appeared, so an idle frame does no light writes.
fn apply_preset(
    preset: Res<LightingPreset>,
    scales: Res<LightScales>,
    added: Query<(), Added<LightRole>>,
    mut lights: Query<(&LightRole, &mut DirectionalLight)>,
    mut ambient: ResMut<GlobalAmbientLight>,
) {
    if !preset.is_changed() && !scales.is_changed() && added.is_empty() {
        return;
    }
    let values = scale_values(preset_values(preset.0), *scales);
    for (role, mut light) in &mut lights {
        let (lux, color) = match role {
            LightRole::Key => (values.key_lux, values.key_color),
            LightRole::Fill => (values.fill_lux, values.fill_color),
        };
        light.illuminance = lux;
        light.color = color;
    }
    ambient.brightness = values.ambient_brightness;
}

/// Registers the L-key cycle and the preset apply. Added once in `build_app`.
pub struct LightingPlugin;

impl Plugin for LightingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LightingPreset>()
            .init_resource::<LightScales>()
            .add_systems(Update, (cycle_preset, apply_preset).chain());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Preset; 4] = [Preset::Studio, Preset::Soft, Preset::Flat, Preset::Dramatic];

    #[test]
    fn next_cycles_through_all_presets_and_wraps() {
        let mut p = Preset::Studio;
        let mut seen = Vec::new();
        for _ in 0..4 {
            seen.push(p);
            p = p.next();
        }
        assert_eq!(p, Preset::Studio, "four presses return to Studio");
        assert_eq!(seen, ALL.to_vec());
    }

    #[test]
    fn names_are_distinct_and_label_carries_name() {
        let names: Vec<&str> = ALL.iter().map(|p| p.name()).collect();
        for (i, a) in names.iter().enumerate() {
            assert!(!a.is_empty());
            for b in &names[i + 1..] {
                assert_ne!(a, b);
            }
        }
        assert_eq!(preset_label(Preset::Soft), "Lighting: Soft");
    }

    #[test]
    fn studio_matches_the_original_window_values() {
        let v = preset_values(Preset::Studio);
        assert_eq!(v.key_lux, 9_000.0);
        assert_eq!(v.key_color, Color::srgb(1.0, 0.96, 0.9));
        assert_eq!(v.fill_lux, 3_000.0);
        assert_eq!(v.fill_color, Color::srgb(0.85, 0.92, 1.0));
        assert_eq!(v.ambient_brightness, 200.0);
        assert_eq!(Preset::default(), Preset::Studio);
    }

    #[test]
    fn from_name_round_trips_every_preset_and_rejects_unknown() {
        for p in ALL {
            assert_eq!(Preset::from_name(p.name()), Some(p));
        }
        assert_eq!(Preset::from_name("studio"), None, "names are exact");
        assert_eq!(Preset::from_name(""), None);
    }

    #[test]
    fn unit_scales_keep_preset_values() {
        let v = preset_values(Preset::Soft);
        assert_eq!(scale_values(v, LightScales::default()), v);
    }

    #[test]
    fn scales_multiply_key_fill_and_ambient_only() {
        let v = preset_values(Preset::Studio);
        let s = LightScales { key: 0.5, fill: 2.0, ambient: 1.5 };
        let out = scale_values(v, s);
        assert_eq!(out.key_lux, 4_500.0);
        assert_eq!(out.fill_lux, 6_000.0);
        assert_eq!(out.ambient_brightness, 300.0);
        assert_eq!(out.key_color, v.key_color);
        assert_eq!(out.fill_color, v.fill_color);
    }

    #[test]
    fn soft_has_lower_key_and_brighter_fill_and_ambient_than_studio() {
        let s = preset_values(Preset::Studio);
        let v = preset_values(Preset::Soft);
        assert!(v.key_lux < s.key_lux);
        assert!(v.fill_lux > s.fill_lux);
        assert!(v.ambient_brightness > s.ambient_brightness);
    }

    #[test]
    fn flat_uses_equal_key_and_fill_with_neutral_fill() {
        let v = preset_values(Preset::Flat);
        assert_eq!(v.key_lux, v.fill_lux);
        assert_eq!(v.fill_color, Color::srgb(1.0, 1.0, 1.0));
    }

    #[test]
    fn dramatic_has_strong_key_and_weak_fill() {
        let s = preset_values(Preset::Studio);
        let v = preset_values(Preset::Dramatic);
        assert!(v.key_lux > s.key_lux);
        assert!(v.fill_lux < s.fill_lux);
        assert!(v.ambient_brightness < s.ambient_brightness);
        assert!(v.fill_lux > 0.0, "shaded side keeps some light so skin is not black");
    }

    #[test]
    fn every_preset_has_sane_values() {
        for p in ALL {
            let v = preset_values(p);
            assert!(v.key_lux > 0.0 && v.key_lux <= 10_000.0, "{p:?} key clips above daylight");
            assert!(v.fill_lux > 0.0 && v.fill_lux <= v.key_lux, "{p:?} fill above key");
            assert!(v.ambient_brightness > 0.0, "{p:?} ambient off");
            for c in [v.key_color, v.fill_color] {
                let s = c.to_srgba();
                for ch in [s.red, s.green, s.blue] {
                    assert!((0.0..=1.0).contains(&ch), "{p:?} colour out of range");
                }
            }
        }
    }
}
