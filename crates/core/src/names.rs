/// Raw MediaPipe output order (index 0 is `_neutral`), per docs/notes/model-blendshapes.md.
pub const MEDIAPIPE_OUTPUT_NAMES: [&str; 52] = [
    "_neutral",
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

/// 52 ARKit names: MediaPipe names 1..51 in order, then `tongueOut`. No `_neutral`.
pub const BLENDSHAPE_NAMES: [&str; 52] = [
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
    "tongueOut",
];

/// Index of `name` in `BLENDSHAPE_NAMES`.
pub fn index_of(name: &str) -> Option<usize> {
    BLENDSHAPE_NAMES.iter().position(|n| *n == name)
}

/// Index of the Left/Right partner of `BLENDSHAPE_NAMES[i]`, or `i` if it has none.
pub fn mirror_partner(i: usize) -> usize {
    let name = BLENDSHAPE_NAMES[i];
    let partner = if let Some(stem) = name.strip_suffix("Left") {
        format!("{stem}Right")
    } else if let Some(stem) = name.strip_suffix("Right") {
        format!("{stem}Left")
    } else {
        return i;
    };
    index_of(&partner).unwrap_or(i)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn blendshape_names_are_52_unique_with_tongue_out_and_no_neutral() {
        let unique: HashSet<&str> = BLENDSHAPE_NAMES.iter().copied().collect();
        assert_eq!(unique.len(), 52);
        assert!(unique.contains("tongueOut"));
        assert!(!unique.contains("_neutral"));
    }

    #[test]
    fn mediapipe_output_names_are_52_unique_neutral_first_no_tongue_out() {
        let unique: HashSet<&str> = MEDIAPIPE_OUTPUT_NAMES.iter().copied().collect();
        assert_eq!(unique.len(), 52);
        assert_eq!(MEDIAPIPE_OUTPUT_NAMES[0], "_neutral");
        assert!(!unique.contains("tongueOut"));
    }

    #[test]
    fn every_left_has_right_partner_and_mirror_is_involution() {
        for (i, name) in BLENDSHAPE_NAMES.iter().enumerate() {
            if let Some(stem) = name.strip_suffix("Left") {
                let right = format!("{stem}Right");
                assert!(index_of(&right).is_some(), "{name} has no Right partner");
            }
            assert_eq!(mirror_partner(mirror_partner(i)), i);
        }
    }

    #[test]
    fn index_of_finds_jaw_open() {
        assert_eq!(index_of("jawOpen"), Some(24));
    }
}
