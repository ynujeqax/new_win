//! Per-user data locations.

use std::path::PathBuf;

/// `%LOCALAPPDATA%\NewWin`, created on demand.
pub fn local_data_dir() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    let dir = base.join("NewWin");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

pub fn taskbar_state_file() -> PathBuf {
    local_data_dir().join("taskbar-state.toml")
}

pub fn log_dir() -> PathBuf {
    let dir = local_data_dir().join("logs");
    let _ = std::fs::create_dir_all(&dir);
    dir
}
