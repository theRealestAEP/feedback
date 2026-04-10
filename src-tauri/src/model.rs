use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionMode {
    Dictation,
    CaptureNotes,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: String,
    pub title: String,
    pub mode: SessionMode,
    pub created_at: String,
    pub updated_at: String,
    pub entries: Vec<TimelineEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TimelineEntry {
    Capture(CaptureEntry),
    Dictation(DictationEntry),
    TextNote(TextNoteEntry),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureEntry {
    pub id: String,
    pub created_at: String,
    pub original_image_path: String,
    pub annotated_image_path: Option<String>,
    pub shapes: Vec<AnnotationShape>,
    pub bubble_note: Option<String>,
    pub bubble_anchor: Option<Point>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DictationEntry {
    pub id: String,
    pub created_at: String,
    pub audio_path: String,
    pub transcript: String,
    pub corrected_transcript: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextNoteEntry {
    pub id: String,
    pub created_at: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnnotationShape {
    pub id: String,
    pub kind: ShapeKind,
    pub start: Point,
    pub end: Point,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShapeKind {
    Arrow,
    Rectangle,
    Highlight,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    pub id: String,
    pub title: String,
    pub mode: SessionMode,
    pub created_at: String,
    pub updated_at: String,
    pub entry_count: usize,
    pub storage_path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionView {
    pub id: String,
    pub title: String,
    pub mode: SessionMode,
    pub created_at: String,
    pub updated_at: String,
    pub entries: Vec<TimelineEntryView>,
    pub storage_path: String,
    pub markdown_path: String,
    pub shortcut: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TimelineEntryView {
    Capture(CaptureEntryView),
    Dictation(DictationEntryView),
    TextNote(TextNoteEntry),
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureEntryView {
    pub id: String,
    pub created_at: String,
    pub original_image_path: String,
    pub annotated_image_path: Option<String>,
    pub shapes: Vec<AnnotationShape>,
    pub bubble_note: Option<String>,
    pub bubble_anchor: Option<Point>,
    pub original_image_data_url: String,
    pub annotated_image_data_url: Option<String>,
    pub display_image_data_url: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DictationEntryView {
    pub id: String,
    pub created_at: String,
    pub audio_path: String,
    pub transcript: String,
    pub corrected_transcript: Option<String>,
    pub audio_data_url: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureDraft {
    pub id: String,
    pub created_at: String,
    pub mode: SessionMode,
    pub original_image_path: String,
    pub original_image_data_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureSavePayload {
    pub id: String,
    pub created_at: String,
    pub original_image_path: String,
    pub shapes: Vec<AnnotationShape>,
    pub bubble_note: Option<String>,
    pub bubble_anchor: Option<Point>,
    pub annotated_image_data_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextNotePayload {
    pub id: Option<String>,
    pub created_at: Option<String>,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DictationSavePayload {
    pub id: Option<String>,
    pub created_at: Option<String>,
    pub audio_base64: Option<String>,
    pub transcript: Option<String>,
    pub corrected_transcript: Option<String>,
    pub audio_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionStatus {
    pub screen_recording: PermissionState,
    pub microphone: PermissionState,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionState {
    Unknown,
    Granted,
    Denied,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptionStatus {
    pub configured: bool,
    pub provider: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TranscriptionProvider {
    #[serde(rename = "openai", alias = "open_ai")]
    OpenAi,
    #[serde(rename = "local_whisper")]
    LocalWhisper,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub transcription_provider: TranscriptionProvider,
    pub openai_model: String,
    pub openai_base_url: String,
    pub openai_prompt: String,
    pub whisper_binary_path: String,
    pub whisper_model_path: String,
    pub whisper_language: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettingsView {
    pub transcription_provider: TranscriptionProvider,
    pub openai_model: String,
    pub openai_base_url: String,
    pub openai_prompt: String,
    pub whisper_binary_path: String,
    pub whisper_model_path: String,
    pub whisper_language: String,
    #[serde(rename = "hasOpenAiApiKey")]
    pub has_openai_api_key: bool,
    pub config_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettingsSavePayload {
    pub transcription_provider: TranscriptionProvider,
    pub openai_model: String,
    pub openai_base_url: String,
    pub openai_prompt: String,
    pub openai_api_key: Option<String>,
    #[serde(rename = "clearOpenAiApiKey")]
    pub clear_openai_api_key: bool,
    pub whisper_binary_path: String,
    pub whisper_model_path: String,
    pub whisper_language: String,
}

impl TimelineEntry {
    pub fn id(&self) -> &str {
        match self {
            Self::Capture(entry) => &entry.id,
            Self::Dictation(entry) => &entry.id,
            Self::TextNote(entry) => &entry.id,
        }
    }

    pub fn created_at(&self) -> &str {
        match self {
            Self::Capture(entry) => &entry.created_at,
            Self::Dictation(entry) => &entry.created_at,
            Self::TextNote(entry) => &entry.created_at,
        }
    }
}
