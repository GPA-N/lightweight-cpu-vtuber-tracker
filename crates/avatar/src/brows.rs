//! Direct eyebrow control for models that ship eyebrow morph targets without VRM expressions for
//! them (Seed-san has `eye_brow_down`, `eye_brow_up` and left/right versions). The tracked ARKit brow
//! shapes are written straight to those morph weights after the expression pass, which would
//! otherwise reset them. Models without such targets are unaffected and keep the emotion mapping
//! (angry / surprised) from the mapping table.

use bevy::mesh::morph::MorphWeights;
use bevy::prelude::*;
use bevy_vrm1::prelude::*;

use crate::settings_apply::Gains;

/// Share of the brow-down reading removed at a full smile.
const SMILE_BROW_CANCEL: f32 = 0.7;

/// Brow amounts in 0..=1, already in avatar left/right terms (the tracker mirrors left and right).
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq)]
pub struct BrowInput {
    pub down_left: f32,
    pub down_right: f32,
    pub up_left: f32,
    pub up_right: f32,
    /// Mouth shapes for models with lip morphs (`mouth_wide`, `lip_o`, `lip_u`, `lip_up`, `lip_down`).
    pub wide: f32,
    pub funnel: f32,
    pub pucker: f32,
    pub lip_up: f32,
    pub lip_down: f32,
}

/// Brow input from the 52 ARKit weights: inner-up and outer-up both raise the brow.
pub fn brows_from_weights(get: impl Fn(&str) -> f32) -> BrowInput {
    let inner = get("browInnerUp");
    // The face model reads the cheek raise of a smile as lowered brows, which makes a smiling face
    // look angry on the avatar. A strong smile therefore cancels part of the brow-down reading.
    let smile = (get("mouthSmileLeft") + get("mouthSmileRight")) / 2.0;
    let keep = 1.0 - SMILE_BROW_CANCEL * smile.clamp(0.0, 1.0);
    BrowInput {
        down_left: (get("browDownLeft") * keep).clamp(0.0, 1.0),
        down_right: (get("browDownRight") * keep).clamp(0.0, 1.0),
        up_left: inner.max(get("browOuterUpLeft")).clamp(0.0, 1.0),
        up_right: inner.max(get("browOuterUpRight")).clamp(0.0, 1.0),
        wide: ((get("mouthStretchLeft") + get("mouthStretchRight")) / 2.0).clamp(0.0, 1.0),
        funnel: get("mouthFunnel").clamp(0.0, 1.0),
        pucker: get("mouthPucker").clamp(0.0, 1.0),
        lip_up: ((get("mouthUpperUpLeft") + get("mouthUpperUpRight")) / 2.0).clamp(0.0, 1.0),
        lip_down: ((get("mouthLowerDownLeft") + get("mouthLowerDownRight")) / 2.0).clamp(0.0, 1.0),
    }
}

/// Weight for a morph target name, or None when it is not a brow target. Names look like
/// `eye_brow_down_L`; the unsided `eye_brow_down` takes the mean of both sides.
pub fn brow_weight(name: &str, b: &BrowInput) -> Option<f32> {
    let n = name.to_ascii_lowercase();
    match n.as_str() {
        "mouth_wide" => return Some(b.wide),
        "lip_o" => return Some(b.funnel),
        "lip_u" => return Some(b.pucker),
        "lip_up" => return Some(b.lip_up),
        "lip_down" => return Some(b.lip_down),
        _ => {}
    }
    if !n.contains("brow") {
        return None;
    }
    let down = n.contains("down");
    let up = n.contains("up");
    if down == up {
        return None;
    }
    let (l, r) = if down { (b.down_left, b.down_right) } else { (b.up_left, b.up_right) };
    if n.ends_with("_l") || n.ends_with(".l") || n.ends_with("left") {
        Some(l)
    } else if n.ends_with("_r") || n.ends_with(".r") || n.ends_with("right") {
        Some(r)
    } else {
        Some((l + r) / 2.0)
    }
}

/// Eyebrow weight after the brow gain. Clamped to 0..=1 so a gain above 1 only pushes a brow to its
/// full value. Mouth shapes do not pass through here.
pub fn scale_brow(weight: f32, gain: f32) -> f32 {
    (weight * gain).clamp(0.0, 1.0)
}

fn apply_brows(
    input: Res<BrowInput>,
    gains: Res<Gains>,
    meshes: Res<Assets<Mesh>>,
    mut weights: Query<&mut MorphWeights>,
) {
    for mut morph in &mut weights {
        let Some(handle) = morph.first_mesh() else { continue };
        let Some(names) = meshes.get(handle).and_then(|m| m.morph_target_names()) else { continue };
        let values: Vec<(usize, f32)> = names
            .iter()
            .enumerate()
            .filter_map(|(i, n)| {
                let w = brow_weight(n, &input)?;
                let is_brow = n.to_ascii_lowercase().contains("brow");
                Some((i, if is_brow { scale_brow(w, gains.brow) } else { w }))
            })
            .collect();
        let slots = morph.weights_mut();
        for (i, w) in values {
            if let Some(slot) = slots.get_mut(i) {
                *slot = w;
            }
        }
    }
}

pub struct BrowPlugin;

impl Plugin for BrowPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BrowInput>().init_resource::<Gains>().add_systems(
            PostUpdate,
            apply_brows.after(VrmSystemSets::Expressions).before(VrmSystemSets::PropagateAfterExpressions),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brow_weights_follow_side_and_direction() {
        let b = BrowInput { down_left: 0.8, down_right: 0.2, up_left: 0.1, up_right: 0.6, wide: 0.4, ..Default::default() };
        assert_eq!(brow_weight("mouth_wide", &b), Some(0.4));
        assert_eq!(brow_weight("eye_brow_down_L", &b), Some(0.8));
        assert_eq!(brow_weight("eye_brow_down_R", &b), Some(0.2));
        assert_eq!(brow_weight("eye_brow_up_R", &b), Some(0.6));
        assert!((brow_weight("eye_brow_up", &b).unwrap() - 0.35).abs() < 1e-6);
        assert_eq!(brow_weight("mouth_aa", &b), None);
        assert_eq!(brow_weight("brow", &b), None, "no direction in the name");
    }

    #[test]
    fn a_smile_cancels_part_of_the_brow_down_reading() {
        let frown = brows_from_weights(|n| if n == "browDownLeft" { 0.8 } else { 0.0 });
        let smiling = brows_from_weights(|n| match n {
            "browDownLeft" => 0.8,
            "mouthSmileLeft" | "mouthSmileRight" => 1.0,
            _ => 0.0,
        });
        assert!((frown.down_left - 0.8).abs() < 1e-6);
        assert!((smiling.down_left - 0.8 * (1.0 - SMILE_BROW_CANCEL)).abs() < 1e-6);
    }

    #[test]
    fn inner_up_or_outer_up_raises_a_brow() {
        let b = brows_from_weights(|n| match n {
            "browInnerUp" => 0.3,
            "browOuterUpRight" => 0.9,
            "browDownLeft" => 1.4,
            _ => 0.0,
        });
        assert_eq!(b.up_left, 0.3);
        assert_eq!(b.up_right, 0.9);
        assert_eq!(b.down_left, 1.0);
        assert_eq!(b.down_right, 0.0);
    }

    #[test]
    fn brow_gain_scales_then_clamps_to_unit_range() {
        assert_eq!(scale_brow(0.5, 1.0), 0.5, "unit gain is identity");
        assert_eq!(scale_brow(0.5, 0.0), 0.0, "zero gain shuts the brow");
        assert_eq!(scale_brow(0.4, 2.0), 0.8);
        assert_eq!(scale_brow(0.8, 2.0), 1.0, "over-driven brow clamps to 1");
    }
}
