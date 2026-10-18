use nokhwa::utils::{ApiBackend, CameraInfo as NokhwaCameraInfo};

/// A webcam found by Media Foundation enumeration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CameraInfo {
    pub index: u32,
    pub name: String,
}

/// Why no camera could be opened or listed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CameraError {
    NoneFound,
    InUse { index: u32, name: String },
    Blocked,
    Other(String),
}

impl std::fmt::Display for CameraError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CameraError::NoneFound => write!(f, "no camera was found"),
            CameraError::InUse { index, name } => {
                write!(f, "camera {index} ({name}) is in use by another program")
            }
            CameraError::Blocked => write!(
                f,
                "camera access is blocked: check Windows Settings > Privacy & security > Camera"
            ),
            CameraError::Other(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for CameraError {}

impl CameraError {
    /// The error text followed by the cameras that were found, for the user.
    pub fn report(&self, available: &[CameraInfo]) -> String {
        format!("{self}; {}", describe_available(available))
    }
}

/// Lists the cameras Windows reports, in index order.
///
/// An empty enumeration is `NoneFound`. A failed enumeration is `Blocked` when the
/// text names access denial, otherwise `Other` with the underlying message.
pub fn list_cameras() -> Result<Vec<CameraInfo>, CameraError> {
    match nokhwa::query(ApiBackend::MediaFoundation) {
        Ok(infos) if infos.is_empty() => Err(CameraError::NoneFound),
        Ok(infos) => Ok(infos.iter().filter_map(to_camera_info).collect()),
        Err(e) => {
            let msg = e.to_string();
            if is_blocked_text(&msg) {
                Err(CameraError::Blocked)
            } else {
                Err(CameraError::Other(msg))
            }
        }
    }
}

// Entries whose index is not numeric are skipped. The Media Foundation backend
// always reports `CameraIndex::Index`, so none are dropped in practice.
fn to_camera_info(info: &NokhwaCameraInfo) -> Option<CameraInfo> {
    info.index().as_index().ok().map(|index| CameraInfo {
        index,
        name: info.human_name(),
    })
}

/// One line listing cameras by index and name, or that none exist.
pub fn describe_available(cameras: &[CameraInfo]) -> String {
    if cameras.is_empty() {
        return "no cameras available".to_string();
    }
    let list = cameras
        .iter()
        .map(|c| format!("{}: {}", c.index, c.name))
        .collect::<Vec<_>>()
        .join(", ");
    format!("available cameras: {list}")
}

/// Maps the text of an open failure to a camera error.
///
/// Blocked: `0x80070005` is the standard Windows `E_ACCESSDENIED` HRESULT, and its
/// English message is "Access is denied". In-use: matched on the words "busy" and
/// "in use" only. No HRESULT for the in-use case is verified against a source.
pub fn classify_open_error(msg: &str, index: u32, name: &str) -> CameraError {
    if is_blocked_text(msg) {
        return CameraError::Blocked;
    }
    let lower = msg.to_lowercase();
    if lower.contains("busy") || lower.contains("in use") {
        return CameraError::InUse {
            index,
            name: name.to_string(),
        };
    }
    CameraError::Other(msg.to_string())
}

fn is_blocked_text(msg: &str) -> bool {
    let lower = msg.to_lowercase();
    lower.contains("0x80070005") || lower.contains("access is denied")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cams(names: &[&str]) -> Vec<CameraInfo> {
        names
            .iter()
            .enumerate()
            .map(|(i, n)| CameraInfo {
                index: i as u32,
                name: n.to_string(),
            })
            .collect()
    }

    #[test]
    fn none_found_text_names_cause() {
        let e = CameraError::NoneFound;
        assert!(e.to_string().contains("no camera was found"));
        assert!(e.report(&[]).contains("no cameras available"));
    }

    #[test]
    fn in_use_text_names_camera_and_lists_available() {
        let e = CameraError::InUse {
            index: 1,
            name: "USB Camera".to_string(),
        };
        assert!(e
            .to_string()
            .contains("camera 1 (USB Camera) is in use by another program"));
        let text = e.report(&cams(&["Integrated Webcam", "USB Camera"]));
        assert!(text.contains("available cameras: 0: Integrated Webcam, 1: USB Camera"));
    }

    #[test]
    fn blocked_text_names_settings_and_lists_available() {
        let e = CameraError::Blocked;
        assert!(e
            .to_string()
            .contains("camera access is blocked: check Windows Settings > Privacy & security > Camera"));
        let text = e.report(&cams(&["Integrated Webcam"]));
        assert!(text.contains("available cameras: 0: Integrated Webcam"));
    }

    #[test]
    fn describe_available_for_0_1_2_cameras() {
        assert_eq!(describe_available(&[]), "no cameras available");
        assert_eq!(
            describe_available(&cams(&["Integrated Webcam"])),
            "available cameras: 0: Integrated Webcam"
        );
        assert_eq!(
            describe_available(&cams(&["Integrated Webcam", "USB Camera"])),
            "available cameras: 0: Integrated Webcam, 1: USB Camera"
        );
    }

    #[test]
    fn classify_access_denied_is_blocked() {
        let e = classify_open_error(
            "Could not open device: HRESULT 0x80070005 Access is denied",
            0,
            "Integrated Webcam",
        );
        assert_eq!(e, CameraError::Blocked);
    }

    #[test]
    fn classify_in_use_text_is_in_use() {
        let e = classify_open_error("device is busy", 1, "USB Camera");
        assert_eq!(
            e,
            CameraError::InUse {
                index: 1,
                name: "USB Camera".to_string()
            }
        );
    }

    #[test]
    fn classify_unknown_text_is_other() {
        let e = classify_open_error("something odd happened", 0, "Cam");
        assert_eq!(e, CameraError::Other("something odd happened".to_string()));
    }
}
