use std::{env, fs, path::{Path, PathBuf}, process::Command};

use anyhow::{Context, Result};
use keyring::{Entry, Error as KeyringError};
use tauri::{AppHandle, Manager};

use crate::{
    model::{AppSettings, AppSettingsSavePayload, AppSettingsView, TranscriptionProvider},
    openai,
};

const SETTINGS_FILE_NAME: &str = "settings.json";
const OPENAI_KEYCHAIN_SERVICE: &str = "com.alexpickett.imagediction.openai-api-key";
const OPENAI_KEYCHAIN_ACCOUNT: &str = "openai";
const DEFAULT_WHISPER_MODEL_FILE: &str = "ggml-base.en.bin";
const DEFAULT_WHISPER_MODEL_URL: &str =
    "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin";

pub fn load_settings(app: &AppHandle) -> Result<AppSettings> {
    openai::load_env();

    let path = settings_path(app)?;
    if !path.exists() {
        return Ok(default_settings());
    }

    let contents = fs::read_to_string(&path)
        .with_context(|| format!("failed to read settings file {}", path.display()))?;
    let mut settings: AppSettings =
        serde_json::from_str(&contents).context("failed to parse settings.json")?;

    normalize_settings(&mut settings);
    Ok(settings)
}

pub fn load_settings_view(app: &AppHandle) -> Result<AppSettingsView> {
    let settings = load_settings(app)?;
    Ok(AppSettingsView {
        transcription_provider: settings.transcription_provider,
        openai_model: settings.openai_model,
        openai_base_url: settings.openai_base_url,
        openai_prompt: settings.openai_prompt,
        whisper_binary_path: settings.whisper_binary_path,
        whisper_model_path: settings.whisper_model_path,
        whisper_language: settings.whisper_language,
        has_openai_api_key: has_openai_api_key(app)?,
        config_path: settings_path(app)?.display().to_string(),
    })
}

pub fn save_settings(app: &AppHandle, payload: AppSettingsSavePayload) -> Result<AppSettingsView> {
    let settings = AppSettings {
        transcription_provider: payload.transcription_provider,
        openai_model: payload.openai_model.trim().to_string(),
        openai_base_url: payload.openai_base_url.trim().to_string(),
        openai_prompt: payload.openai_prompt.trim().to_string(),
        whisper_binary_path: payload.whisper_binary_path.trim().to_string(),
        whisper_model_path: payload.whisper_model_path.trim().to_string(),
        whisper_language: payload.whisper_language.trim().to_string(),
    };

    persist_settings(app, &settings)?;

    if payload.clear_openai_api_key {
        delete_openai_api_key()?;
    } else if let Some(api_key) = payload
        .openai_api_key
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        save_openai_api_key(&api_key)?;
    }

    load_settings_view(app)
}

pub fn install_local_whisper(app: &AppHandle) -> Result<AppSettingsView> {
    let brew = resolve_brew()?;
    ensure_whisper_formula(&brew)?;

    let binary = resolve_whisper_binary(&brew)?;
    let models_dir = local_whisper_models_dir(app)?;
    fs::create_dir_all(&models_dir)
        .with_context(|| format!("failed to create {}", models_dir.display()))?;

    let model_path = models_dir.join(DEFAULT_WHISPER_MODEL_FILE);
    if !model_path.exists() {
        download_whisper_model(&model_path)?;
    }

    let mut settings = load_settings(app)?;
    settings.transcription_provider = TranscriptionProvider::LocalWhisper;
    settings.whisper_binary_path = binary.display().to_string();
    settings.whisper_model_path = model_path.display().to_string();
    if settings.whisper_language.trim().is_empty() {
        settings.whisper_language = "en".to_string();
    }
    persist_settings(app, &settings)?;

    load_settings_view(app)
}

pub fn has_openai_api_key(app: &AppHandle) -> Result<bool> {
    Ok(load_openai_api_key(app)?.is_some())
}

pub fn load_openai_api_key(app: &AppHandle) -> Result<Option<String>> {
    if let Some(key) = load_openai_api_key_from_keychain()? {
        return Ok(Some(key));
    }

    openai::load_env();
    let env_key = env::var("OPENAI_API_KEY")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    if env_key.is_some() {
        return Ok(env_key);
    }

    let _ = app;
    Ok(None)
}

pub fn settings_path(app: &AppHandle) -> Result<PathBuf> {
    let config_dir = app
        .path()
        .app_config_dir()
        .context("failed to resolve app config directory")?;
    Ok(config_dir.join(SETTINGS_FILE_NAME))
}

pub fn default_whisper_binary_path() -> Option<String> {
    [
        "/opt/homebrew/bin/whisper-cli",
        "/usr/local/bin/whisper-cli",
        "/usr/bin/whisper-cli",
    ]
    .into_iter()
    .map(PathBuf::from)
    .find(|path| path.exists())
    .map(|path| path.display().to_string())
}

pub fn expand_path(raw: &str) -> PathBuf {
    let trimmed = raw.trim();
    if trimmed == "~" {
        if let Some(home) = env::var_os("HOME") {
            return PathBuf::from(home);
        }
    }

    if let Some(rest) = trimmed.strip_prefix("~/") {
        if let Some(home) = env::var_os("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }

    PathBuf::from(trimmed)
}

fn default_settings() -> AppSettings {
    AppSettings {
        transcription_provider: TranscriptionProvider::OpenAi,
        openai_model: env::var("OPENAI_TRANSCRIPTION_MODEL")
            .unwrap_or_else(|_| openai::default_model().to_string()),
        openai_base_url: env::var("OPENAI_BASE_URL")
            .unwrap_or_else(|_| openai::default_base_url().to_string()),
        openai_prompt: env::var("OPENAI_TRANSCRIPTION_PROMPT").unwrap_or_default(),
        whisper_binary_path: default_whisper_binary_path().unwrap_or_default(),
        whisper_model_path: String::new(),
        whisper_language: String::new(),
    }
}

fn normalize_settings(settings: &mut AppSettings) {
    if settings.openai_model.trim().is_empty() {
        settings.openai_model = openai::default_model().to_string();
    }
    if settings.openai_base_url.trim().is_empty() {
        settings.openai_base_url = openai::default_base_url().to_string();
    }
}

fn persist_settings(app: &AppHandle, settings: &AppSettings) -> Result<()> {
    let path = settings_path(app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create settings directory {}", parent.display()))?;
    }

    let payload = serde_json::to_string_pretty(settings).context("failed to encode settings")?;
    fs::write(&path, payload)
        .with_context(|| format!("failed to write settings file {}", path.display()))?;
    Ok(())
}

fn save_openai_api_key(api_key: &str) -> Result<()> {
    keychain_entry()?
        .set_password(api_key)
        .context("failed to write OpenAI API key to Keychain")
}

fn load_openai_api_key_from_keychain() -> Result<Option<String>> {
    match keychain_entry().and_then(|entry| {
        entry
            .get_password()
            .context("failed to read OpenAI API key from Keychain")
    }) {
        Ok(value) if value.trim().is_empty() => Ok(None),
        Ok(value) => Ok(Some(value)),
        Err(error) => {
            if let Some(KeyringError::NoEntry) = error.downcast_ref::<KeyringError>() {
                return Ok(None);
            }
            Err(error)
        }
    }
}

fn delete_openai_api_key() -> Result<()> {
    match keychain_entry()?
        .delete_credential()
        .context("failed to delete OpenAI API key from Keychain")
    {
        Ok(_) => Ok(()),
        Err(error) => {
            if let Some(KeyringError::NoEntry) = error.downcast_ref::<KeyringError>() {
                return Ok(());
            }
            Err(error)
        }
    }
}

fn keychain_entry() -> Result<Entry> {
    Entry::new(OPENAI_KEYCHAIN_SERVICE, OPENAI_KEYCHAIN_ACCOUNT)
        .context("failed to prepare OpenAI Keychain entry")
}

fn resolve_brew() -> Result<PathBuf> {
    for candidate in ["/opt/homebrew/bin/brew", "/usr/local/bin/brew"] {
        let path = PathBuf::from(candidate);
        if path.exists() {
            return Ok(path);
        }
    }

    let output = Command::new("which")
        .arg("brew")
        .output()
        .context("failed to look up Homebrew")?;

    if output.status.success() {
        let resolved = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !resolved.is_empty() {
            return Ok(PathBuf::from(resolved));
        }
    }

    Err(anyhow::anyhow!(
        "Homebrew is required to install Local Whisper automatically."
    ))
}

fn ensure_whisper_formula(brew: &Path) -> Result<()> {
    let list = Command::new(brew)
        .args(["list", "--versions", "whisper-cpp"])
        .output()
        .context("failed to check whisper-cpp with Homebrew")?;

    if list.status.success() && !String::from_utf8_lossy(&list.stdout).trim().is_empty() {
        return Ok(());
    }

    let install = Command::new(brew)
        .args(["install", "whisper-cpp"])
        .output()
        .context("failed to launch Homebrew install for whisper-cpp")?;

    if install.status.success() {
        return Ok(());
    }

    Err(anyhow::anyhow!(
        homebrew_error("whisper-cpp", &install.stderr)
    ))
}

fn resolve_whisper_binary(brew: &Path) -> Result<PathBuf> {
    if let Some(path) = default_whisper_binary_path() {
        return Ok(PathBuf::from(path));
    }

    let which = Command::new("which")
        .arg("whisper-cli")
        .output()
        .context("failed to look up whisper-cli")?;
    if which.status.success() {
        let resolved = String::from_utf8_lossy(&which.stdout).trim().to_string();
        if !resolved.is_empty() {
            return Ok(PathBuf::from(resolved));
        }
    }

    let prefix = Command::new(brew)
        .args(["--prefix", "whisper-cpp"])
        .output()
        .context("failed to read whisper-cpp Homebrew prefix")?;
    if prefix.status.success() {
        let resolved = String::from_utf8_lossy(&prefix.stdout).trim().to_string();
        if !resolved.is_empty() {
            let candidate = PathBuf::from(resolved).join("bin").join("whisper-cli");
            if candidate.exists() {
                return Ok(candidate);
            }
        }
    }

    Err(anyhow::anyhow!(
        "whisper-cli was not found after installing whisper-cpp."
    ))
}

fn local_whisper_models_dir(app: &AppHandle) -> Result<PathBuf> {
    Ok(app
        .path()
        .app_data_dir()
        .context("failed to resolve app data directory")?
        .join("whisper-models"))
}

fn download_whisper_model(destination: &Path) -> Result<()> {
    let output = Command::new("curl")
        .args([
            "-L",
            "--fail",
            "--output",
            &destination.display().to_string(),
            DEFAULT_WHISPER_MODEL_URL,
        ])
        .output()
        .context("failed to launch curl for Whisper model download")?;

    if output.status.success() {
        return Ok(());
    }

    Err(anyhow::anyhow!(
        "Failed to download Whisper model: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    ))
}

fn homebrew_error(formula: &str, stderr: &[u8]) -> String {
    let detail = String::from_utf8_lossy(stderr).trim().to_string();
    if detail.is_empty() {
        format!("Homebrew could not install {}.", formula)
    } else {
        format!("Homebrew could not install {}: {}", formula, detail)
    }
}
