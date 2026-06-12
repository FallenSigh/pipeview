use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tracing::warn;
use pipeview_client::SessionConfig;

const TUI_STATE_FILE_NAME: &str = "tui-state.json";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PersistedTuiState {
    #[serde(default)]
    pub sessions: Vec<SessionConfig>,
    #[serde(default)]
    pub active: usize,
    #[serde(default = "default_true")]
    pub show_timestamp: bool,
    #[serde(default = "default_true")]
    pub show_direction: bool,
    #[serde(default = "default_true")]
    pub show_pipeline: bool,
}

pub fn load_tui_state() -> PersistedTuiState {
    match load_tui_state_from_path(&tui_state_path()) {
        Ok(state) => state,
        Err(err) if err.kind() == io::ErrorKind::NotFound => PersistedTuiState::default(),
        Err(err) => {
            warn!(error = %err, "Failed to load persisted TUI state");
            PersistedTuiState::default()
        }
    }
}

pub fn save_tui_state(state: &PersistedTuiState) {
    if let Err(err) = save_tui_state_to_path(state, &tui_state_path()) {
        warn!(error = %err, "Failed to persist TUI state");
    }
}

fn tui_state_path() -> PathBuf {
    state_path_for_os_and_env(env::consts::OS, |key| env::var(key).ok())
}

fn state_path_for_os_and_env(os: &str, get_env: impl Fn(&str) -> Option<String>) -> PathBuf {
    match os {
        "windows" => {
            if let Some(path) = get_env("APPDATA").filter(|path| !path.trim().is_empty()) {
                return PathBuf::from(path)
                    .join("pipeview")
                    .join(TUI_STATE_FILE_NAME);
            }
            if let Some(path) = get_env("LOCALAPPDATA").filter(|path| !path.trim().is_empty()) {
                return PathBuf::from(path)
                    .join("pipeview")
                    .join(TUI_STATE_FILE_NAME);
            }
        }
        "macos" => {
            if let Some(home) = get_env("HOME").filter(|path| !path.trim().is_empty()) {
                return PathBuf::from(home)
                    .join("Library")
                    .join("Application Support")
                    .join("pipeview")
                    .join(TUI_STATE_FILE_NAME);
            }
        }
        _ => {
            if let Some(path) = get_env("XDG_CONFIG_HOME").filter(|path| !path.trim().is_empty()) {
                return PathBuf::from(path)
                    .join("pipeview")
                    .join(TUI_STATE_FILE_NAME);
            }
            if let Some(home) = get_env("HOME").filter(|path| !path.trim().is_empty()) {
                return PathBuf::from(home)
                    .join(".config")
                    .join("pipeview")
                    .join(TUI_STATE_FILE_NAME);
            }
        }
    }

    PathBuf::from(TUI_STATE_FILE_NAME)
}

fn load_tui_state_from_path(path: &Path) -> io::Result<PersistedTuiState> {
    let text = fs::read_to_string(path)?;
    serde_json::from_str(&text).map_err(io::Error::other)
}

fn save_tui_state_to_path(state: &PersistedTuiState, path: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(state).map_err(io::Error::other)?;
    fs::write(path, json)
}

const fn default_true() -> bool {
    true
}
