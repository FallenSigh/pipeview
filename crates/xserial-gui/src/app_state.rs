use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tracing::warn;
use xserial_client::config::SessionConfig;

const GUI_STATE_FILE_NAME: &str = "gui-state.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionLogConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub file_path: String,
}

impl Default for SessionLogConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            file_path: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PersistedGuiState {
    #[serde(default)]
    pub sessions: Vec<SessionConfig>,
    #[serde(default)]
    pub sessions_log: Vec<SessionLogConfig>,
    #[serde(default)]
    pub active: usize,
    #[serde(default = "default_true")]
    pub show_timestamp: bool,
    #[serde(default = "default_true")]
    pub show_direction: bool,
    #[serde(default = "default_true")]
    pub show_pipeline: bool,
}

pub fn load_gui_state() -> PersistedGuiState {
    match load_gui_state_from_path(&gui_state_path()) {
        Ok(state) => state,
        Err(err) if err.kind() == io::ErrorKind::NotFound => PersistedGuiState::default(),
        Err(err) => {
            warn!(error = %err, "Failed to load persisted GUI state");
            PersistedGuiState::default()
        }
    }
}

pub fn save_gui_state(state: &PersistedGuiState) {
    if let Err(err) = save_gui_state_to_path(state, &gui_state_path()) {
        warn!(error = %err, "Failed to persist GUI state");
    }
}

fn gui_state_path() -> PathBuf {
    gui_state_path_for_os_and_env(env::consts::OS, |key| env::var(key).ok())
}

/// Returns the xserial configuration directory (the parent of gui-state.json).
pub fn config_dir() -> PathBuf {
    gui_state_path()
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn gui_state_path_for_os_and_env(os: &str, get_env: impl Fn(&str) -> Option<String>) -> PathBuf {
    match os {
        "windows" => {
            if let Some(path) = get_env("APPDATA").filter(|path| !path.trim().is_empty()) {
                return PathBuf::from(path)
                    .join("xserial")
                    .join(GUI_STATE_FILE_NAME);
            }
            if let Some(path) = get_env("LOCALAPPDATA").filter(|path| !path.trim().is_empty()) {
                return PathBuf::from(path)
                    .join("xserial")
                    .join(GUI_STATE_FILE_NAME);
            }
        }
        "macos" => {
            if let Some(home) = get_env("HOME").filter(|path| !path.trim().is_empty()) {
                return PathBuf::from(home)
                    .join("Library")
                    .join("Application Support")
                    .join("xserial")
                    .join(GUI_STATE_FILE_NAME);
            }
        }
        _ => {
            if let Some(path) = get_env("XDG_CONFIG_HOME").filter(|path| !path.trim().is_empty()) {
                return PathBuf::from(path)
                    .join("xserial")
                    .join(GUI_STATE_FILE_NAME);
            }
            if let Some(home) = get_env("HOME").filter(|path| !path.trim().is_empty()) {
                return PathBuf::from(home)
                    .join(".config")
                    .join("xserial")
                    .join(GUI_STATE_FILE_NAME);
            }
        }
    }

    PathBuf::from(GUI_STATE_FILE_NAME)
}

fn load_gui_state_from_path(path: &Path) -> io::Result<PersistedGuiState> {
    let text = fs::read_to_string(path)?;
    serde_json::from_str(&text).map_err(io::Error::other)
}

fn save_gui_state_to_path(state: &PersistedGuiState, path: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(state).map_err(io::Error::other)?;
    fs::write(path, json)
}

const fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};
    use xserial_core::transport::TransportConfig;

    #[test]
    fn windows_gui_state_path_uses_appdata() {
        let path = gui_state_path_for_os_and_env("windows", |key| match key {
            "APPDATA" => Some(String::from(r"C:\Users\Test\AppData\Roaming")),
            _ => None,
        });
        assert_eq!(
            path,
            PathBuf::from(r"C:\Users\Test\AppData\Roaming")
                .join("xserial")
                .join(GUI_STATE_FILE_NAME)
        );
    }

    #[test]
    fn gui_state_roundtrip() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = env::temp_dir().join(format!("xserial-gui-state-{unique}"));
        let path = dir.join(GUI_STATE_FILE_NAME);
        let state = PersistedGuiState {
            sessions: vec![SessionConfig {
                transport: TransportConfig::Tcp {
                    addr: String::from("127.0.0.1:9000"),
                },
                ..SessionConfig::default()
            }],
            sessions_log: vec![SessionLogConfig::default()],
            active: 0,
            show_timestamp: false,
            show_direction: true,
            show_pipeline: false,
        };

        save_gui_state_to_path(&state, &path).unwrap();
        let loaded = load_gui_state_from_path(&path).unwrap();
        assert_eq!(loaded.sessions.len(), 1);
        assert!(!loaded.show_timestamp);
        assert!(loaded.show_direction);
        assert!(!loaded.show_pipeline);

        let _ = fs::remove_file(&path);
        let _ = fs::remove_dir_all(&dir);
    }
}
