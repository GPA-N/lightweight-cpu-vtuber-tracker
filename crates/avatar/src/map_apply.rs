use crate::map_table::MapTable;
use vt_core::names::index_of;
use std::collections::HashMap;

/// Applies 52 ARKit weights through the table to avatar expression weights.
///
/// Returns only expressions with at least one table entry. Unmapped expressions
/// are absent so the driver leaves them at their default weight.
///
/// Several ARKit shapes may drive one expression: their `weight * entry.weight`
/// contributions are summed first, then the total is clamped to 0.0..=1.0.
/// (Sum-then-clamp, not clamp-per-shape, so two half-strength shapes reach full.)
/// Per-entry `max` clamps are already applied to `entry.weight` at load time.
pub fn apply(table: &MapTable, weights: &[f32; 52]) -> HashMap<String, f32> {
    let mut sums: HashMap<String, f32> = HashMap::new();
    for (name, entries) in &table.entries {
        let Some(i) = index_of(name) else { continue };
        for e in entries {
            *sums.entry(e.expression.clone()).or_insert(0.0) += weights[i] * e.weight;
        }
    }
    for v in sums.values_mut() {
        *v = v.clamp(0.0, 1.0);
    }
    sums
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map_table::{parse, MapEntry};

    const REAL_TABLE: &str = include_str!("../../../data/blendshape_map.json");

    /// Builds a 52-weight array with the given ARKit names set and all others 0.0.
    fn weights_with(pairs: &[(&str, f32)]) -> [f32; 52] {
        let mut w = [0.0f32; 52];
        for (name, value) in pairs {
            let i = vt_core::names::index_of(name).expect("known ARKit name");
            w[i] = *value;
        }
        w
    }

    #[test]
    fn jaw_open_gives_mapped_amount() {
        let table = parse(REAL_TABLE, "data/blendshape_map.json").expect("real table parses");
        let out = apply(&table, &weights_with(&[("jawOpen", 1.0)]));
        assert_eq!(out.get("aa").copied(), Some(1.0));
    }

    #[test]
    fn mapped_amount_above_one_is_clamped() {
        let table = MapTable {
            entries: [(
                "jawOpen".to_string(),
                vec![MapEntry { expression: "aa".into(), weight: 2.0 }],
            )]
            .into_iter()
            .collect(),
        };
        let out = apply(&table, &weights_with(&[("jawOpen", 1.0)]));
        assert_eq!(out.get("aa").copied(), Some(1.0));
    }

    #[test]
    fn several_shapes_on_one_expression_add_then_clamp() {
        let table = parse(REAL_TABLE, "data/blendshape_map.json").expect("real table parses");
        let out = apply(
            &table,
            &weights_with(&[("mouthFunnel", 1.0), ("mouthPucker", 1.0)]),
        );
        assert_eq!(out.get("ou").copied(), Some(1.0));

        let half = apply(&table, &weights_with(&[("mouthFunnel", 0.5)]));
        assert_eq!(half.get("ou").copied(), Some(0.5));
    }

    #[test]
    fn unmapped_expression_is_absent() {
        let table = parse(REAL_TABLE, "data/blendshape_map.json").expect("real table parses");
        let out = apply(&table, &weights_with(&[("jawOpen", 1.0)]));
        assert!(!out.contains_key("neutral"));
        assert!(!out.contains_key("blink"));
        // Expressions that have a table entry are returned (0.0 when their shapes are 0.0);
        // only jawOpen is non-zero here.
        assert_eq!(out.get("aa").copied(), Some(1.0));
        assert_eq!(out.values().filter(|v| **v > 0.0).count(), 1);
    }
}
