//! VRM 1.0 makes `isBinary`, `overrideBlink`, `overrideLookAt` and `overrideMouth` optional.
//! bevy_vrm1 0.10.0 required them, so models that omit them (VRM1_Constraint_Twist_Sample)
//! were silently rejected. The vendored bevy_vrm1 (see vendor/bevy_vrm1/PATCHES.md) adds the defaults.

use bevy_vrm1::prelude::VrmExtensions;
use serde_json::{Map, Value, json};

fn extensions(presets: Value) -> Map<String, Value> {
    let value = json!({
        "VRMC_vrm": {
            "specVersion": "1.0",
            "humanoid": { "humanBones": {} },
            "expressions": { "preset": presets }
        }
    });
    value.as_object().unwrap().clone()
}

#[test]
fn preset_without_optional_keys_gets_spec_defaults() {
    let map = extensions(json!({
        "neutral": { "morphTargetBinds": [ { "node": 1, "index": 0, "weight": 1.0 } ] },
        "blink": {}
    }));
    let parsed = match VrmExtensions::new(&map) {
        Ok(parsed) => parsed,
        Err(e) => panic!("preset without optional keys must parse: {e}"),
    };
    let presets = parsed.vrmc_vrm.expressions.expect("expressions").preset;
    for name in ["neutral", "blink"] {
        let preset = &presets[name];
        assert!(!preset.is_binary, "{name} isBinary");
        assert_eq!(preset.override_blink, "none", "{name} overrideBlink");
        assert_eq!(preset.override_look_at, "none", "{name} overrideLookAt");
        assert_eq!(preset.override_mouth, "none", "{name} overrideMouth");
    }
    assert_eq!(presets["neutral"].morph_target_binds.as_ref().unwrap().len(), 1);
}

#[test]
fn preset_with_explicit_keys_keeps_them() {
    let map = extensions(json!({
        "aa": {
            "isBinary": true,
            "overrideBlink": "block",
            "overrideLookAt": "blend",
            "overrideMouth": "block"
        }
    }));
    let parsed = match VrmExtensions::new(&map) {
        Ok(parsed) => parsed,
        Err(e) => panic!("explicit keys must parse: {e}"),
    };
    let preset = &parsed.vrmc_vrm.expressions.expect("expressions").preset["aa"];
    assert!(preset.is_binary);
    assert_eq!(preset.override_blink, "block");
    assert_eq!(preset.override_look_at, "blend");
    assert_eq!(preset.override_mouth, "block");
}
