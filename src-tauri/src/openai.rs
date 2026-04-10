use std::{env, fs, path::Path};

use anyhow::{Context, Result};
use reqwest::blocking::{multipart, Client};
use serde::Deserialize;

use crate::model::AppSettings;

pub const DEFAULT_MODEL: &str = "gpt-4o-transcribe";
pub const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";

#[derive(Debug, Deserialize)]
struct AudioTranscriptionResponse {
    text: String,
}

pub fn load_env() {
    let _ = dotenvy::dotenv();

    if let Ok(manifest_dir) = env::var("CARGO_MANIFEST_DIR") {
        let repo_env = Path::new(&manifest_dir).join("../.env");
        if repo_env.exists() {
            let _ = dotenvy::from_path_override(repo_env);
        }
    }
}

pub fn default_model() -> &'static str {
    DEFAULT_MODEL
}

pub fn default_base_url() -> &'static str {
    DEFAULT_BASE_URL
}

pub fn transcribe_audio(input: &Path, settings: &AppSettings, api_key: &str) -> Result<String> {
    load_env();

    let base_url = if settings.openai_base_url.trim().is_empty() {
        DEFAULT_BASE_URL.to_string()
    } else {
        settings.openai_base_url.trim().to_string()
    };
    let model = if settings.openai_model.trim().is_empty() {
        DEFAULT_MODEL.to_string()
    } else {
        settings.openai_model.trim().to_string()
    };
    let prompt = if settings.openai_prompt.trim().is_empty() {
        env::var("OPENAI_TRANSCRIPTION_PROMPT").ok()
    } else {
        Some(settings.openai_prompt.clone())
    };
    let bytes = fs::read(input)
        .with_context(|| format!("failed to read audio clip {}", input.display()))?;
    let filename = input
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("audio.wav")
        .to_string();
    let mime = match input.extension().and_then(|value| value.to_str()) {
        Some("m4a") => "audio/m4a",
        Some("mp3") => "audio/mpeg",
        Some("webm") => "audio/webm",
        _ => "audio/wav",
    };

    let file_part = multipart::Part::bytes(bytes)
        .file_name(filename)
        .mime_str(mime)
        .context("failed to prepare audio payload")?;

    let mut form = multipart::Form::new()
        .text("model", model)
        .text("response_format", "json")
        .part("file", file_part);

    if let Some(prompt) = prompt.filter(|value| !value.trim().is_empty()) {
        form = form.text("prompt", prompt);
    }

    let client = Client::builder()
        .build()
        .context("failed to create OpenAI HTTP client")?;

    let response = client
        .post(format!("{base_url}/audio/transcriptions"))
        .bearer_auth(api_key)
        .multipart(form)
        .send()
        .context("failed to send OpenAI transcription request")?
        .error_for_status()
        .context("OpenAI transcription request failed")?;

    let payload: AudioTranscriptionResponse = response
        .json()
        .context("failed to parse OpenAI transcription response")?;

    Ok(payload.text)
}
