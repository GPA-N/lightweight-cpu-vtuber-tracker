//! Applies the tracked head rotation to the VRM head bone.

use bevy::prelude::*;
use bevy_vrm1::prelude::*;

/// Latest tracked head rotation, in Bevy space.
#[derive(Resource, Default)]
pub struct HeadTarget(pub Quat);

/// Converts nalgebra `UnitQuaternion` coords, which are stored as `[i, j, k, w]`.
pub fn quat_from_tracking(c: [f32; 4]) -> Quat {
    Quat::from_xyzw(c[0], c[1], c[2], c[3])
}

/// Local bone rotation that moves the bone by `delta` (a world-space rotation
/// away from its rest pose). Same formula as bevy_vrm1 `bone_rotation`:
/// `rest * rest_g^-1 * delta * rest_g`. With an identity delta the bone keeps
/// its rest rotation, whatever that is.
pub fn local_from_delta(delta: Quat, rest: Quat, rest_global: Quat) -> Quat {
    rest * rest_global.inverse() * delta * rest_global
}

/// How far the head's parent has turned away from its rest pose in the world (the torso twisting,
/// leaning or rolling). The face tracker measures the head's own world rotation, which already
/// includes this, so it must be taken out before the head bone gets its local rotation.
fn parent_motion(
    head: Entity,
    parents: &Query<&ChildOf>,
    globals: &Query<(&GlobalTransform, &RestGlobalTransform)>,
) -> Quat {
    let Ok(parent) = parents.get(head) else { return Quat::IDENTITY };
    let Ok((now, rest)) = globals.get(parent.parent()) else { return Quat::IDENTITY };
    now.rotation() * rest.rotation().inverse()
}

fn apply_head_rotation(
    target: Res<HeadTarget>,
    roots: Query<&HeadBoneEntity>,
    parents: Query<&ChildOf>,
    globals: Query<(&GlobalTransform, &RestGlobalTransform)>,
    mut bones: Query<(&mut Transform, &RestTransform, &RestGlobalTransform)>,
) {
    for head in &roots {
        let delta = parent_motion(head.0, &parents, &globals).inverse() * target.0;
        if let Ok((mut transform, rest, rest_global)) = bones.get_mut(head.0) {
            transform.rotation = local_from_delta(delta, rest.rotation, rest_global.rotation());
        }
    }
}

pub struct HeadPlugin;

impl Plugin for HeadPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HeadTarget>().add_systems(
            PostUpdate,
            apply_head_rotation
                .after(VrmSystemSets::GazeControl)
                .before(VrmSystemSets::PropagateAfterExpressions),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::app::App;
    use bevy::math::Vec3;

    fn spawn_rig(app: &mut App) -> Entity {
        spawn_rig_with_rest(app, Quat::IDENTITY, Quat::IDENTITY)
    }

    /// Head bone whose parent has global rest `parent_global`, and whose own
    /// local rest is `rest`.
    fn spawn_rig_with_rest(app: &mut App, rest: Quat, parent_global: Quat) -> Entity {
        let head = app
            .world_mut()
            .spawn((
                Transform::from_rotation(rest),
                RestTransform(Transform::from_rotation(rest)),
                RestGlobalTransform(GlobalTransform::from(Transform::from_rotation(
                    parent_global * rest,
                ))),
            ))
            .id();
        app.world_mut().spawn(HeadBoneEntity(head));
        head
    }

    fn head_rotation(app: &App, head: Entity) -> Quat {
        app.world().get::<Transform>(head).unwrap().rotation
    }

    #[test]
    fn yaw_20_degrees_reaches_head_bone_after_one_update() {
        let mut app = App::new();
        app.add_plugins(HeadPlugin);
        let head = spawn_rig(&mut app);

        app.world_mut().resource_mut::<HeadTarget>().0 =
            Quat::from_rotation_y(20f32.to_radians());
        app.update();

        let angle = head_rotation(&app, head)
            .angle_between(Quat::IDENTITY)
            .to_degrees();
        assert!((angle - 20.0).abs() < 0.01, "angle was {angle}");
    }

    /// World motion of the head away from its rest pose, given the parent global rotation.
    fn world_motion(local: Quat, rest: Quat, parent_global: Quat) -> Quat {
        (parent_global * local) * (parent_global * rest).inverse()
    }

    #[test]
    fn turned_torso_is_not_counted_twice_in_the_head_turn() {
        let mut app = App::new();
        app.add_plugins(HeadPlugin);
        let torso_yaw = Quat::from_rotation_y(20f32.to_radians());
        let parent = app
            .world_mut()
            .spawn((GlobalTransform::from(Transform::from_rotation(torso_yaw)), RestGlobalTransform(GlobalTransform::IDENTITY)))
            .id();
        let head = app
            .world_mut()
            .spawn((
                Transform::IDENTITY,
                RestTransform(Transform::IDENTITY),
                RestGlobalTransform(GlobalTransform::IDENTITY),
                ChildOf(parent),
            ))
            .id();
        app.world_mut().spawn(HeadBoneEntity(head));
        // The face tracker sees the head turned 30 degrees in the world; the torso already turned 20.
        app.world_mut().resource_mut::<HeadTarget>().0 = Quat::from_rotation_y(30f32.to_radians());
        app.update();
        let local = head_rotation(&app, head).angle_between(Quat::IDENTITY).to_degrees();
        assert!((local - 10.0).abs() < 0.01, "head turned {local} on top of the torso");
    }

    /// Same rotation, tolerant of f32 error (`angle_between` loses precision near 0).
    fn close(a: Quat, b: Quat) -> bool {
        1.0 - a.dot(b).abs() < 1e-6
    }

    /// About 120 degrees on a diagonal axis, like the Twist sample head.
    fn big_rest() -> Quat {
        Quat::from_xyzw(0.468, 0.530, 0.468, 0.530).normalize()
    }

    #[test]
    fn large_rest_rotation_gives_same_world_motion_as_identity_rest() {
        let delta = Quat::from_euler(EulerRot::YXZ, 20f32.to_radians(), -10f32.to_radians(), 0.0);
        let parent = Quat::from_rotation_z(0.3);

        for rest in [Quat::IDENTITY, big_rest()] {
            let local = local_from_delta(delta, rest, parent * rest);
            let motion = world_motion(local, rest, parent);
            assert!(close(motion, delta), "rest {rest:?}: {motion:?}");
            // Identity tracking keeps the rest pose.
            let neutral = local_from_delta(Quat::IDENTITY, rest, parent * rest);
            assert!(close(neutral, rest), "rest {rest:?}");
        }
    }

    #[test]
    fn system_keeps_rest_rotation_for_neutral_and_applies_delta() {
        let mut app = App::new();
        app.add_plugins(HeadPlugin);
        let head = spawn_rig_with_rest(&mut app, big_rest(), Quat::IDENTITY);

        app.update();
        assert!(close(head_rotation(&app, head), big_rest()));

        let delta = Quat::from_rotation_y(20f32.to_radians());
        app.world_mut().resource_mut::<HeadTarget>().0 = delta;
        app.update();
        let motion = world_motion(head_rotation(&app, head), big_rest(), Quat::IDENTITY);
        assert!(close(motion, delta), "m {motion:?} d {delta:?}");
    }

    #[test]
    fn tracking_coords_convert_to_bevy_rotation() {
        // nalgebra stores coords as [i, j, k, w]. 90 degrees about +y.
        let half = 45f32.to_radians();
        let q = quat_from_tracking([0.0, half.sin(), 0.0, half.cos()]);
        let rotated = q * Vec3::X;
        assert!(rotated.distance(Vec3::NEG_Z) < 1e-5, "rotated to {rotated:?}");
    }
}
