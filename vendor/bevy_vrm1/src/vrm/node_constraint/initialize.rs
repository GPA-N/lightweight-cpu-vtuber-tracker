use crate::prelude::ChildSearcher;
use crate::vrm::node_constraint::registry::{Constraint, NodeConstraintRegistry};
use crate::vrm::node_constraint::{
    AimConstraintDest, AimConstraintDestinations, RollConstraintDest, RollConstraintDestinations,
    RotationConstraintDest, RotationConstraintDestinations,
};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;

#[derive(EntityEvent)]
pub(crate) struct RequestInitializeNodeConstraints(pub(crate) Entity);

pub struct NodeConstraintInitializePlugin;

impl Plugin for NodeConstraintInitializePlugin {
    fn build(
        &self,
        app: &mut App,
    ) {
        app.add_observer(apply_initialize_node_constraints);
    }
}

/// Destinations gathered per source entity, so that several constraints with the same source are
/// inserted together. Inserting them one by one through `Commands` replaced the earlier ones, because
/// the component is not visible to the query until the commands are applied (a patch to upstream
/// bevy_vrm1; see PATCHES.md).
#[derive(Default)]
struct Gathered {
    rotation: HashMap<Entity, Vec<RotationConstraintDest>>,
    roll: HashMap<Entity, Vec<RollConstraintDest>>,
    aim: HashMap<Entity, Vec<AimConstraintDest>>,
}

fn apply_initialize_node_constraints(
    trigger: On<RequestInitializeNodeConstraints>,
    mut commands: Commands,
    mut rotation_constraints: Query<Option<&mut RotationConstraintDestinations>>,
    mut roll_constraints: Query<Option<&mut RollConstraintDestinations>>,
    mut aim_constraints: Query<Option<&mut AimConstraintDestinations>>,
    child_searcher: ChildSearcher,
    models: Query<(Entity, &NodeConstraintRegistry)>,
) {
    let root = trigger.event_target();
    let Ok((vrm, nodes)) = models.get(root) else {
        return;
    };
    let mut gathered = Gathered::default();
    for (name, constraints) in nodes.iter() {
        let Some(dest) = child_searcher.find_from_name(root, name.as_str()) else {
            continue;
        };
        for constraint in constraints {
            match constraint {
                Constraint::Rotation { source, weight } => {
                    if let Some(source) = child_searcher.find_from_name(vrm, source) {
                        gathered
                            .rotation
                            .entry(source)
                            .or_default()
                            .push(RotationConstraintDest { dest, weight: *weight });
                    }
                }
                Constraint::Roll {
                    roll_axis,
                    source,
                    weight,
                } => {
                    if let Some(source) = child_searcher.find_from_name(vrm, source) {
                        gathered.roll.entry(source).or_default().push(RollConstraintDest {
                            roll_axis: *roll_axis,
                            dest,
                            weight: *weight,
                        });
                    }
                }
                Constraint::Aim {
                    aim_axis,
                    source,
                    weight,
                } => {
                    if let Some(source) = child_searcher.find_from_name(vrm, source) {
                        gathered.aim.entry(source).or_default().push(AimConstraintDest {
                            aim_axis: *aim_axis,
                            dest,
                            weight: *weight,
                        });
                    }
                }
            }
        }
    }

    for (source, dests) in gathered.rotation {
        if let Ok(Some(mut existing)) = rotation_constraints.get_mut(source) {
            existing.0.extend(dests);
        } else {
            commands.entity(source).insert(RotationConstraintDestinations(dests));
        }
    }
    for (source, dests) in gathered.roll {
        if let Ok(Some(mut existing)) = roll_constraints.get_mut(source) {
            existing.0.extend(dests);
        } else {
            commands.entity(source).insert(RollConstraintDestinations(dests));
        }
    }
    for (source, dests) in gathered.aim {
        if let Ok(Some(mut existing)) = aim_constraints.get_mut(source) {
            existing.0.extend(dests);
        } else {
            commands.entity(source).insert(AimConstraintDestinations(dests));
        }
    }
}
