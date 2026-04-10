use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{anyhow, Context, Result};

use crate::{model::AppSettings, settings};

pub fn is_configured(settings: &AppSettings) -> bool {
    !settings.whisper_model_path.trim().is_empty() && resolve_binary(settings).is_ok()
}

pub fn transcribe_audio(input: &Path, settings: &AppSettings) -> Result<String> {
    let binary = resolve_binary(settings)?;
    let model = resolve_model_path(settings)?;
    let output_base = input.with_extension("");
    let output_txt = output_base.with_extension("txt");
    let mut base_command = Command::new(&binary);

    base_command.args(["-m", &model.display().to_string()]);
    base_command.args(["-f", &input.display().to_string()]);
    if !settings.whisper_language.trim().is_empty() {
        base_command.args(["-l", settings.whisper_language.trim()]);
    }

    let primary = base_command
        .args([
            "-otxt",
            "-of",
            &output_base.display().to_string(),
            "-nt",
            "-np",
        ])
        .output()
        .with_context(|| format!("failed to launch {}", binary.display()))?;

    if primary.status.success() {
        if output_txt.exists() {
            let transcript = fs::read_to_string(&output_txt)
                .with_context(|| format!("failed to read {}", output_txt.display()))?;
            let _ = fs::remove_file(&output_txt);
            let cleaned = transcript.trim().to_string();
            if !cleaned.is_empty() {
                return Ok(cleaned);
            }
        }
    }

    let mut fallback_command = Command::new(&binary);
    fallback_command.args(["-m", &model.display().to_string()]);
    fallback_command.args(["-f", &input.display().to_string()]);
    if !settings.whisper_language.trim().is_empty() {
        fallback_command.args(["-l", settings.whisper_language.trim()]);
    }

    let fallback = fallback_command
        .output()
        .with_context(|| format!("failed to launch {}", binary.display()))?;

    if !fallback.status.success() {
        let stderr = String::from_utf8_lossy(&fallback.stderr).trim().to_string();
        let primary_stderr = String::from_utf8_lossy(&primary.stderr).trim().to_string();
        let message = if !stderr.is_empty() {
            stderr
        } else if !primary_stderr.is_empty() {
            primary_stderr
        } else {
            format!("whisper-cli exited with status {}", fallback.status)
        };
        return Err(anyhow!(message));
    }

    let stdout = String::from_utf8_lossy(&fallback.stdout);
    let transcript = parse_transcript(&stdout);
    if transcript.is_empty() {
        return Err(anyhow!(
            "whisper-cli finished without returning a transcript"
        ));
    }

    Ok(transcript)
}

fn resolve_binary(settings: &AppSettings) -> Result<PathBuf> {
    let configured = settings.whisper_binary_path.trim();
    if !configured.is_empty() {
        let path = settings::expand_path(configured);
        if path.exists() {
            return Ok(path);
        }
        return Err(anyhow!(
            "Local Whisper binary was not found at {}",
            path.display()
        ));
    }

    if let Some(path) = settings::default_whisper_binary_path() {
        return Ok(PathBuf::from(path));
    }

    let which_output = Command::new("which")
        .arg("whisper-cli")
        .output()
        .context("failed to look up whisper-cli in PATH")?;

    if which_output.status.success() {
        let resolved = String::from_utf8_lossy(&which_output.stdout)
            .trim()
            .to_string();
        if !resolved.is_empty() {
            return Ok(PathBuf::from(resolved));
        }
    }

    Err(anyhow!(
        "whisper-cli was not found. Install whisper.cpp or set the binary path in Feedback -> Settings."
    ))
}

fn resolve_model_path(settings: &AppSettings) -> Result<PathBuf> {
    let model = settings.whisper_model_path.trim();
    if model.is_empty() {
        return Err(anyhow!(
            "Choose a local Whisper model file in Settings before using Local Whisper."
        ));
    }

    let path = settings::expand_path(model);
    if !path.exists() {
        return Err(anyhow!(
            "Local Whisper model was not found at {}",
            path.display()
        ));
    }

    Ok(path)
}

fn parse_transcript(stdout: &str) -> String {
    let segments: Vec<String> = stdout
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                return None;
            }

            if trimmed.starts_with('[') && trimmed.contains("-->") {
                return trimmed
                    .split_once(']')
                    .map(|(_, text)| text.trim().to_string());
            }

            if trimmed.contains(':') {
                return None;
            }

            Some(trimmed.to_string())
        })
        .collect();

    segments.join(" ").trim().to_string()
}
