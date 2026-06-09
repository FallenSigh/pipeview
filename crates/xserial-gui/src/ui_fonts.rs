use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use eframe::egui::{self, FontData, FontDefinitions, FontFamily, FontId, Style, TextStyle};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

const FONT_SETTINGS_FILE_NAME: &str = "gui-fonts.json";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FontChoice {
    Auto,
    Default,
    System(String),
}

impl Default for FontChoice {
    fn default() -> Self {
        Self::Default
    }
}

#[derive(Clone, Debug)]
pub struct FontCandidate {
    pub id: String,
    pub label: String,
    pub display_label: String,
    pub path: String,
    pub likely_cjk: bool,
    pub search_key: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UiFontSettings {
    #[serde(default = "default_primary_choice")]
    pub primary_choice: FontChoice,
    #[serde(default = "default_fallback_choice")]
    pub fallback_choice: FontChoice,
    #[serde(default = "default_ui_font_size")]
    pub ui_font_size: f32,
    #[serde(default = "default_monospace_font_size")]
    pub monospace_font_size: f32,
    #[serde(default = "default_heading_font_size")]
    pub heading_font_size: f32,
}

impl Default for UiFontSettings {
    fn default() -> Self {
        Self {
            primary_choice: FontChoice::Auto,
            fallback_choice: FontChoice::Default,
            ui_font_size: 14.0,
            monospace_font_size: 14.0,
            heading_font_size: 20.0,
        }
    }
}

pub fn discover_font_candidates() -> Vec<FontCandidate> {
    discover_via_fc_list().unwrap_or_else(discover_via_fallback_paths)
}

pub fn load_font_settings() -> UiFontSettings {
    match load_font_settings_from_path(&font_settings_path()) {
        Ok(settings) => settings,
        Err(err) if err.kind() == io::ErrorKind::NotFound => UiFontSettings::default(),
        Err(err) => {
            warn!(error = %err, "Failed to load persisted font settings");
            UiFontSettings::default()
        }
    }
}

pub fn save_font_settings(settings: &UiFontSettings) {
    if let Err(err) = save_font_settings_to_path(settings, &font_settings_path()) {
        warn!(error = %err, "Failed to persist font settings");
    }
}

pub fn apply_font_settings(
    ctx: &egui::Context,
    settings: &UiFontSettings,
    candidates: &[FontCandidate],
) {
    let mut fonts = FontDefinitions::default();
    let mut inserted = Vec::new();

    for choice in [&settings.primary_choice, &settings.fallback_choice] {
        if let Some((font_name, font_bytes)) = load_selected_font(choice, candidates, &inserted) {
            fonts
                .font_data
                .insert(font_name.clone(), FontData::from_owned(font_bytes).into());
            inserted.push(font_name);
        }
    }

    if !inserted.is_empty() {
        if let Some(family) = fonts.families.get_mut(&FontFamily::Proportional) {
            for font_name in inserted.iter().rev() {
                family.insert(0, font_name.clone());
            }
        }
        if let Some(family) = fonts.families.get_mut(&FontFamily::Monospace) {
            for font_name in inserted.iter().rev() {
                family.insert(0, font_name.clone());
            }
        }
        info!(fonts = ?inserted, "Applied egui font chain");
    } else {
        if settings.primary_choice != FontChoice::Default
            || settings.fallback_choice != FontChoice::Default
        {
            warn!("No configured fonts could be loaded; falling back to egui default fonts");
        }
    }

    ctx.set_fonts(fonts);

    let mut style = (*ctx.global_style()).clone();
    apply_font_size(&mut style, settings);
    ctx.set_global_style(style);
}

pub fn font_choice_label(choice: &FontChoice, candidates: &[FontCandidate]) -> String {
    match choice {
        FontChoice::Auto => String::from("Auto"),
        FontChoice::Default => String::from("Default"),
        FontChoice::System(id) => candidates
            .iter()
            .find(|candidate| candidate.id == *id)
            .map(|candidate| candidate.label.clone())
            .unwrap_or_else(|| id.clone()),
    }
}

fn load_selected_font(
    choice: &FontChoice,
    candidates: &[FontCandidate],
    already_loaded: &[String],
) -> Option<(String, Vec<u8>)> {
    match choice {
        FontChoice::Default => None,
        FontChoice::Auto => {
            load_auto_font(candidates, already_loaded)
        }
        FontChoice::System(id) => {
            let candidate = candidates.iter().find(|candidate| &candidate.id == id)?;
            if already_loaded.iter().any(|loaded| loaded == &candidate.id) {
                return None;
            }
            match fs::read(&candidate.path) {
                Ok(bytes) => Some((candidate.id.clone(), bytes)),
                Err(err) => {
                    warn!(path = %candidate.path, error = %err, "Failed to load selected font");
                    None
                }
            }
        }
    }
}

fn load_auto_font(
    candidates: &[FontCandidate],
    already_loaded: &[String],
) -> Option<(String, Vec<u8>)> {
    for candidate in candidates.iter().filter(|candidate| candidate.likely_cjk) {
        if already_loaded.iter().any(|loaded| loaded == &candidate.id) {
            continue;
        }
        match fs::read(&candidate.path) {
            Ok(bytes) => return Some((candidate.id.clone(), bytes)),
            Err(err) => {
                warn!(path = %candidate.path, error = %err, "Failed to load preferred CJK font");
            }
        }
    }
    for candidate in candidates {
        if already_loaded.iter().any(|loaded| loaded == &candidate.id) {
            continue;
        }
        match fs::read(&candidate.path) {
            Ok(bytes) => return Some((candidate.id.clone(), bytes)),
            Err(err) => {
                warn!(path = %candidate.path, error = %err, "Failed to load candidate font");
            }
        }
    }
    None
}

fn apply_font_size(style: &mut Style, settings: &UiFontSettings) {
    style
        .text_styles
        .insert(TextStyle::Body, FontId::proportional(settings.ui_font_size));
    style
        .text_styles
        .insert(TextStyle::Button, FontId::proportional(settings.ui_font_size));
    style.text_styles.insert(
        TextStyle::Monospace,
        FontId::monospace(settings.monospace_font_size),
    );
    style
        .text_styles
        .insert(TextStyle::Small, FontId::proportional((settings.ui_font_size - 2.0).max(8.0)));
    style
        .text_styles
        .insert(TextStyle::Heading, FontId::proportional(settings.heading_font_size));
}

fn discover_via_fc_list() -> Option<Vec<FontCandidate>> {
    let output = Command::new("fc-list")
        .args([":", "file", "family", "style"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8(output.stdout).ok()?;
    let mut candidates = Vec::new();

    for line in stdout.lines() {
        let (path, rest) = line.split_once(": ")?;
        let (family_part, style_part) = rest.split_once(":style=").unwrap_or((rest, ""));
        let family = family_part
            .split(',')
            .find(|name| !name.trim().is_empty())
            .unwrap_or(family_part)
            .trim()
            .to_owned();
        let style = style_part
            .split(',')
            .find(|name| !name.trim().is_empty())
            .unwrap_or(style_part)
            .trim()
            .to_owned();
        let path = path.trim();
        if !Path::new(path).is_file() {
            continue;
        }

        let stem = Path::new(path)
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or("font");
        let id = sanitize_id(&format!("{family}_{style}_{stem}"));
        let label = if style.is_empty() || style == "Regular" {
            family.clone()
        } else {
            format!("{family} ({style})")
        };
        candidates.push(FontCandidate {
            id,
            label,
            display_label: font_display_label(&family, &style, path),
            path: path.to_owned(),
            likely_cjk: is_likely_cjk(&family, &style, path),
            search_key: build_search_key(&family, &style, path),
        });
    }

    candidates.sort_by(|a, b| a.label.cmp(&b.label).then(a.path.cmp(&b.path)));
    candidates.dedup_by(|a, b| a.path == b.path);
    (!candidates.is_empty()).then_some(candidates)
}

fn discover_via_fallback_paths() -> Vec<FontCandidate> {
    let fallback_paths = [
        "/usr/share/fonts/google-noto-sans-cjk-fonts/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/google-noto-sans-cjk-vf-fonts/NotoSansCJK-VF.ttc",
        "/usr/share/fonts/google-droid-sans-fonts/DroidSansFallbackFull.ttf",
    ];

    fallback_paths
        .into_iter()
        .filter_map(|path| {
            fs::metadata(path).ok().map(|_| {
                let stem = Path::new(path)
                    .file_stem()
                    .and_then(|name| name.to_str())
                    .unwrap_or("font")
                    .to_owned();
                FontCandidate {
                    id: sanitize_id(&stem),
                    label: stem.clone(),
                    display_label: format!("{stem}  [CJK]"),
                    path: path.to_owned(),
                    likely_cjk: true,
                    search_key: build_search_key(&stem, "Regular", path),
                }
            })
        })
        .collect()
}

fn font_display_label(family: &str, style: &str, path: &str) -> String {
    let likely_cjk = is_likely_cjk(family, style, path);
    let base = if style.is_empty() || style == "Regular" {
        family.to_owned()
    } else {
        format!("{family} ({style})")
    };
    if likely_cjk {
        format!("{base}  [CJK]")
    } else {
        base
    }
}

fn build_search_key(family: &str, style: &str, path: &str) -> String {
    format!("{family}\n{style}\n{path}").to_lowercase()
}

fn font_settings_path() -> PathBuf {
    if let Ok(path) = env::var("XDG_CONFIG_HOME") {
        let trimmed = path.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed).join("xserial").join(FONT_SETTINGS_FILE_NAME);
        }
    }

    if let Ok(home) = env::var("HOME") {
        let trimmed = home.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed)
                .join(".config")
                .join("xserial")
                .join(FONT_SETTINGS_FILE_NAME);
        }
    }

    PathBuf::from(FONT_SETTINGS_FILE_NAME)
}

fn load_font_settings_from_path(path: &Path) -> io::Result<UiFontSettings> {
    let text = fs::read_to_string(path)?;
    serde_json::from_str(&text).map_err(io::Error::other)
}

fn save_font_settings_to_path(settings: &UiFontSettings, path: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(settings).map_err(io::Error::other)?;
    fs::write(path, json)
}

const fn default_ui_font_size() -> f32 {
    14.0
}

const fn default_monospace_font_size() -> f32 {
    14.0
}

const fn default_heading_font_size() -> f32 {
    20.0
}

fn default_primary_choice() -> FontChoice {
    FontChoice::Auto
}

fn default_fallback_choice() -> FontChoice {
    FontChoice::Default
}

fn sanitize_id(input: &str) -> String {
    input
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect()
}

fn is_likely_cjk(family: &str, style: &str, path: &str) -> bool {
    let haystack = format!("{family} {style} {path}").to_lowercase();
    [
        "cjk",
        "noto sans sc",
        "noto sans tc",
        "noto sans jp",
        "noto sans kr",
        "source han",
        "sarasa",
        "wenquanyi",
        "fallback",
        "han",
        "hei",
        "kai",
        "song",
        "gothic",
        "mincho",
    ]
    .iter()
    .any(|needle| haystack.contains(needle))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn font_settings_roundtrip() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = env::temp_dir().join(format!("xserial-gui-font-settings-{unique}"));
        let path = dir.join("gui-fonts.json");
        let settings = UiFontSettings {
            primary_choice: FontChoice::System(String::from("primary")),
            fallback_choice: FontChoice::System(String::from("fallback")),
            ui_font_size: 15.5,
            monospace_font_size: 13.0,
            heading_font_size: 22.0,
        };

        save_font_settings_to_path(&settings, &path).unwrap();
        let loaded = load_font_settings_from_path(&path).unwrap();
        assert_eq!(loaded.primary_choice, settings.primary_choice);
        assert_eq!(loaded.fallback_choice, settings.fallback_choice);
        assert_eq!(loaded.ui_font_size, settings.ui_font_size);
        assert_eq!(loaded.monospace_font_size, settings.monospace_font_size);
        assert_eq!(loaded.heading_font_size, settings.heading_font_size);

        let _ = fs::remove_file(&path);
        let _ = fs::remove_dir_all(&dir);
    }
}
