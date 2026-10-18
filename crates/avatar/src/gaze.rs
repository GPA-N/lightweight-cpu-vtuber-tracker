//! Eye gaze for VRMs whose lookAt type is `bone` (for example the Twist sample): a target is kept in
//! front of the face and moved from the tracked look-left/right/up/down values, and bevy_vrm1's
//! `LookAt` turns the eye bones towards it. Models whose lookAt type is `expression` (Seed-san) take
//! gaze through the `lookLeft` / `lookRight` / `lookUp` / `lookDown` expressions in the mapping table.

use bevy::prelude::*;
use bevy_vrm1::prelude::*;

use crate::settings_apply::Gains;

type LookAtRoots<'w, 's> = Query<'w, 's, (Entity, &'static LookAtProperties), (Without<LookAt>, With<HeadBoneEntity>)>;

/// Gaze in -1..=1: `x` positive is the avatar's left, `y` positive is up.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq)]
pub struct GazeInput {
    pub x: f32,
    pub y: f32,
}

/// Gaze from the four look expression weights.
pub fn gaze_from_expressions(get: impl Fn(&str) -> f32) -> GazeInput {
    GazeInput {
        x: (get("lookLeft") - get("lookRight")).clamp(-1.0, 1.0),
        y: (get("lookUp") - get("lookDown")).clamp(-1.0, 1.0),
    }
}

/// How far the target moves for full gaze, in metres at 1 m from the face. The VRM range maps turn
/// the eyes by `outputScale` degrees at 90 degrees of input, so the target has to sit far to the side
/// for the eyes to reach their range.
const GAZE_SPAN_X: f32 = 4.0;
const GAZE_SPAN_Y: f32 = 3.0;

#[derive(Resource)]
struct GazeTarget(Entity);

fn setup(mut commands: Commands) {
    let target = commands.spawn(Transform::from_xyz(0.0, 1.4, 1.0)).id();
    commands.insert_resource(GazeTarget(target));
}

/// Adds `LookAt` to bone-type VRMs once their look-at data is loaded. Expression-type models are
/// left alone: bevy_vrm1 does not implement look-at for them (it would panic).
fn attach_look_at(
    mut commands: Commands,
    target: Res<GazeTarget>,
    roots: LookAtRoots,
) {
    for (root, properties) in &roots {
        if properties.r#type == LookAtType::Bone {
            commands.entity(root).insert(LookAt::Target(target.0));
        }
    }
}

/// Gaze scaled by the gaze gain. No clamp: a gain above 1 moves the target further out.
pub fn scale_gaze(input: GazeInput, gain: f32) -> GazeInput {
    GazeInput { x: input.x * gain, y: input.y * gain }
}

fn aim_target(
    input: Res<GazeInput>,
    gains: Res<Gains>,
    target: Res<GazeTarget>,
    heads: Query<&HeadBoneEntity, With<LookAt>>,
    globals: Query<&GlobalTransform>,
    mut transforms: Query<&mut Transform>,
) {
    let Some(head) = heads.iter().next() else { return };
    let Ok(head_global) = globals.get(head.0) else { return };
    let g = scale_gaze(*input, gains.gaze);
    if let Ok(mut t) = transforms.get_mut(target.0) {
        t.translation = head_global.translation() + Vec3::new(g.x * GAZE_SPAN_X, g.y * GAZE_SPAN_Y, 1.0);
    }
}

pub struct GazePlugin;

impl Plugin for GazePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GazeInput>()
            .init_resource::<Gains>()
            .add_systems(Startup, setup)
            .add_systems(Update, (attach_look_at, aim_target).chain());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gaze_is_left_minus_right_and_up_minus_down_clamped() {
        let g = gaze_from_expressions(|n| match n {
            "lookLeft" => 0.9,
            "lookRight" => 0.1,
            "lookUp" => 0.2,
            "lookDown" => 0.9,
            _ => 0.0,
        });
        assert!((g.x - 0.8).abs() < 1e-6 && (g.y + 0.7).abs() < 1e-6, "{g:?}");
        let none = gaze_from_expressions(|_| 0.0);
        assert_eq!(none, GazeInput::default());
    }

    #[test]
    fn gaze_gain_scales_both_axes_without_clamp() {
        let g = GazeInput { x: 0.5, y: -0.4 };
        assert_eq!(scale_gaze(g, 1.0), g, "unit gain is identity");
        assert_eq!(scale_gaze(g, 0.0), GazeInput::default(), "zero gain looks straight ahead");
        assert_eq!(scale_gaze(g, 2.0), GazeInput { x: 1.0, y: -0.8 });
    }
}
