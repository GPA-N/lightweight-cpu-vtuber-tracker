use std::fmt;
use std::path::{Path, PathBuf};

const GLB_MAGIC: &[u8; 4] = b"glTF";
const CHUNK_TYPE_JSON: u32 = 0x4E4F_534A;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AvatarError {
    FileMissing(PathBuf),
    NotVrm1 { path: PathBuf, reason: String },
    NoGraphicsApi(String),
}

impl fmt::Display for AvatarError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AvatarError::FileMissing(path) => {
                write!(f, "avatar file not found: {}", path.display())
            }
            AvatarError::NotVrm1 { path, reason } => {
                write!(
                    f,
                    "not a valid VRM 1.0 file: {} ({reason})",
                    path.display()
                )
            }
            AvatarError::NoGraphicsApi(detail) => write!(
                f,
                "no usable graphics API (DirectX 12 or Vulkan) was found: {detail}"
            ),
        }
    }
}

impl std::error::Error for AvatarError {}

/// Checks that `path` is a glTF binary (.vrm) with a VRMC_vrm 1.x extension.
pub fn validate_vrm1(path: &Path) -> Result<(), AvatarError> {
    if !path.is_file() {
        return Err(AvatarError::FileMissing(path.to_path_buf()));
    }
    let not_vrm1 = |reason: String| AvatarError::NotVrm1 {
        path: path.to_path_buf(),
        reason,
    };
    let bytes = std::fs::read(path).map_err(|e| not_vrm1(format!("cannot read file: {e}")))?;

    if bytes.len() < 12 || &bytes[0..4] != GLB_MAGIC {
        return Err(not_vrm1("not a glTF binary file (missing glTF magic)".into()));
    }
    let version = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    if version != 2 {
        return Err(not_vrm1(format!("glTF version {version}, expected 2")));
    }

    if bytes.len() < 20 {
        return Err(not_vrm1("missing JSON chunk".into()));
    }
    let chunk_len = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]) as usize;
    let chunk_type = u32::from_le_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
    if chunk_type != CHUNK_TYPE_JSON {
        return Err(not_vrm1("first chunk is not JSON".into()));
    }
    let json_bytes = bytes
        .get(20..20 + chunk_len)
        .ok_or_else(|| not_vrm1("JSON chunk is truncated".into()))?;
    let json: serde_json::Value = serde_json::from_slice(json_bytes)
        .map_err(|e| not_vrm1(format!("invalid JSON chunk: {e}")))?;

    let extensions = json.get("extensions");
    if let Some(vrmc) = extensions.and_then(|e| e.get("VRMC_vrm")) {
        let spec = vrmc
            .get("specVersion")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if spec.starts_with("1.") {
            return Ok(());
        }
        return Err(not_vrm1(format!(
            "VRMC_vrm specVersion \"{spec}\" is not 1.x"
        )));
    }
    if extensions.and_then(|e| e.get("VRM")).is_some() {
        return Err(not_vrm1(
            "VRM 0.x is not supported, expected VRM 1.0".into(),
        ));
    }
    Err(not_vrm1("no VRMC_vrm extension".into()))
}

/// Humanoid bones the tracker drives. A model without them loads but that part does not move.
const TRACKED_BONES: [&str; 9] = [
    "hips", "spine", "head", "leftUpperArm", "rightUpperArm", "leftLowerArm", "rightLowerArm", "leftHand",
    "rightHand",
];
/// Eye bones matter only when the model turns its eyes with bones (`lookAt.type` is `bone`). A model
/// with `expression` look-at moves its eyes through the look expressions and needs no eye bones.
const EYE_BONES: [&str; 2] = ["leftEye", "rightEye"];
/// Expression presets the mapping table writes (data/blendshape_map.json).
const MAPPED_EXPRESSIONS: [&str; 10] =
    ["aa", "ou", "blinkLeft", "blinkRight", "angry", "sad", "surprised", "lookLeft", "lookUp", "lookDown"];

/// Problems that do not stop a VRM 1.0 model from loading but limit what the tracker can move.
/// Empty when the file cannot be read as a VRM (use `validate_vrm1` for that error).
pub fn vrm_warnings(path: &Path) -> Vec<String> {
    let Ok(bytes) = std::fs::read(path) else { return vec![] };
    if bytes.len() < 20 {
        return vec![];
    }
    let chunk_len = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]) as usize;
    let Some(json) = bytes.get(20..20 + chunk_len).and_then(|j| serde_json::from_slice::<serde_json::Value>(j).ok())
    else {
        return vec![];
    };
    let vrm = &json["extensions"]["VRMC_vrm"];
    let mut out = Vec::new();

    let eyes_by_bone = vrm["lookAt"]["type"].as_str() == Some("bone");
    let wanted = TRACKED_BONES.iter().chain(EYE_BONES.iter().filter(|_| eyes_by_bone));
    let missing: Vec<&str> =
        wanted.copied().filter(|b| vrm["humanoid"]["humanBones"][*b]["node"].is_null()).collect();
    if !missing.is_empty() {
        out.push(format!("missing humanoid bones, these parts will not move: {}", missing.join(", ")));
    }

    let presets = &vrm["expressions"]["preset"];
    let missing: Vec<&str> = MAPPED_EXPRESSIONS.iter().copied().filter(|e| presets[*e].is_null()).collect();
    if !missing.is_empty() {
        out.push(format!("missing expressions, these face shapes will not move: {}", missing.join(", ")));
    }

    let has_morphs = json["meshes"]
        .as_array()
        .is_some_and(|m| m.iter().any(|m| m["primitives"].as_array().is_some_and(|p| p.iter().any(|p| p["targets"].is_array()))));
    if !has_morphs {
        out.push("no morph targets in any mesh, the face cannot animate".into());
    }
    out
}

/// Wraps raw graphics adapter error text as the startup error.
pub fn describe_graphics_failure(raw: &str) -> AvatarError {
    AvatarError::NoGraphicsApi(raw.trim().to_string())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn temp_file(name: &str, bytes: &[u8]) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "avatar_errors_{name}_{}.vrm",
            std::process::id()
        ));
        std::fs::write(&path, bytes).unwrap();
        path
    }

    pub(crate) fn glb(json: &str) -> Vec<u8> {
        let mut body = json.as_bytes().to_vec();
        while !body.len().is_multiple_of(4) {
            body.push(b' ');
        }
        let total = 12 + 8 + body.len();
        let mut out = b"glTF".to_vec();
        out.extend(2u32.to_le_bytes());
        out.extend((total as u32).to_le_bytes());
        out.extend((body.len() as u32).to_le_bytes());
        out.extend(0x4E4F_534Au32.to_le_bytes());
        out.extend(body);
        out
    }

    const VRM1_JSON: &str =
        r#"{"asset":{"version":"2.0"},"extensions":{"VRMC_vrm":{"specVersion":"1.0"}}}"#;

    #[test]
    fn display_file_missing_names_file() {
        let e = AvatarError::FileMissing(PathBuf::from("C:/x/Seed.vrm"));
        let s = e.to_string();
        assert!(s.contains("avatar file not found"));
        assert!(s.contains("Seed.vrm"));
    }

    #[test]
    fn display_not_vrm1_names_file_and_reason() {
        let e = AvatarError::NotVrm1 {
            path: PathBuf::from("C:/x/Seed.vrm"),
            reason: "VRM 0.x is not supported, expected VRM 1.0".into(),
        };
        let s = e.to_string();
        assert!(s.contains("not a valid VRM 1.0 file"));
        assert!(s.contains("Seed.vrm"));
        assert!(s.contains("VRM 0.x is not supported"));
    }

    #[test]
    fn display_no_graphics_api_names_api_and_detail() {
        let e = AvatarError::NoGraphicsApi("adapter request failed".into());
        let s = e.to_string();
        assert!(s.contains("no usable graphics API"));
        assert!(s.contains("DirectX 12"));
        assert!(s.contains("Vulkan"));
        assert!(s.contains("adapter request failed"));
    }

    #[test]
    fn valid_minimal_vrm1_is_ok() {
        let path = temp_file("valid", &glb(VRM1_JSON));
        assert_eq!(validate_vrm1(&path), Ok(()));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn missing_file_is_file_missing() {
        let path = std::env::temp_dir().join("avatar_errors_does_not_exist.vrm");
        let _ = std::fs::remove_file(&path);
        assert_eq!(
            validate_vrm1(&path),
            Err(AvatarError::FileMissing(path.clone()))
        );
    }

    #[test]
    fn text_file_is_not_vrm1() {
        let path = temp_file("text", b"hello, this is not a model");
        match validate_vrm1(&path) {
            Err(AvatarError::NotVrm1 { path: p, .. }) => assert_eq!(p, path),
            other => panic!("expected NotVrm1, got {other:?}"),
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn vrm0_file_gives_0x_reason() {
        let path = temp_file(
            "vrm0",
            &glb(r#"{"asset":{"version":"2.0"},"extensions":{"VRM":{"specVersion":"0.0"}}}"#),
        );
        match validate_vrm1(&path) {
            Err(AvatarError::NotVrm1 { reason, .. }) => {
                assert_eq!(reason, "VRM 0.x is not supported, expected VRM 1.0");
            }
            other => panic!("expected NotVrm1, got {other:?}"),
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn truncated_header_is_not_vrm1_without_panic() {
        let path = temp_file("truncated", b"glTF\x02");
        assert!(matches!(
            validate_vrm1(&path),
            Err(AvatarError::NotVrm1 { .. })
        ));
        let _ = std::fs::remove_file(&path);

        let header_only = temp_file("header_only", &b"glTF\x02\0\0\0\x20\0\0\0"[..]);
        assert!(matches!(
            validate_vrm1(&header_only),
            Err(AvatarError::NotVrm1 { .. })
        ));
        let _ = std::fs::remove_file(&header_only);
    }

    #[test]
    fn bare_vrm1_lists_every_limit() {
        let path = temp_file("bare", &glb(VRM1_JSON));
        let w = vrm_warnings(&path);
        let _ = std::fs::remove_file(&path);
        assert_eq!(w.len(), 3, "{w:?}");
        assert!(w[0].contains("leftHand") && !w[0].contains("leftEye"), "eye bones are not needed without bone look-at");
        assert!(w[1].contains("blinkLeft"));
        assert!(w[2].contains("morph"));
    }

    #[test]
    fn eye_bones_are_checked_only_for_bone_type_look_at() {
        let make = |look_at: &str| {
            let bones: Vec<String> = TRACKED_BONES.iter().map(|b| format!("\"{b}\":{{\"node\":1}}")).collect();
            format!(
                "{{\"asset\":{{\"version\":\"2.0\"}},\"extensions\":{{\"VRMC_vrm\":{{\"specVersion\":\"1.0\",\
                 \"humanoid\":{{\"humanBones\":{{{}}}}},\"lookAt\":{{\"type\":\"{look_at}\"}}}}}}}}",
                bones.join(",")
            )
        };
        for (look_at, expect_eye_warning) in [("expression", false), ("bone", true)] {
            let path = temp_file(look_at, &glb(&make(look_at)));
            let w = vrm_warnings(&path);
            let _ = std::fs::remove_file(&path);
            assert_eq!(w.iter().any(|t| t.contains("leftEye")), expect_eye_warning, "{look_at}: {w:?}");
        }
    }

    #[test]
    fn complete_vrm1_has_no_warnings() {
        let bones: Vec<String> = TRACKED_BONES.iter().map(|b| format!("\"{b}\":{{\"node\":1}}")).collect();
        let exprs: Vec<String> = MAPPED_EXPRESSIONS.iter().map(|e| format!("\"{e}\":{{}}")).collect();
        let json = format!(
            "{{\"asset\":{{\"version\":\"2.0\"}},\"meshes\":[{{\"primitives\":[{{\"targets\":[{{}}]}}]}}],\
             \"extensions\":{{\"VRMC_vrm\":{{\"specVersion\":\"1.0\",\"humanoid\":{{\"humanBones\":{{{}}}}},\
             \"expressions\":{{\"preset\":{{{}}}}}}}}}}}",
            bones.join(","),
            exprs.join(",")
        );
        let path = temp_file("complete", &glb(&json));
        let w = vrm_warnings(&path);
        let _ = std::fs::remove_file(&path);
        assert!(w.is_empty(), "{w:?}");
    }

    #[test]
    fn describe_graphics_failure_wraps_raw_text() {
        let e = describe_graphics_failure("  Unable to find a GPU!  ");
        assert_eq!(e, AvatarError::NoGraphicsApi("Unable to find a GPU!".into()));
    }
}
