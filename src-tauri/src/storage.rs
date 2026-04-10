use std::{
    env, fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use chrono::{Local, Utc};
use uuid::Uuid;

use crate::model::{
    CaptureDraft, CaptureEntry, CaptureEntryView, CaptureSavePayload, DictationEntry,
    DictationEntryView, DictationSavePayload, Session, SessionMode, SessionSummary, SessionView,
    TextNoteEntry, TextNotePayload, TimelineEntry, TimelineEntryView,
};

pub const SESSION_META_FILE: &str = "session.meta.json";
pub const SESSION_MARKDOWN_FILE: &str = "session.md";
pub const SESSION_ASSETS_DIR: &str = "assets";
pub const CAPTURE_SHORTCUT: &str = "CmdOrCtrl+Shift+4";

pub fn ensure_sessions_root(root: &Path) -> Result<()> {
    fs::create_dir_all(root).context("failed to create sessions root")
}

pub fn create_session(root: &Path, title: Option<String>, mode: SessionMode) -> Result<Session> {
    ensure_sessions_root(root)?;

    let now_local = Local::now();
    let created_at = Utc::now().to_rfc3339();
    let base_title = title
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| format!("Review {}", now_local.format("%b %d %H:%M")));
    let slug = slugify(&base_title);
    let id = format!("{}-{}", now_local.format("%Y-%m-%d-%H%M%S"), slug);
    let session = Session {
        id: id.clone(),
        title: base_title,
        mode,
        created_at: created_at.clone(),
        updated_at: created_at,
        entries: Vec::new(),
    };

    let paths = session_paths(root, &id);
    fs::create_dir_all(&paths.assets_dir)?;
    persist_session(root, &session)?;

    Ok(session)
}

pub fn load_session(root: &Path, session_id: &str) -> Result<Session> {
    let paths = session_paths(root, session_id);
    let content = fs::read_to_string(&paths.meta_path)
        .with_context(|| format!("failed to read session metadata for {session_id}"))?;
    let mut session: Session =
        serde_json::from_str(&content).context("failed to parse session metadata")?;
    session.entries.sort_by(|left, right| {
        left.created_at()
            .cmp(right.created_at())
            .then_with(|| left.id().cmp(right.id()))
    });
    Ok(session)
}

pub fn load_summaries(root: &Path) -> Result<Vec<SessionSummary>> {
    ensure_sessions_root(root)?;
    let mut summaries = Vec::new();

    for entry in fs::read_dir(root).context("failed to read sessions directory")? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }

        let session_id = entry.file_name().to_string_lossy().to_string();
        if let Ok(session) = load_session(root, &session_id) {
            summaries.push(SessionSummary {
                id: session.id.clone(),
                title: session.title.clone(),
                mode: session.mode.clone(),
                created_at: session.created_at.clone(),
                updated_at: session.updated_at.clone(),
                entry_count: session.entries.len(),
                storage_path: entry.path().display().to_string(),
            });
        }
    }

    summaries.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    Ok(summaries)
}

pub fn persist_session(root: &Path, session: &Session) -> Result<()> {
    let paths = session_paths(root, &session.id);
    fs::create_dir_all(&paths.assets_dir)?;

    let mut session = session.clone();
    session.entries.sort_by(|left, right| {
        left.created_at()
            .cmp(right.created_at())
            .then_with(|| left.id().cmp(right.id()))
    });

    fs::write(
        &paths.meta_path,
        serde_json::to_string_pretty(&session).context("failed to encode session metadata")?,
    )
    .context("failed to write session metadata")?;
    fs::write(&paths.markdown_path, render_markdown(&session))
        .context("failed to write session markdown")?;
    Ok(())
}

pub fn capture_draft_for_temp_file(
    mode: SessionMode,
    original_image_data_url: String,
) -> Result<CaptureDraft> {
    let id = Uuid::new_v4().to_string();
    let created_at = Utc::now().to_rfc3339();
    let capture_dir = env::temp_dir().join("imagediction-captures");
    fs::create_dir_all(&capture_dir).context("failed to create temporary capture directory")?;
    let original_absolute_path = capture_dir.join(format!("{id}-original.png"));

    Ok(CaptureDraft {
        id,
        created_at,
        mode,
        original_image_path: original_absolute_path.display().to_string(),
        original_image_data_url,
    })
}

pub fn save_capture_entry(
    root: &Path,
    session_id: &str,
    payload: CaptureSavePayload,
) -> Result<Session> {
    let mut session = load_session(root, session_id)?;
    let paths = session_paths(root, session_id);
    let original_relative_path =
        persist_capture_original_path(&paths.session_dir, &session, &payload)?;

    let annotated_relative_path = if let Some(data_url) = payload.annotated_image_data_url.as_ref()
    {
        let relative = asset_relative_path(&payload.id, "annotated", "png");
        let absolute = paths.session_dir.join(&relative);
        write_data_url(&absolute, data_url)?;
        Some(relative)
    } else {
        existing_capture(&session, &payload.id).and_then(|entry| entry.annotated_image_path.clone())
    };

    let entry = CaptureEntry {
        id: payload.id,
        created_at: payload.created_at,
        original_image_path: original_relative_path,
        annotated_image_path: annotated_relative_path,
        shapes: payload.shapes,
        bubble_note: payload.bubble_note.filter(|value| !value.trim().is_empty()),
        bubble_anchor: payload.bubble_anchor,
    };

    upsert_entry(&mut session.entries, TimelineEntry::Capture(entry));
    session.updated_at = Utc::now().to_rfc3339();
    persist_session(root, &session)?;
    Ok(session)
}

pub fn save_text_note(root: &Path, session_id: &str, payload: TextNotePayload) -> Result<Session> {
    let mut session = load_session(root, session_id)?;
    let entry = TextNoteEntry {
        id: payload.id.unwrap_or_else(|| Uuid::new_v4().to_string()),
        created_at: payload
            .created_at
            .unwrap_or_else(|| Utc::now().to_rfc3339()),
        text: payload.text.trim().to_string(),
    };

    upsert_entry(&mut session.entries, TimelineEntry::TextNote(entry));
    session.updated_at = Utc::now().to_rfc3339();
    persist_session(root, &session)?;
    Ok(session)
}

pub fn save_dictation_entry(
    root: &Path,
    session_id: &str,
    payload: DictationSavePayload,
    transcript: Option<String>,
) -> Result<Session> {
    let mut session = load_session(root, session_id)?;
    let paths = session_paths(root, session_id);
    let existing = payload
        .id
        .as_deref()
        .and_then(|id| existing_dictation(&session, id).cloned());

    let id = payload
        .id
        .clone()
        .or_else(|| existing.as_ref().map(|entry| entry.id.clone()))
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let created_at = payload
        .created_at
        .clone()
        .or_else(|| existing.as_ref().map(|entry| entry.created_at.clone()))
        .unwrap_or_else(|| Utc::now().to_rfc3339());

    let audio_relative_path = if let Some(audio_base64) = payload.audio_base64.as_ref() {
        let relative = asset_relative_path(&id, "clip", "wav");
        let absolute = paths.session_dir.join(&relative);
        write_base64_file(&absolute, audio_base64)?;
        relative
    } else if let Some(audio_path) = payload.audio_path.as_ref() {
        absolutize_to_relative(&paths.session_dir, audio_path)?
    } else if let Some(existing) = existing.as_ref() {
        existing.audio_path.clone()
    } else {
        asset_relative_path(&id, "clip", "wav")
    };

    let base_transcript = payload
        .transcript
        .clone()
        .or(transcript)
        .or_else(|| existing.as_ref().map(|entry| entry.transcript.clone()))
        .unwrap_or_else(|| "Transcription unavailable".to_string());

    let corrected_transcript = payload
        .corrected_transcript
        .clone()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            existing
                .as_ref()
                .and_then(|entry| entry.corrected_transcript.clone())
        })
        .or_else(|| Some(base_transcript.clone()));

    let entry = DictationEntry {
        id,
        created_at,
        audio_path: audio_relative_path,
        transcript: base_transcript,
        corrected_transcript,
    };

    upsert_entry(&mut session.entries, TimelineEntry::Dictation(entry));
    session.updated_at = Utc::now().to_rfc3339();
    persist_session(root, &session)?;
    Ok(session)
}

pub fn session_to_view(root: &Path, session: Session) -> Result<SessionView> {
    let paths = session_paths(root, &session.id);
    let mut entries = Vec::new();

    for entry in &session.entries {
        match entry {
            TimelineEntry::Capture(item) => {
                let original_absolute = paths.session_dir.join(&item.original_image_path);
                let annotated_absolute = item
                    .annotated_image_path
                    .as_ref()
                    .map(|value| paths.session_dir.join(value));
                let original_image_data_url = file_to_data_url(&original_absolute, "image/png")?;
                let annotated_image_data_url = annotated_absolute
                    .as_ref()
                    .map(|value| file_to_data_url(value, "image/png"))
                    .transpose()?;
                let display_image_data_url = annotated_image_data_url
                    .clone()
                    .unwrap_or_else(|| original_image_data_url.clone());

                entries.push(TimelineEntryView::Capture(CaptureEntryView {
                    id: item.id.clone(),
                    created_at: item.created_at.clone(),
                    original_image_path: original_absolute.display().to_string(),
                    annotated_image_path: annotated_absolute
                        .as_ref()
                        .map(|value| value.display().to_string()),
                    shapes: item.shapes.clone(),
                    bubble_note: item.bubble_note.clone(),
                    bubble_anchor: item.bubble_anchor.clone(),
                    original_image_data_url,
                    annotated_image_data_url,
                    display_image_data_url,
                }));
            }
            TimelineEntry::Dictation(item) => {
                let audio_absolute = paths.session_dir.join(&item.audio_path);
                entries.push(TimelineEntryView::Dictation(DictationEntryView {
                    id: item.id.clone(),
                    created_at: item.created_at.clone(),
                    audio_path: audio_absolute.display().to_string(),
                    transcript: item.transcript.clone(),
                    corrected_transcript: item.corrected_transcript.clone(),
                    audio_data_url: file_to_data_url(
                        &audio_absolute,
                        audio_mime_type(&audio_absolute),
                    )?,
                }));
            }
            TimelineEntry::TextNote(item) => {
                entries.push(TimelineEntryView::TextNote(item.clone()));
            }
        }
    }

    Ok(SessionView {
        id: session.id.clone(),
        title: session.title.clone(),
        mode: session.mode.clone(),
        created_at: session.created_at.clone(),
        updated_at: session.updated_at.clone(),
        entries,
        storage_path: paths.session_dir.display().to_string(),
        markdown_path: paths.markdown_path.display().to_string(),
        shortcut: CAPTURE_SHORTCUT.to_string(),
    })
}

pub fn session_root(root: &Path, session_id: &str) -> PathBuf {
    root.join(session_id)
}

pub fn asset_absolute_path(root: &Path, session_id: &str, relative: &str) -> PathBuf {
    session_paths(root, session_id).session_dir.join(relative)
}

fn render_markdown(session: &Session) -> String {
    let mut entries = session.entries.clone();
    entries.sort_by(|left, right| {
        left.created_at()
            .cmp(right.created_at())
            .then_with(|| left.id().cmp(right.id()))
    });

    let mut blocks = Vec::new();
    let mut index = 0;

    while index < entries.len() {
        match &entries[index] {
            TimelineEntry::Dictation(item) => {
                let next_dictation_created_at = entries[index + 1..].iter().find_map(|entry| {
                    if let TimelineEntry::Dictation(dictation) = entry {
                        Some(dictation.created_at.as_str())
                    } else {
                        None
                    }
                });
                let end_time = next_dictation_created_at.unwrap_or(&session.updated_at);
                let mut inserts = Vec::new();
                let mut next_index = index + 1;

                while next_index < entries.len() {
                    if matches!(entries[next_index], TimelineEntry::Dictation(_)) {
                        break;
                    }

                    inserts.push(entries[next_index].clone());
                    next_index += 1;
                }

                blocks.extend(render_dictation_blocks(item, &inserts, end_time));
                index = next_index;
            }
            entry => {
                blocks.extend(render_entry_blocks(entry));
                index += 1;
            }
        }
    }

    let content = blocks
        .into_iter()
        .map(|block| block.trim().to_string())
        .filter(|block| !block.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");

    if content.is_empty() {
        String::new()
    } else {
        format!("{content}\n")
    }
}

fn render_dictation_blocks(
    entry: &DictationEntry,
    inserts: &[TimelineEntry],
    dictation_end: &str,
) -> Vec<String> {
    let transcript = preferred_transcript(entry);
    if transcript.is_empty() {
        return inserts
            .iter()
            .flat_map(render_entry_blocks)
            .collect::<Vec<_>>();
    }

    if inserts.is_empty() {
        return split_markdown_blocks(&transcript);
    }

    let units = split_transcript_units(&transcript);
    if units.is_empty() {
        let mut blocks = vec![transcript];
        blocks.extend(
            inserts
                .iter()
                .flat_map(render_entry_blocks)
                .collect::<Vec<String>>(),
        );
        return blocks;
    }

    let start_time = chrono::DateTime::parse_from_rfc3339(&entry.created_at).ok();
    let end_time = chrono::DateTime::parse_from_rfc3339(dictation_end).ok();

    let Some(start_time) = start_time else {
        let mut blocks = vec![transcript];
        blocks.extend(
            inserts
                .iter()
                .flat_map(render_entry_blocks)
                .collect::<Vec<String>>(),
        );
        return blocks;
    };

    let Some(end_time) = end_time else {
        let mut blocks = vec![transcript];
        blocks.extend(
            inserts
                .iter()
                .flat_map(render_entry_blocks)
                .collect::<Vec<String>>(),
        );
        return blocks;
    };

    let total_millis = end_time
        .signed_duration_since(start_time)
        .num_milliseconds()
        .max(1) as f64;

    let mut blocks = Vec::new();
    let mut last_boundary = 0usize;

    for insert in inserts {
        let insert_time = chrono::DateTime::parse_from_rfc3339(insert.created_at()).ok();
        let boundary = insert_time
            .map(|timestamp| {
                let elapsed = timestamp
                    .signed_duration_since(start_time)
                    .num_milliseconds()
                    .clamp(0, total_millis as i64) as f64;
                ((elapsed / total_millis) * units.len() as f64).round() as usize
            })
            .unwrap_or(last_boundary)
            .clamp(last_boundary, units.len());

        if boundary > last_boundary {
            blocks.push(join_transcript_units(&units[last_boundary..boundary]));
        }

        blocks.extend(render_entry_blocks(insert));
        last_boundary = boundary;
    }

    if last_boundary < units.len() {
        blocks.push(join_transcript_units(&units[last_boundary..]));
    }

    blocks
}

fn render_entry_blocks(entry: &TimelineEntry) -> Vec<String> {
    match entry {
        TimelineEntry::Capture(item) => {
            let mut blocks = vec![format!(
                "![]({})",
                item.annotated_image_path
                    .as_deref()
                    .unwrap_or(&item.original_image_path)
            )];

            if let Some(note) = clean_text_block(item.bubble_note.as_deref()) {
                blocks.push(note);
            }

            blocks
        }
        TimelineEntry::TextNote(item) => split_markdown_blocks(&item.text),
        TimelineEntry::Dictation(item) => split_markdown_blocks(&preferred_transcript(item)),
    }
}

fn preferred_transcript(entry: &DictationEntry) -> String {
    let corrected = clean_text_block(entry.corrected_transcript.as_deref());
    let transcript = clean_text_block(Some(&entry.transcript));

    match (corrected, transcript) {
        (Some(corrected), Some(transcript))
            if is_transcription_placeholder(&corrected)
                && !is_transcription_placeholder(&transcript) =>
        {
            transcript
        }
        (Some(corrected), _) => corrected,
        (None, Some(transcript)) => transcript,
        (None, None) => String::new(),
    }
}

fn clean_text_block(raw: Option<&str>) -> Option<String> {
    raw.map(|value| value.replace("\r\n", "\n"))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn split_markdown_blocks(text: &str) -> Vec<String> {
    text.replace("\r\n", "\n")
        .split("\n\n")
        .map(|block| block.trim())
        .filter(|block| !block.is_empty())
        .map(|block| block.to_string())
        .collect()
}

fn is_transcription_placeholder(text: &str) -> bool {
    let trimmed = text.trim();
    trimmed.eq_ignore_ascii_case("Transcription unavailable")
        || trimmed.starts_with("Transcription failed:")
}

fn split_transcript_units(text: &str) -> Vec<String> {
    let sentences = split_sentences(text);
    if sentences.len() >= 2 {
        return sentences;
    }

    split_word_chunks(text, 14)
}

fn split_sentences(text: &str) -> Vec<String> {
    let normalized = text.replace('\n', " ");
    let mut sentences = Vec::new();
    let mut current = String::new();
    let mut chars = normalized.chars().peekable();

    while let Some(ch) = chars.next() {
        current.push(ch);

        if matches!(ch, '.' | '!' | '?') {
            while matches!(chars.peek(), Some(next) if next.is_whitespace()) {
                chars.next();
            }

            let sentence = current.trim();
            if !sentence.is_empty() {
                sentences.push(sentence.to_string());
            }
            current.clear();
        }
    }

    let tail = current.trim();
    if !tail.is_empty() {
        sentences.push(tail.to_string());
    }

    sentences
}

fn split_word_chunks(text: &str, chunk_size: usize) -> Vec<String> {
    let words = text
        .split_whitespace()
        .map(|word| word.trim())
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>();

    if words.is_empty() {
        return Vec::new();
    }

    words
        .chunks(chunk_size.max(1))
        .map(|chunk| chunk.join(" "))
        .collect()
}

fn join_transcript_units(units: &[String]) -> String {
    units
        .iter()
        .map(|unit| unit.trim())
        .filter(|unit| !unit.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn upsert_entry(entries: &mut Vec<TimelineEntry>, next: TimelineEntry) {
    if let Some(index) = entries.iter().position(|entry| entry.id() == next.id()) {
        entries[index] = next;
    } else {
        entries.push(next);
    }
}

fn existing_capture<'a>(session: &'a Session, id: &str) -> Option<&'a CaptureEntry> {
    session.entries.iter().find_map(|entry| match entry {
        TimelineEntry::Capture(item) if item.id == id => Some(item),
        _ => None,
    })
}

fn existing_dictation<'a>(session: &'a Session, id: &str) -> Option<&'a DictationEntry> {
    session.entries.iter().find_map(|entry| match entry {
        TimelineEntry::Dictation(item) if item.id == id => Some(item),
        _ => None,
    })
}

fn slugify(input: &str) -> String {
    let mut slug = String::new();
    let mut last_was_dash = false;

    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            last_was_dash = false;
        } else if !last_was_dash {
            slug.push('-');
            last_was_dash = true;
        }
    }

    let trimmed = slug.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "session".to_string()
    } else {
        trimmed
    }
}

fn file_to_data_url(path: &Path, mime: &str) -> Result<String> {
    let bytes =
        fs::read(path).with_context(|| format!("failed to read asset {}", path.display()))?;
    Ok(format!("data:{mime};base64,{}", BASE64.encode(bytes)))
}

fn audio_mime_type(path: &Path) -> &'static str {
    match path.extension().and_then(|value| value.to_str()) {
        Some("m4a") => "audio/m4a",
        Some("mp3") => "audio/mpeg",
        Some("webm") => "audio/webm",
        _ => "audio/wav",
    }
}

fn write_data_url(path: &Path, data_url: &str) -> Result<()> {
    let encoded = data_url
        .split_once(',')
        .map(|(_, encoded)| encoded)
        .context("invalid data URL payload")?;
    write_base64_file(path, encoded)
}

fn write_base64_file(path: &Path, encoded: &str) -> Result<()> {
    let bytes = BASE64
        .decode(encoded)
        .context("failed to decode base64 file payload")?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, bytes).with_context(|| format!("failed to write {}", path.display()))
}

fn move_file_into_session(source: &Path, destination: &Path) -> Result<()> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }

    if source == destination {
        return Ok(());
    }

    match fs::rename(source, destination) {
        Ok(()) => Ok(()),
        Err(_) => {
            fs::copy(source, destination).with_context(|| {
                format!(
                    "failed to copy capture from {} to {}",
                    source.display(),
                    destination.display()
                )
            })?;
            fs::remove_file(source)
                .with_context(|| format!("failed to remove temporary capture {}", source.display()))
        }
    }
}

fn persist_capture_original_path(
    session_dir: &Path,
    session: &Session,
    payload: &CaptureSavePayload,
) -> Result<String> {
    let input_path = PathBuf::from(&payload.original_image_path);

    if !input_path.is_absolute() {
        return Ok(payload.original_image_path.clone());
    }

    if let Ok(relative) = input_path.strip_prefix(session_dir) {
        return Ok(relative.to_string_lossy().to_string());
    }

    let relative = existing_capture(session, &payload.id)
        .map(|entry| entry.original_image_path.clone())
        .unwrap_or_else(|| asset_relative_path(&payload.id, "original", "png"));
    let destination = session_dir.join(&relative);

    if input_path.exists() {
        move_file_into_session(&input_path, &destination)?;
        return Ok(relative);
    }

    Err(anyhow::anyhow!(
        "capture source file does not exist: {}",
        input_path.display()
    ))
}

fn absolutize_to_relative(session_dir: &Path, input: &str) -> Result<String> {
    let input_path = PathBuf::from(input);
    if input_path.is_absolute() {
        let relative = input_path.strip_prefix(session_dir).with_context(|| {
            format!("path {} is outside session directory", input_path.display())
        })?;
        Ok(relative.to_string_lossy().to_string())
    } else {
        Ok(input.to_string())
    }
}

fn asset_relative_path(id: &str, suffix: &str, ext: &str) -> String {
    format!("{SESSION_ASSETS_DIR}/{id}-{suffix}.{ext}")
}

struct SessionPaths {
    session_dir: PathBuf,
    assets_dir: PathBuf,
    meta_path: PathBuf,
    markdown_path: PathBuf,
}

fn session_paths(root: &Path, session_id: &str) -> SessionPaths {
    let session_dir = root.join(session_id);
    SessionPaths {
        assets_dir: session_dir.join(SESSION_ASSETS_DIR),
        meta_path: session_dir.join(SESSION_META_FILE),
        markdown_path: session_dir.join(SESSION_MARKDOWN_FILE),
        session_dir,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AnnotationShape, Point, SessionMode, ShapeKind};
    use tempfile::tempdir;

    #[test]
    fn creates_slugged_session_ids() {
        let root = tempdir().unwrap();
        let session = create_session(
            root.path(),
            Some("Design Review #12".into()),
            SessionMode::CaptureNotes,
        )
        .unwrap();
        assert!(session.id.contains("design-review-12"));
    }

    #[test]
    fn persists_ordered_timeline_markdown() {
        let root = tempdir().unwrap();
        let session =
            create_session(root.path(), Some("Session".into()), SessionMode::Dictation).unwrap();

        let note = TextNotePayload {
            id: Some("note-1".into()),
            created_at: Some("2026-04-08T12:00:00Z".into()),
            text: "Later note".into(),
        };
        save_text_note(root.path(), &session.id, note).unwrap();

        let payload = CaptureSavePayload {
            id: "cap-1".into(),
            created_at: "2026-04-08T11:00:00Z".into(),
            original_image_path: "assets/cap-1-original.png".into(),
            shapes: vec![AnnotationShape {
                id: "shape-1".into(),
                kind: ShapeKind::Arrow,
                start: Point { x: 10.0, y: 10.0 },
                end: Point { x: 40.0, y: 40.0 },
            }],
            bubble_note: Some("Point here".into()),
            bubble_anchor: Some(Point { x: 20.0, y: 20.0 }),
            annotated_image_data_url: None,
        };
        save_capture_entry(root.path(), &session.id, payload).unwrap();

        let markdown_path = session_paths(root.path(), &session.id).markdown_path;
        let markdown = fs::read_to_string(markdown_path).unwrap();
        let capture_index = markdown.find("![](assets/cap-1-original.png)").unwrap();
        let note_index = markdown.find("Later note").unwrap();
        assert!(!markdown.contains("## Timeline"));
        assert!(!markdown.contains("Offset:"));
        assert!(capture_index < note_index);
    }

    #[test]
    fn interleaves_capture_into_dictation_markdown() {
        let session = Session {
            id: "session-1".into(),
            title: "Session".into(),
            mode: SessionMode::Dictation,
            created_at: "2026-04-09T11:00:00Z".into(),
            updated_at: "2026-04-09T11:10:00Z".into(),
            entries: vec![
                TimelineEntry::Dictation(DictationEntry {
                    id: "dict-1".into(),
                    created_at: "2026-04-09T11:00:00Z".into(),
                    audio_path: "assets/dict-1-clip.wav".into(),
                    transcript: "First thought. Second thought. Third thought.".into(),
                    corrected_transcript: None,
                }),
                TimelineEntry::Capture(CaptureEntry {
                    id: "cap-1".into(),
                    created_at: "2026-04-09T11:05:00Z".into(),
                    original_image_path: "assets/cap-1-original.png".into(),
                    annotated_image_path: None,
                    shapes: Vec::new(),
                    bubble_note: None,
                    bubble_anchor: None,
                }),
            ],
        };

        let markdown = render_markdown(&session);
        let second_index = markdown.find("Second thought.").unwrap();
        let image_index = markdown.find("![](assets/cap-1-original.png)").unwrap();
        let third_index = markdown.find("Third thought.").unwrap();

        assert!(!markdown.contains("# Session"));
        assert!(!markdown.contains("Transcript:"));
        assert!(second_index < image_index);
        assert!(image_index < third_index);
    }

    #[test]
    fn saves_dictation_and_uses_relative_audio_paths() {
        let root = tempdir().unwrap();
        let session =
            create_session(root.path(), Some("Session".into()), SessionMode::Dictation).unwrap();
        let clip = BASE64.encode(b"RIFFdemo");

        let saved = save_dictation_entry(
            root.path(),
            &session.id,
            DictationSavePayload {
                id: Some("dict-1".into()),
                created_at: Some("2026-04-08T11:00:00Z".into()),
                audio_base64: Some(clip),
                transcript: Some("Initial transcript".into()),
                corrected_transcript: Some("Corrected transcript".into()),
                audio_path: None,
            },
            None,
        )
        .unwrap();

        let dictation = existing_dictation(&saved, "dict-1").unwrap();
        assert!(dictation.audio_path.starts_with("assets/"));
        assert_eq!(
            dictation.corrected_transcript.as_deref(),
            Some("Corrected transcript")
        );
    }

    #[test]
    fn imports_external_capture_files_into_session_assets() {
        let root = tempdir().unwrap();
        let session = create_session(
            root.path(),
            Some("Session".into()),
            SessionMode::CaptureNotes,
        )
        .unwrap();
        let external_dir = tempdir().unwrap();
        let external_capture = external_dir.path().join("capture.png");
        fs::write(&external_capture, b"png").unwrap();

        let saved = save_capture_entry(
            root.path(),
            &session.id,
            CaptureSavePayload {
                id: "cap-import".into(),
                created_at: "2026-04-08T11:00:00Z".into(),
                original_image_path: external_capture.display().to_string(),
                shapes: Vec::new(),
                bubble_note: None,
                bubble_anchor: None,
                annotated_image_data_url: None,
            },
        )
        .unwrap();

        let capture = existing_capture(&saved, "cap-import").unwrap();
        let stored_capture = session_paths(root.path(), &session.id)
            .session_dir
            .join(&capture.original_image_path);

        assert!(capture.original_image_path.starts_with("assets/"));
        assert!(stored_capture.exists());
        assert!(!external_capture.exists());
    }
}
