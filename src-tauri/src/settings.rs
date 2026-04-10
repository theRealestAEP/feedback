use std::{env, fs, path::PathBuf};

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
