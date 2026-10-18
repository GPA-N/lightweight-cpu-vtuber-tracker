//! User settings: what the settings panel edits, saved as `settings.json` next to the program.
//! Basic options are the few most people touch; advanced options tune the tracker and renderer.
//! Every value is clamped on load and on change, so a hand-edited file cannot break the app.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const FILE_NAME: &str = "settings.json";

/// Background behind the avatar. Green and blue are for chroma-key capture in OBS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Background {
    Dark,
    Grey,
    Green,
    Blue,
    White,
}

impl Background {
    pub const ALL: [Background; 5] =
        [Background::Dark, Background::Grey, Background::Green, Background::Blue, Background::White];

    pub fn name(self) -> &'static str {
        match self {
            Background::Dark => "Dark",
            Background::Grey => "Grey",
            Background::Green => "Green (chroma key)",
            Background::Blue => "Blue (chroma key)",
            Background::White => "White",
        }
    }

    /// sRGB colour of the background.
    pub fn rgb(self) -> [f32; 3] {
        match self {
            Background::Dark => [0.2, 0.22, 0.26],
            Background::Grey => [0.5, 0.5, 0.5],
            Background::Green => [0.0, 0.69, 0.25],
            Background::Blue => [0.0, 0.28, 0.9],
            Background::White => [1.0, 1.0, 1.0],
        }
    }

    pub fn next(self) -> Background {
        let i = Self::ALL.iter().position(|b| *b == self).unwrap_or(0);
        Self::ALL[(i + 1) % Self::ALL.len()]
    }
}

/// Multisample anti-aliasing level. Off is cheapest; the live window measured no gain from more.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Antialiasing {
    Off,
    X2,
    X4,
}

impl Antialiasing {
    pub fn name(self) -> &'static str {
        match self {
            Antialiasing::Off => "Off",
            Antialiasing::X2 => "2x",
            Antialiasing::X4 => "4x",
        }
    }

    pub fn next(self) -> Antialiasing {
        match self {
            Antialiasing::Off => Antialiasing::X2,
            Antialiasing::X2 => Antialiasing::X4,
            Antialiasing::X4 => Antialiasing::Off,
        }
    }
}

/// Everything the panel can change. `#[serde(default)]` keeps old files working when options are added.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    // Basic
    pub background: Background,
    /// Name of the lighting preset (see `lighting::Preset::name`).
    pub lighting: String,
    /// One slider for the three confidence thresholds below; the panel sets all of them together.
    pub confidence: f32,
    pub show_preview: bool,
    pub show_debug_drawing: bool,
    // Advanced
    pub face_confidence: f32,
    pub body_confidence: f32,
    pub hand_confidence: f32,
    /// Multipliers on the preset's key, fill and ambient light.
    pub key_scale: f32,
    pub fill_scale: f32,
    pub ambient_scale: f32,
    pub antialiasing: Antialiasing,
    /// Render frame cap in frames per second.
    pub frame_cap: u32,
    /// Scale on eyebrow movement, 1.0 = as tracked.
    pub brow_gain: f32,
    /// Scale on eye (gaze) movement, 1.0 = as tracked.
    pub gaze_gain: f32,
    /// Camera size and mirroring take effect on the next start.
    pub camera_width: u32,
    pub camera_height: u32,
    pub mirror: bool,
    /// The last model that was opened, loaded again at the next start when no `--vrm` is given.
    pub last_model: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            background: Background::Dark,
            lighting: "Studio".into(),
            confidence: 0.8,
            show_preview: true,
            show_debug_drawing: true,
            face_confidence: 0.8,
            body_confidence: 0.8,
            hand_confidence: 0.8,
            key_scale: 1.0,
            fill_scale: 1.0,
            ambient_scale: 1.0,
            antialiasing: Antialiasing::Off,
            frame_cap: 30,
            brow_gain: 1.0,
            gaze_gain: 1.0,
            camera_width: 1280,
            camera_height: 720,
            mirror: true,
            last_model: None,
        }
    }
}

pub const CONFIDENCE_RANGE: (f32, f32) = (0.3, 0.99);
pub const LIGHT_SCALE_RANGE: (f32, f32) = (0.2, 2.0);
pub const GAIN_RANGE: (f32, f32) = (0.0, 2.0);
pub const FRAME_CAP_RANGE: (u32, u32) = (15, 120);

impl Settings {
    /// Puts every value back inside its allowed range (NaN becomes the default).
    pub fn clamped(mut self) -> Settings {
        let d = Settings::default();
        let fix = |v: f32, range: (f32, f32), default: f32| if v.is_finite() { v.clamp(range.0, range.1) } else { default };
        self.confidence = fix(self.confidence, CONFIDENCE_RANGE, d.confidence);
        self.face_confidence = fix(self.face_confidence, CONFIDENCE_RANGE, d.face_confidence);
        self.body_confidence = fix(self.body_confidence, CONFIDENCE_RANGE, d.body_confidence);
        self.hand_confidence = fix(self.hand_confidence, CONFIDENCE_RANGE, d.hand_confidence);
        self.key_scale = fix(self.key_scale, LIGHT_SCALE_RANGE, d.key_scale);
        self.fill_scale = fix(self.fill_scale, LIGHT_SCALE_RANGE, d.fill_scale);
        self.ambient_scale = fix(self.ambient_scale, LIGHT_SCALE_RANGE, d.ambient_scale);
        self.brow_gain = fix(self.brow_gain, GAIN_RANGE, d.brow_gain);
        self.gaze_gain = fix(self.gaze_gain, GAIN_RANGE, d.gaze_gain);
        self.frame_cap = self.frame_cap.clamp(FRAME_CAP_RANGE.0, FRAME_CAP_RANGE.1);
        self.camera_width = self.camera_width.clamp(320, 3840);
        self.camera_height = self.camera_height.clamp(240, 2160);
        self
    }

    /// Basic confidence slider: sets face, body and hand thresholds together.
    pub fn set_all_confidence(&mut self, value: f32) {
        let v = value.clamp(CONFIDENCE_RANGE.0, CONFIDENCE_RANGE.1);
        self.confidence = v;
        self.face_confidence = v;
        self.body_confidence = v;
        self.hand_confidence = v;
    }

    /// Reads `path`. A missing file gives the defaults; a file that does not parse gives the defaults
    /// and an error text the caller can show.
    pub fn load(path: &Path) -> (Settings, Option<String>) {
        match std::fs::read_to_string(path) {
            Ok(text) => match serde_json::from_str::<Settings>(&text) {
                Ok(s) => (s.clamped(), None),
                Err(e) => (Settings::default(), Some(format!("settings file ignored: {e}"))),
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Settings::default(), None),
            Err(e) => (Settings::default(), Some(format!("settings file unreadable: {e}"))),
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, text).map_err(|e| format!("cannot save {}: {e}", path.display()))
    }
}

/// The live settings in the Bevy world. The panel edits `value` (Bevy sees that through
/// `Res::is_changed`); the apply systems in `settings_apply.rs` react to the change.
#[derive(bevy::prelude::Resource, Debug, Clone)]
pub struct SettingsRes {
    pub value: Settings,
    pub path: PathBuf,
}

/// The settings the tracker threads read while they run. The window writes it when a setting changes.
/// Floats are stored as their bits in atomics so the threads never lock.
#[derive(Debug)]
pub struct LiveTuning {
    face: std::sync::atomic::AtomicU32,
    body: std::sync::atomic::AtomicU32,
    hand: std::sync::atomic::AtomicU32,
    preview: std::sync::atomic::AtomicBool,
    overlay: std::sync::atomic::AtomicBool,
}

impl LiveTuning {
    pub fn new(s: &Settings) -> Self {
        let t = LiveTuning {
            face: 0.into(),
            body: 0.into(),
            hand: 0.into(),
            preview: true.into(),
            overlay: true.into(),
        };
        t.apply(s);
        t
    }

    pub fn apply(&self, s: &Settings) {
        use std::sync::atomic::Ordering::Relaxed;
        self.face.store(s.face_confidence.to_bits(), Relaxed);
        self.body.store(s.body_confidence.to_bits(), Relaxed);
        self.hand.store(s.hand_confidence.to_bits(), Relaxed);
        self.preview.store(s.show_preview, Relaxed);
        self.overlay.store(s.show_debug_drawing, Relaxed);
    }

    pub fn face_confidence(&self) -> f32 {
        f32::from_bits(self.face.load(std::sync::atomic::Ordering::Relaxed))
    }

    pub fn body_confidence(&self) -> f32 {
        f32::from_bits(self.body.load(std::sync::atomic::Ordering::Relaxed))
    }

    pub fn hand_confidence(&self) -> f32 {
        f32::from_bits(self.hand.load(std::sync::atomic::Ordering::Relaxed))
    }

    pub fn show_preview(&self) -> bool {
        self.preview.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn show_debug_drawing(&self) -> bool {
        self.overlay.load(std::sync::atomic::Ordering::Relaxed)
    }
}

/// `settings.json` in the program's folder, or in the working folder when that is unknown.
pub fn default_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(FILE_NAME)))
        .unwrap_or_else(|| PathBuf::from(FILE_NAME))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("avatar_settings_{name}_{}.json", std::process::id()))
    }

    #[test]
    fn defaults_survive_a_save_and_load() {
        let path = temp("roundtrip");
        let mut s = Settings::default();
        s.background = Background::Green;
        s.set_all_confidence(0.9);
        s.save(&path).unwrap();
        let (loaded, err) = Settings::load(&path);
        let _ = std::fs::remove_file(&path);
        assert_eq!(err, None);
        assert_eq!(loaded, s);
    }

    #[test]
    fn missing_file_is_the_defaults_without_an_error() {
        let (s, err) = Settings::load(&temp("missing"));
        assert_eq!(s, Settings::default());
        assert_eq!(err, None);
    }

    #[test]
    fn broken_file_gives_defaults_and_a_message() {
        let path = temp("broken");
        std::fs::write(&path, "{ not json").unwrap();
        let (s, err) = Settings::load(&path);
        let _ = std::fs::remove_file(&path);
        assert_eq!(s, Settings::default());
        assert!(err.unwrap().contains("ignored"));
    }

    #[test]
    fn old_file_with_fewer_options_fills_the_rest_from_defaults() {
        let path = temp("old");
        std::fs::write(&path, r#"{ "background": "Blue", "confidence": 0.9 }"#).unwrap();
        let (s, _) = Settings::load(&path);
        let _ = std::fs::remove_file(&path);
        assert_eq!(s.background, Background::Blue);
        assert_eq!(s.confidence, 0.9);
        assert_eq!(s.frame_cap, 30);
    }

    #[test]
    fn out_of_range_values_are_clamped() {
        let mut s = Settings::default();
        s.confidence = 5.0;
        s.key_scale = f32::NAN;
        s.frame_cap = 1000;
        s.camera_width = 1;
        let s = s.clamped();
        assert_eq!(s.confidence, CONFIDENCE_RANGE.1);
        assert_eq!(s.key_scale, 1.0);
        assert_eq!(s.frame_cap, FRAME_CAP_RANGE.1);
        assert_eq!(s.camera_width, 320);
    }

    #[test]
    fn basic_confidence_sets_all_three_thresholds() {
        let mut s = Settings::default();
        s.set_all_confidence(0.6);
        assert_eq!((s.face_confidence, s.body_confidence, s.hand_confidence), (0.6, 0.6, 0.6));
    }

    #[test]
    fn the_last_model_is_remembered_and_old_files_have_none() {
        let path = temp("last_model");
        let mut s = Settings::default();
        assert_eq!(s.last_model, None);
        s.last_model = Some("C:/models/a.vrm".into());
        s.save(&path).unwrap();
        let (loaded, _) = Settings::load(&path);
        assert_eq!(loaded.last_model.as_deref(), Some("C:/models/a.vrm"));
        std::fs::write(&path, r#"{ "confidence": 0.9 }"#).unwrap();
        let (old, _) = Settings::load(&path);
        let _ = std::fs::remove_file(&path);
        assert_eq!(old.last_model, None);
    }

    #[test]
    fn live_tuning_follows_the_settings() {
        let mut s = Settings::default();
        let t = LiveTuning::new(&s);
        assert_eq!(t.body_confidence(), 0.8);
        s.body_confidence = 0.95;
        s.show_preview = false;
        t.apply(&s);
        assert_eq!(t.body_confidence(), 0.95);
        assert!(!t.show_preview() && t.show_debug_drawing());
    }

    #[test]
    fn background_cycles_through_every_choice_and_wraps() {
        let mut b = Background::Dark;
        for _ in 0..Background::ALL.len() {
            b = b.next();
        }
        assert_eq!(b, Background::Dark);
    }
}
