use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
};

use anyhow::{Context, Result};
use chrono::Local;
use tauri::{AppHandle, Manager};

const DEBUG_LOG_FILE: &str = "debug.log";

fn log_dir(app: &AppHandle) -> Result<PathBuf> {
    if let Some(home) = std::env::var_os("HOME") {
        return Ok(PathBuf::from(home)
            .join("Library")
            .join("Logs")
            .join("Feedback"));
    }

    let fallback = app
        .path()
        .app_log_dir()
        .context("failed to resolve log directory")?;
    Ok(fallback)
}

pub fn log_path(app: &AppHandle) -> Result<PathBuf> {
    Ok(log_dir(app)?.join(DEBUG_LOG_FILE))
}

pub fn append(app: &AppHandle, message: &str) -> Result<()> {
    let path = log_path(app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create log directory {}", parent.display()))?;
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("failed to open log file {}", path.display()))?;

    writeln!(
        file,
        "{} {}",
        Local::now().format("%Y-%m-%d %H:%M:%S%.3f"),
        message
    )
    .with_context(|| format!("failed to write log file {}", path.display()))?;

    Ok(())
}

pub fn clear(app: &AppHandle) -> Result<PathBuf> {
    let path = log_path(app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create log directory {}", parent.display()))?;
    }
    fs::write(&path, "").with_context(|| format!("failed to clear log file {}", path.display()))?;
    Ok(path)
}

pub fn read(app: &AppHandle) -> Result<String> {
    let path = log_path(app)?;
    if !path.exists() {
        return Ok(String::new());
    }
    fs::read_to_string(&path).with_context(|| format!("failed to read log file {}", path.display()))
}
