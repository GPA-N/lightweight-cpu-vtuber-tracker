//! Shading overrides for VRM 1.0 MToon materials. The Shading settings are applied on top of each
//! model's own values while `shading_override` is on; with it off the model's values are restored.
//! Pure functions only. `settings_apply.rs` runs them over the loaded `MToonMaterial` assets.

use bevy::color::LinearRgba;
use bevy_vrm1::prelude::{MToonMaterial, MToonOutline, OutlineWidthMode, RimLighting, Shade};

use crate::settings::Settings;

/// Outline width that `outline_width` 1.0 gives when the model's own width is smaller. A thin line.
pub const OUTLINE_BASE_WIDTH: f32 = 0.002;

/// The shading values the settings change, as a model has them.
#[derive(Debug, Clone, PartialEq)]
pub struct ShadingValues {
    pub shade: Shade,
    pub outline: MToonOutline,
    pub rim_lighting: RimLighting,
    pub gi_equalization_factor: f32,
}

impl ShadingValues {
    /// Reads the shading values off a material.
    pub fn capture(m: &MToonMaterial) -> Self {
        Self {
            shade: m.shade,
            outline: m.outline.clone(),
            rim_lighting: m.rim_lighting,
            gi_equalization_factor: m.gi_equalization_factor,
        }
    }

    /// Writes the shading values onto a material.
    pub fn write(&self, m: &mut MToonMaterial) {
        m.shade = self.shade;
        m.outline = self.outline.clone();
        m.rim_lighting = self.rim_lighting;
        m.gi_equalization_factor = self.gi_equalization_factor;
    }
}

/// The values a material should have now. Override off gives the model's own values. Override on
/// gives the settings applied to them. `base` is the material's base colour, which the shade colour
/// blends toward at `shade_strength` 0.
pub fn target(original: &ShadingValues, base: LinearRgba, s: &Settings) -> ShadingValues {
    if !s.shading_override {
        return original.clone();
    }
    let mut v = original.clone();
    v.shade.color = mix(base, original.shade.color, s.shade_strength);
    v.shade.toony_factor = s.shade_toony;
    v.shade.shading_shift_factor = s.shade_shift;
    v.rim_lighting.mix_factor = original.rim_lighting.mix_factor * s.rim_strength;
    v.outline = outline(&original.outline, s.outline_width);
    v.gi_equalization_factor = s.gi_equalization;
    v
}

/// Outline for a scale: 0 turns it off. Above 0 it draws in world coordinates, width = the model's
/// own width (or `OUTLINE_BASE_WIDTH` when that is smaller) times the scale.
pub fn outline(original: &MToonOutline, scale: f32) -> MToonOutline {
    let mut o = original.clone();
    if scale > 0.0 {
        o.mode = OutlineWidthMode::WorldCoordinates;
        o.width_factor = original.width_factor.max(OUTLINE_BASE_WIDTH) * scale;
    } else {
        o.mode = OutlineWidthMode::None;
    }
    o
}

fn mix(a: LinearRgba, b: LinearRgba, t: f32) -> LinearRgba {
    LinearRgba {
        red: a.red * (1.0 - t) + b.red * t,
        green: a.green * (1.0 - t) + b.green * t,
        blue: a.blue * (1.0 - t) + b.blue * t,
        alpha: b.alpha,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model() -> ShadingValues {
        let mut m = MToonMaterial::default();
        m.shade.color = LinearRgba::rgb(0.5, 0.25, 0.1);
        m.shade.toony_factor = 0.6;
        m.rim_lighting.mix_factor = 0.8;
        m.outline.width_factor = 0.01;
        m.gi_equalization_factor = 0.4;
        ShadingValues::capture(&m)
    }

    fn base() -> LinearRgba {
        LinearRgba::rgb(0.2, 0.4, 0.6)
    }

    #[test]
    fn override_off_gives_the_model_values_whatever_the_settings() {
        let original = model();
        let mut s = Settings::default();
        s.shade_toony = 0.1;
        s.outline_width = 2.0;
        s.gi_equalization = 0.0;
        assert_eq!(target(&original, base(), &s), original);
    }

    #[test]
    fn override_on_with_default_settings_keeps_the_model_values() {
        let original = model();
        let mut s = Settings::default();
        s.shading_override = true;
        s.gi_equalization = original.gi_equalization_factor;
        s.shade_toony = original.shade.toony_factor;
        assert_eq!(target(&original, base(), &s), original);
    }

    #[test]
    fn shade_strength_blends_from_base_colour_to_model_shade() {
        let original = model();
        let mut s = Settings::default();
        s.shading_override = true;
        s.shade_strength = 0.0;
        assert_eq!(target(&original, base(), &s).shade.color, base());
        s.shade_strength = 1.0;
        assert_eq!(target(&original, base(), &s).shade.color, original.shade.color);
    }

    #[test]
    fn toony_shift_gi_and_rim_are_applied() {
        let original = model();
        let mut s = Settings::default();
        s.shading_override = true;
        s.shade_toony = 0.3;
        s.shade_shift = -0.5;
        s.rim_strength = 0.5;
        s.gi_equalization = 1.0;
        let v = target(&original, base(), &s);
        assert_eq!(v.shade.toony_factor, 0.3);
        assert_eq!(v.shade.shading_shift_factor, -0.5);
        assert!((v.rim_lighting.mix_factor - 0.4).abs() < 1e-6);
        assert_eq!(v.gi_equalization_factor, 1.0);
    }

    #[test]
    fn outline_zero_is_no_outline() {
        let o = outline(&model().outline, 0.0);
        assert_eq!(o.mode, OutlineWidthMode::None);
    }

    #[test]
    fn outline_scales_the_models_own_width() {
        let o = outline(&model().outline, 2.0);
        assert_eq!(o.mode, OutlineWidthMode::WorldCoordinates);
        assert!((o.width_factor - 0.02).abs() < 1e-6);
    }

    #[test]
    fn outline_uses_base_width_when_model_has_none() {
        let mut thin = model().outline;
        thin.width_factor = 0.0;
        let o = outline(&thin, 1.0);
        assert_eq!(o.mode, OutlineWidthMode::WorldCoordinates);
        assert!((o.width_factor - OUTLINE_BASE_WIDTH).abs() < 1e-9);
    }

    #[test]
    fn write_then_capture_round_trips() {
        let original = model();
        let mut s = Settings::default();
        s.shading_override = true;
        s.outline_width = 1.5;
        let want = target(&original, base(), &s);
        let mut m = MToonMaterial::default();
        want.write(&mut m);
        assert_eq!(ShadingValues::capture(&m), want);
    }

    #[test]
    fn turning_override_off_restores_the_originals() {
        let original = model();
        let mut s = Settings::default();
        s.shading_override = true;
        s.shade_toony = 0.1;
        let mut m = MToonMaterial::default();
        target(&original, base(), &s).write(&mut m);
        s.shading_override = false;
        target(&original, base(), &s).write(&mut m);
        assert_eq!(ShadingValues::capture(&m), original);
    }
}
