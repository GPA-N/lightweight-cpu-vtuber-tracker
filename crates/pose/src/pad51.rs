use core::names::{index_of, MEDIAPIPE_OUTPUT_NAMES};

/// Fallback model output order: the 51 names of `arkit_blendshapes.json`, alphabetical.
/// Order from docs/notes/model-fallback.md section 6 (inferred; index-to-output mapping unproven).
pub const FALLBACK_NAMES: [&str; 51] = [
    "browDownLeft",
    "browDownRight",
    "browInnerUp",
    "browOuterUpLeft",
    "browOuterUpRight",
    "cheekPuff",
    "cheekSquintLeft",
    "cheekSquintRight",
    "eyeBlinkLeft",
    "eyeBlinkRight",
    "eyeLookDownLeft",
    "eyeLookDownRight",
    "eyeLookInLeft",
    "eyeLookInRight",
    "eyeLookOutLeft",
    "eyeLookOutRight",
    "eyeLookUpLeft",
    "eyeLookUpRight",
    "eyeSquintLeft",
    "eyeSquintRight",
    "eyeWideLeft",
    "eyeWideRight",
    "jawForward",
    "jawLeft",
    "jawOpen",
    "jawRight",
    "mouthClose",
    "mouthDimpleLeft",
    "mouthDimpleRight",
    "mouthFrownLeft",
    "mouthFrownRight",
    "mouthFunnel",
    "mouthLeft",
    "mouthLowerDownLeft",
    "mouthLowerDownRight",
    "mouthPressLeft",
    "mouthPressRight",
    "mouthPucker",
    "mouthRight",
    "mouthRollLower",
    "mouthRollUpper",
    "mouthShrugLower",
    "mouthShrugUpper",
    "mouthSmileLeft",
    "mouthSmileRight",
    "mouthStretchLeft",
    "mouthStretchRight",
    "mouthUpperUpLeft",
    "mouthUpperUpRight",
    "noseSneerLeft",
    "noseSneerRight",
];

/// MediaPipe raw output (MEDIAPIPE_OUTPUT_NAMES order) to 52 ARKit weights. Drops `_neutral`, `tongueOut` = 0.0.
pub fn from_mediapipe(raw: &[f32; 52]) -> [f32; 52] {
    let mut out = [0.0; 52];
    for (name, &v) in MEDIAPIPE_OUTPUT_NAMES.iter().zip(raw.iter()) {
        if let Some(i) = index_of(name) {
            out[i] = v.clamp(0.0, 1.0);
        }
    }
    out
}

/// Fallback model raw output (FALLBACK_NAMES order) to 52 ARKit weights. `tongueOut` = 0.0.
pub fn from_fallback(raw: &[f32; 51]) -> [f32; 52] {
    let mut out = [0.0; 52];
    for (name, &v) in FALLBACK_NAMES.iter().zip(raw.iter()) {
        if let Some(i) = index_of(name) {
            out[i] = v.clamp(0.0, 1.0);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::names::BLENDSHAPE_NAMES;
    use std::collections::HashSet;

    /// Distinct value per index, all inside 0.0..=1.0.
    fn distinct(i: usize) -> f32 {
        (i as f32 + 1.0) / 100.0
    }

    #[test]
    fn pad51_mediapipe_maps_each_name_to_its_index() {
        let mut raw = [0.0f32; 52];
        for (i, v) in raw.iter_mut().enumerate() {
            *v = distinct(i);
        }
        let out = from_mediapipe(&raw);
        assert_eq!(out.len(), 52);
        for i in 1..52 {
            let idx = index_of(MEDIAPIPE_OUTPUT_NAMES[i]).expect("known name");
            assert_eq!(out[idx], raw[i], "{}", MEDIAPIPE_OUTPUT_NAMES[i]);
        }
    }

    #[test]
    fn pad51_mediapipe_tongue_out_zero_and_neutral_absent() {
        let mut raw = [0.0f32; 52];
        for (i, v) in raw.iter_mut().enumerate() {
            *v = distinct(i);
        }
        let out = from_mediapipe(&raw);
        assert_eq!(out[index_of("tongueOut").unwrap()], 0.0);
        assert!(index_of("_neutral").is_none());
        // Multiset of outputs = raw[1..] plus one 0.0 (tongueOut); raw[0] (_neutral) never appears.
        let mut got: Vec<f32> = out.to_vec();
        let mut want: Vec<f32> = raw[1..].to_vec();
        want.push(0.0);
        got.sort_by(|a, b| a.partial_cmp(b).unwrap());
        want.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(got, want);
        assert!(!out.contains(&raw[0]));
    }

    #[test]
    fn pad51_mediapipe_clamps_to_unit_range() {
        let mut raw = [0.5f32; 52];
        let j = MEDIAPIPE_OUTPUT_NAMES.iter().position(|n| *n == "jawOpen").unwrap();
        raw[j] = 2.0;
        raw[1] = -0.5; // browDownLeft
        let out = from_mediapipe(&raw);
        assert_eq!(out[index_of("jawOpen").unwrap()], 1.0);
        assert_eq!(out[index_of("browDownLeft").unwrap()], 0.0);
    }

    #[test]
    fn pad51_fallback_maps_each_name_to_its_index() {
        let mut raw = [0.0f32; 51];
        for (j, v) in raw.iter_mut().enumerate() {
            *v = distinct(j);
        }
        let out = from_fallback(&raw);
        assert_eq!(out.len(), 52);
        for j in 0..51 {
            let idx = index_of(FALLBACK_NAMES[j]).expect("known name");
            assert_eq!(out[idx], raw[j], "{}", FALLBACK_NAMES[j]);
        }
        assert_eq!(out[index_of("tongueOut").unwrap()], 0.0);
    }

    #[test]
    fn pad51_fallback_clamps_to_unit_range() {
        let mut raw = [0.5f32; 51];
        let j = FALLBACK_NAMES.iter().position(|n| *n == "jawOpen").unwrap();
        raw[j] = 3.0;
        raw[0] = -1.0; // browDownLeft
        let out = from_fallback(&raw);
        assert_eq!(out[index_of("jawOpen").unwrap()], 1.0);
        assert_eq!(out[index_of("browDownLeft").unwrap()], 0.0);
    }

    #[test]
    fn pad51_fallback_names_are_51_unique_and_known() {
        let unique: HashSet<&str> = FALLBACK_NAMES.iter().copied().collect();
        assert_eq!(unique.len(), 51);
        for name in FALLBACK_NAMES {
            assert!(BLENDSHAPE_NAMES.contains(&name), "{name} not in BLENDSHAPE_NAMES");
        }
    }
}
