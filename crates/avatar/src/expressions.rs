//! Drives VRM expression weights from the mapped ARKit blendshape output.

use bevy::prelude::*;
use bevy_vrm1::prelude::*;
use std::collections::HashMap;

/// Latest expression weights, keyed by VRM expression name.
#[derive(Resource, Default)]
pub struct ExpressionTarget(pub HashMap<String, f32>);

/// Clamps weights to 0.0..=1.0, drops NaN, and sorts by name.
pub fn to_set_expressions(weights: &HashMap<String, f32>) -> Vec<(String, f32)> {
    let mut out: Vec<(String, f32)> = weights
        .iter()
        .filter(|(_, w)| !w.is_nan())
        .map(|(name, w)| (name.clone(), w.clamp(0.0, 1.0)))
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn drive_expressions(
    target: Res<ExpressionTarget>,
    roots: Query<Entity, With<Vrm>>,
    ready: Query<(), Added<ExpressionEntityMap>>,
    mut commands: Commands,
) {
    // bevy_vrm1 drops SetExpressions sent before the model builds its expression map, so
    // also drive when a model becomes ready. Otherwise a target set during loading is lost.
    if !target.is_changed() && ready.is_empty() {
        return;
    }
    let weights = to_set_expressions(&target.0);
    for root in &roots {
        let pairs = weights
            .iter()
            .map(|(name, w)| (VrmExpression(name.clone()), *w));
        commands.trigger(SetExpressions::from_iter(root, pairs));
    }
}

pub struct ExpressionPlugin;

impl Plugin for ExpressionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ExpressionTarget>().add_systems(
            PostUpdate,
            drive_expressions.before(VrmSystemSets::Expressions),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Resource, Default)]
    struct Captured(Vec<HashMap<String, f32>>);

    fn weights(pairs: &[(&str, f32)]) -> HashMap<String, f32> {
        pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
    }

    #[test]
    fn clamps_to_unit_range() {
        let out = to_set_expressions(&weights(&[("aa", 1.7), ("ih", -0.2)]));
        assert_eq!(out, vec![("aa".to_string(), 1.0), ("ih".to_string(), 0.0)]);
    }

    #[test]
    fn drops_nan() {
        let out = to_set_expressions(&weights(&[("aa", f32::NAN), ("ih", 0.5)]));
        assert_eq!(out, vec![("ih".to_string(), 0.5)]);
    }

    #[test]
    fn sorts_by_name() {
        let out = to_set_expressions(&weights(&[("oh", 0.1), ("aa", 0.2), ("ee", 0.3)]));
        let names: Vec<&str> = out.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, vec!["aa", "ee", "oh"]);
    }

    fn spawn_app() -> (App, Entity) {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(ExpressionPlugin)
            .init_resource::<Captured>();
        let root = app
            .world_mut()
            .spawn(Vrm)
            .observe(|trigger: On<SetExpressions>, mut captured: ResMut<Captured>| {
                let map = trigger
                    .weights
                    .iter()
                    .map(|(k, v)| (k.0.clone(), *v))
                    .collect();
                captured.0.push(map);
            })
            .id();
        (app, root)
    }

    #[test]
    fn aa_0_8_triggers_exactly_one_set_expressions_on_root() {
        let (mut app, _root) = spawn_app();
        app.world_mut()
            .resource_mut::<ExpressionTarget>()
            .0
            .insert("aa".to_string(), 0.8);
        app.update();

        let captured = &app.world().resource::<Captured>().0;
        assert_eq!(captured.len(), 1, "captured {captured:?}");
        assert_eq!(captured[0].get("aa").copied(), Some(0.8));
    }

    #[test]
    fn unchanged_target_triggers_nothing() {
        let (mut app, _root) = spawn_app();
        app.world_mut()
            .resource_mut::<ExpressionTarget>()
            .0
            .insert("aa".to_string(), 0.8);
        app.update();
        app.world_mut().resource_mut::<Captured>().0.clear();

        app.update();

        assert!(app.world().resource::<Captured>().0.is_empty());
    }

    #[test]
    fn target_set_before_the_model_is_ready_reaches_it_when_it_becomes_ready() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(ExpressionPlugin)
            .init_resource::<Captured>();
        let root = app
            .world_mut()
            .spawn(Vrm)
            .observe(|trigger: On<SetExpressions>, mut captured: ResMut<Captured>| {
                let map = trigger.weights.iter().map(|(k, v)| (k.0.clone(), *v)).collect();
                captured.0.push(map);
            })
            .id();
        app.world_mut()
            .resource_mut::<ExpressionTarget>()
            .0
            .insert("aa".to_string(), 0.8);
        app.update();
        app.world_mut().resource_mut::<Captured>().0.clear();

        app.world_mut().entity_mut(root).insert(ExpressionEntityMap(Default::default()));
        app.update();

        let captured = &app.world().resource::<Captured>().0;
        assert_eq!(captured.len(), 1, "captured {captured:?}");
        assert_eq!(captured[0].get("aa").copied(), Some(0.8));
    }
}
