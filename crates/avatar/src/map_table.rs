use vt_core::names::index_of;
use serde::Deserialize;
use std::fmt;
use std::path::Path;

/// One avatar expression driven by an ARKit blendshape.
#[derive(Debug, Clone, PartialEq)]
pub struct MapEntry {
    pub expression: String,
    /// Weight applied to the blendshape value, clamped to 0.0..=max at load time.
    pub weight: f32,
}

/// ARKit blendshape name to its expression entries.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MapTable {
    pub entries: std::collections::BTreeMap<String, Vec<MapEntry>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MapError {
    Missing(String),
    Malformed(String, String),
    UnknownBlendshape(String),
}

impl fmt::Display for MapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MapError::Missing(path) => write!(f, "mapping table file {path} is missing"),
            MapError::Malformed(path, msg) => {
                write!(f, "mapping table file {path} is malformed: {msg}")
            }
            MapError::UnknownBlendshape(name) => {
                write!(f, "mapping table entry {name} is not an ARKit blendshape name")
            }
        }
    }
}

impl std::error::Error for MapError {}

/// Reads and parses the mapping table at `path`.
pub fn load(path: &Path) -> Result<MapTable, MapError> {
    let name = path.display().to_string();
    let text = std::fs::read_to_string(path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => MapError::Missing(name.clone()),
        _ => MapError::Malformed(name.clone(), e.to_string()),
    })?;
    parse(&text, &name)
}

#[derive(Deserialize)]
#[serde(untagged)]
enum OneOrMany {
    Many(Vec<RawEntry>),
    One(RawEntry),
}

#[derive(Deserialize)]
struct RawEntry {
    expression: String,
    weight: f32,
    max: Option<f32>,
}

/// Parses mapping table JSON. `source_name` is the file name used in errors.
///
/// Format: a JSON object keyed by ARKit name. Each value is either one entry
/// object or an array of entry objects (one ARKit name may drive several
/// expressions). Entry: `{"expression": str, "weight": f32, "max": f32}`,
/// where `max` is optional and defaults to 1.0 and clamps the weight.
pub fn parse(json: &str, source_name: &str) -> Result<MapTable, MapError> {
    let raw: std::collections::BTreeMap<String, OneOrMany> =
        serde_json::from_str(json).map_err(|e| MapError::Malformed(source_name.into(), e.to_string()))?;
    let mut entries = std::collections::BTreeMap::new();
    for (name, value) in raw {
        if index_of(&name).is_none() {
            return Err(MapError::UnknownBlendshape(name));
        }
        let raws = match value {
            OneOrMany::Many(v) => v,
            OneOrMany::One(e) => vec![e],
        };
        let list = raws
            .into_iter()
            .map(|r| MapEntry {
                weight: r.weight.min(r.max.unwrap_or(1.0)).max(0.0),
                expression: r.expression,
            })
            .collect();
        entries.insert(name, list);
    }
    Ok(MapTable { entries })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Expression names of the Seed-san VRM 1.0 preset set, from docs/notes/seed-san.md.
    const SEED_SAN_EXPRESSIONS: [&str; 18] = [
        "aa", "angry", "blink", "blinkLeft", "blinkRight", "ee", "happy", "ih", "lookDown",
        "lookLeft", "lookRight", "lookUp", "neutral", "oh", "ou", "relaxed", "sad", "surprised",
    ];

    const REAL_TABLE: &str = include_str!("../../../data/blendshape_map.json");

    #[test]
    fn valid_real_table_parses_and_uses_only_seed_san_expressions() {
        let table = parse(REAL_TABLE, "data/blendshape_map.json").expect("real table parses");
        assert!(!table.entries.is_empty());
        for (name, entries) in &table.entries {
            for e in entries {
                assert!(
                    SEED_SAN_EXPRESSIONS.contains(&e.expression.as_str()),
                    "{name} uses unknown expression {}",
                    e.expression
                );
                assert!((0.0..=1.0).contains(&e.weight));
            }
        }
        let jaw = &table.entries["jawOpen"];
        assert_eq!(jaw, &vec![MapEntry { expression: "aa".into(), weight: 1.0 }]);
    }

    #[test]
    fn array_value_gives_several_entries_and_max_clamps_weight() {
        let json = r#"{ "jawOpen": [
            { "expression": "aa", "weight": 1.5, "max": 0.8 },
            { "expression": "oh", "weight": 0.5 } ] }"#;
        let table = parse(json, "t.json").expect("parses");
        let entries = &table.entries["jawOpen"];
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].weight, 0.8);
        assert_eq!(entries[1].weight, 0.5);
    }

    #[test]
    fn missing_file_error_names_the_path() {
        let path = Path::new("no_such_dir/no_such_blendshape_map.json");
        let err = load(path).expect_err("missing file must fail");
        assert!(matches!(err, MapError::Missing(_)));
        assert!(err.to_string().contains("no_such_blendshape_map.json"));
    }

    #[test]
    fn malformed_json_error_names_the_file() {
        let err = parse("{ \"jawOpen\": ", "bad_map.json").expect_err("malformed must fail");
        assert!(matches!(err, MapError::Malformed(ref p, _) if p == "bad_map.json"));
        assert!(err.to_string().contains("bad_map.json"));
    }

    #[test]
    fn eye_look_entry_is_accepted_and_the_real_table_drives_all_four_gaze_expressions() {
        let json = r#"{ "eyeLookUpLeft": { "expression": "lookUp", "weight": 0.7 } }"#;
        assert!(parse(json, "t.json").is_ok());
        let table = parse(REAL_TABLE, "data/blendshape_map.json").expect("real table parses");
        for expr in ["lookLeft", "lookRight", "lookUp", "lookDown"] {
            assert!(
                table.entries.values().flatten().any(|e| e.expression == expr),
                "no blendshape drives {expr}"
            );
        }
    }

    #[test]
    fn unknown_blendshape_is_rejected() {
        let json = r#"{ "jawOpenish": { "expression": "aa", "weight": 1.0 } }"#;
        let err = parse(json, "t.json").expect_err("unknown name must fail");
        assert_eq!(err, MapError::UnknownBlendshape("jawOpenish".into()));
    }
}
