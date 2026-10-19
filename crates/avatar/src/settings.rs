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
    /// Software auto brightness on the camera frames (works on any camera).
    pub auto_brightness: bool,
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
    // Shading (VRM 1.0 models only). The values below change the model only while `shading_override` is on.
    pub shading_override: bool,
    /// Toon softness of the shade edge, 0.0 = hard, 1.0 = soft.
    pub shade_toony: f32,
    /// Moves the shade edge toward light (negative) or shadow (positive).
    pub shade_shift: f32,
    /// How dark the shade colour is, 1.0 = as the model, 0.0 = same as the lit colour.
    pub shade_strength: f32,
    /// Scale on the rim light, 1.0 = as the model.
    pub rim_strength: f32,
    /// Outline width scale, 0.0 = no outline.
    pub outline_width: f32,
    /// Global illumination equalisation, 0.0 = none, 1.0 = full.
    pub gi_equalization: f32,
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
            auto_brightness: true,
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
            shading_override: false,
            shade_toony: 0.9,
            shade_shift: 0.0,
            shade_strength: 1.0,
            rim_strength: 1.0,
            outline_width: 0.0,
            gi_equalization: 0.9,
            last_model: None,
        }
    }
}

pub const CONFIDENCE_RANGE: (f32, f32) = (0.3, 0.99);
pub const LIGHT_SCALE_RANGE: (f32, f32) = (0.2, 2.0);
pub const GAIN_RANGE: (f32, f32) = (0.0, 2.0);
pub const FRAME_CAP_RANGE: (u32, u32) = (15, 120);
pub const SHADE_RANGE: (f32, f32) = (0.0, 1.0);
pub const SHIFT_RANGE: (f32, f32) = (-1.0, 1.0);
pub const RIM_RANGE: (f32, f32) = (0.0, 2.0);
pub const OUTLINE_RANGE: (f32, f32) = (0.0, 3.0);
pub const GI_RANGE: (f32, f32) = (0.0, 1.0);

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
        self.shade_toony = fix(self.shade_toony, SHADE_RANGE, d.shade_toony);
        self.shade_shift = fix(self.shade_shift, SHIFT_RANGE, d.shade_shift);
        self.shade_strength = fix(self.shade_strength, SHADE_RANGE, d.shade_strength);
        self.rim_strength = fix(self.rim_strength, RIM_RANGE, d.rim_strength);
        self.outline_width = fix(self.outline_width, OUTLINE_RANGE, d.outline_width);
        self.gi_equalization = fix(self.gi_equalization, GI_RANGE, d.gi_equalization);
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
    /// Shared with the capture thread, which reads it for every frame.
    auto_brightness: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl LiveTuning {
    pub fn new(s: &Settings) -> Self {
        let t = LiveTuning {
            face: 0.into(),
            body: 0.into(),
            hand: 0.into(),
            preview: true.into(),
            overlay: true.into(),
            auto_brightness: std::sync::Arc::new(true.into()),
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
        self.auto_brightness.store(s.auto_brightness, Relaxed);
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

    /// The auto-brightness switch, to hand to the capture thread.
    pub fn auto_brightness_flag(&self) -> std::sync::Arc<std::sync::atomic::AtomicBool> {
        std::sync::Arc::clone(&self.auto_brightness)
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
    fn shading_defaults_keep_the_models_own_look() {
        let s = Settings::default();
        assert!(!s.shading_override);
        assert_eq!((s.shade_toony, s.shade_shift, s.shade_strength), (0.9, 0.0, 1.0));
        assert_eq!((s.rim_strength, s.outline_width, s.gi_equalization), (1.0, 0.0, 0.9));
    }

    #[test]
    fn shading_values_from_json_are_clamped() {
        let path = temp("shading_clamp");
        std::fs::write(
            &path,
            r#"{ "shade_toony": 5.0, "shade_shift": -3.0, "shade_strength": -1.0, "rim_strength": 9.0, "outline_width": -2.0, "gi_equalization": 2.0 }"#,
        )
        .unwrap();
        let (s, err) = Settings::load(&path);
        let _ = std::fs::remove_file(&path);
        assert_eq!(err, None);
        assert_eq!((s.shade_toony, s.shade_shift, s.shade_strength), (SHADE_RANGE.1, SHIFT_RANGE.0, SHADE_RANGE.0));
        assert_eq!((s.rim_strength, s.outline_width, s.gi_equalization), (RIM_RANGE.1, OUTLINE_RANGE.0, GI_RANGE.1));
    }

    #[test]
    fn shading_nan_falls_back_to_defaults() {
        let mut s = Settings::default();
        s.shade_toony = f32::NAN;
        s.rim_strength = f32::INFINITY;
        s.gi_equalization = f32::NAN;
        let s = s.clamped();
        assert_eq!((s.shade_toony, s.rim_strength, s.gi_equalization), (0.9, 1.0, 0.9));
    }

    #[test]
    fn old_file_without_shading_keys_keeps_its_other_values() {
        let path = temp("shading_old");
        std::fs::write(&path, r#"{ "background": "Blue", "mirror": false, "confidence": 0.9 }"#).unwrap();
        let (s, err) = Settings::load(&path);
        let _ = std::fs::remove_file(&path);
        assert_eq!(err, None);
        assert_eq!(s.background, Background::Blue);
        assert!(!s.mirror);
        assert_eq!(s.confidence, 0.9);
        assert!(!s.shading_override);
        assert_eq!((s.shade_toony, s.shade_strength, s.gi_equalization), (0.9, 1.0, 0.9));
    }

    #[test]
    fn shading_values_survive_a_save_and_load() {
        let path = temp("shading_roundtrip");
        let mut s = Settings::default();
        s.shading_override = true;
        s.shade_toony = 0.4;
        s.shade_shift = -0.25;
        s.shade_strength = 0.5;
        s.rim_strength = 1.5;
        s.outline_width = 2.0;
        s.gi_equalization = 0.0;
        s.save(&path).unwrap();
        let (loaded, err) = Settings::load(&path);
        let _ = std::fs::remove_file(&path);
        assert_eq!(err, None);
        assert_eq!(loaded, s);
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
