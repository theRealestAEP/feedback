use std::path::Path;

use anyhow::Result;
use tauri::AppHandle;

use crate::{
    model::{TranscriptionProvider, TranscriptionStatus},
    openai, settings, whisper,
};

pub fn transcription_status(app: &AppHandle) -> Result<TranscriptionStatus> {
    let settings = settings::load_settings(app)?;
    let openai_configured = settings::has_openai_api_key(app)?;
    let whisper_configured = whisper::is_configured(&settings);
    let (configured, message, fallback_provider, fallback_configured) =
        match settings.transcription_provider {
            TranscriptionProvider::OpenAi => (
                openai_configured,
                if openai_configured {
                    None
                } else {
                    Some(
                        "OpenAI is selected, but no API key is configured in Feedback -> Settings."
                            .to_string(),
                    )
                },
                Some("local_whisper".to_string()),
                whisper_configured,
            ),
            TranscriptionProvider::LocalWhisper => (
                whisper_configured,
                whisper::configuration_error(&settings),
                Some("openai".to_string()),
                openai_configured,
            ),
        };
    let model = match settings.transcription_provider {
        TranscriptionProvider::OpenAi => settings.openai_model.clone(),
        TranscriptionProvider::LocalWhisper => {
            if settings.whisper_model_path.trim().is_empty() {
                "Local Whisper".to_string()
            } else {
                settings.whisper_model_path.clone()
            }
        }
    };

    Ok(TranscriptionStatus {
        configured,
        provider: match settings.transcription_provider {
            TranscriptionProvider::OpenAi => "openai".to_string(),
            TranscriptionProvider::LocalWhisper => "local_whisper".to_string(),
        },
        model,
        message,
        fallback_provider,
        fallback_configured,
    })
}

pub fn transcribe_audio(app: &AppHandle, input: &Path) -> Result<String> {
    let settings = settings::load_settings(app)?;
    match settings.transcription_provider {
        TranscriptionProvider::OpenAi => {
            let api_key = settings::load_openai_api_key(app)?.ok_or_else(|| {
                anyhow::anyhow!(
                    "OpenAI API key is not configured. Add it in Feedback -> Settings."
                )
            })?;
            openai::transcribe_audio(input, &settings, &api_key)
        }
        TranscriptionProvider::LocalWhisper => whisper::transcribe_audio(input, &settings),
    }
}
