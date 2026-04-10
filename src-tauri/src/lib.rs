mod debug_log;
mod model;
mod openai;
mod settings;
mod sidecar;
mod storage;
mod transcription;
mod whisper;

use std::{
    path::PathBuf,
    process::{Child, Command},
    sync::Mutex,
    thread,
    time::Duration,
};

use anyhow::{Context, Result};
use base64::Engine;
use model::{
    AppSettingsSavePayload, CaptureSavePayload, DictationSavePayload, PermissionState,
    PermissionStatus, SessionMode, TextNotePayload, TranscriptionStatus,
};
use storage::{session_root, session_to_view, CAPTURE_SHORTCUT};
use tauri::{
    menu::{AboutMetadata, Menu, MenuItem, PredefinedMenuItem, Submenu},
    window::Color,
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, Runtime, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};
use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut, ShortcutState};

struct AppState {
    permissions: Mutex<PermissionStatus>,
    recording: Mutex<Option<RecordingState>>,
}

struct RecordingState {
    session_id: String,
    entry_id: String,
    created_at: String,
    output_path: PathBuf,
    child: Child,
}

const SETTINGS_MENU_ID: &str = "settings";
const SETTINGS_WINDOW_LABEL: &str = "settings";

#[tauri::command]
fn load_app_settings(app: AppHandle) -> Result<model::AppSettingsView, String> {
    settings::load_settings_view(&app).map_err(stringify_error)
}

#[tauri::command]
fn save_app_settings(
    app: AppHandle,
    payload: AppSettingsSavePayload,
) -> Result<model::AppSettingsView, String> {
    settings::save_settings(&app, payload).map_err(stringify_error)
}

#[tauri::command]
fn open_settings_window(app: AppHandle) -> Result<(), String> {
    show_settings_window(&app).map_err(stringify_error)
}

#[tauri::command]
fn append_debug_log(app: AppHandle, message: String) -> Result<(), String> {
    debug_log::append(&app, &message).map_err(stringify_error)
}

#[tauri::command]
fn read_debug_log(app: AppHandle) -> Result<String, String> {
    debug_log::read(&app).map_err(stringify_error)
}

#[tauri::command]
fn clear_debug_log(app: AppHandle) -> Result<String, String> {
    let path = debug_log::clear(&app).map_err(stringify_error)?;
    Ok(path.display().to_string())
}

#[tauri::command]
fn create_session(
    app: AppHandle,
    title: Option<String>,
    mode: SessionMode,
) -> Result<model::SessionView, String> {
    let root = sessions_root(&app).map_err(stringify_error)?;
    let session = storage::create_session(&root, title, mode).map_err(stringify_error)?;
    session_to_view(&root, session).map_err(stringify_error)
}

#[tauri::command]
fn load_sessions(app: AppHandle) -> Result<Vec<model::SessionSummary>, String> {
    let root = sessions_root(&app).map_err(stringify_error)?;
    storage::load_summaries(&root).map_err(stringify_error)
}

#[tauri::command]
fn load_session(app: AppHandle, session_id: String) -> Result<model::SessionView, String> {
    let root = sessions_root(&app).map_err(stringify_error)?;
    let session = storage::load_session(&root, &session_id).map_err(stringify_error)?;
    session_to_view(&root, session).map_err(stringify_error)
}

#[tauri::command]
fn capture_interactive(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    mode: SessionMode,
) -> Result<Option<model::CaptureDraft>, String> {
    let _ = debug_log::append(&app, &format!("capture_interactive:start mode={mode:?}"));
    let draft =
        storage::capture_draft_for_temp_file(mode, String::new()).map_err(stringify_error)?;
    let output = PathBuf::from(&draft.original_image_path);
    let main_window = app.get_webview_window("main");

    if let Some(window) = &main_window {
        let _ = window.hide();
        thread::sleep(Duration::from_millis(120));
    }

    let captured = sidecar::capture_interactive(&app, &output);

    if let Some(window) = &main_window {
        let _ = window.show();
    }

    let captured = captured.map_err(|error| {
        let _ = debug_log::append(&app, &format!("capture_interactive:error {}", error));
        let message = error.to_string();
        if message.to_lowercase().contains("permission") {
            update_screen_permission(&state, PermissionState::Denied);
        }
        stringify_error(error)
    })?;

    if !captured {
        let _ = debug_log::append(&app, "capture_interactive:cancelled");
        return Ok(None);
    }

    update_screen_permission(&state, PermissionState::Granted);
    let _ = debug_log::append(&app, "capture_interactive:success");

    let original_image_data_url =
        std::fs::read(&output)
            .map_err(stringify_error)
            .and_then(|bytes| {
                Ok(format!(
                    "data:image/png;base64,{}",
                    base64::engine::general_purpose::STANDARD.encode(bytes)
                ))
            })?;

    Ok(Some(model::CaptureDraft {
        original_image_data_url,
        ..draft
    }))
}

#[tauri::command]
fn save_capture_entry(
    app: AppHandle,
    session_id: String,
    entry_draft: CaptureSavePayload,
) -> Result<model::SessionView, String> {
    let root = sessions_root(&app).map_err(stringify_error)?;
    let session =
        storage::save_capture_entry(&root, &session_id, entry_draft).map_err(stringify_error)?;
    session_to_view(&root, session).map_err(stringify_error)
}

#[tauri::command]
fn save_dictation_entry(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    session_id: String,
    payload: DictationSavePayload,
) -> Result<model::SessionView, String> {
    let root = sessions_root(&app).map_err(stringify_error)?;
    let payload_id = payload.id.clone();
    let has_fresh_audio = payload.audio_base64.is_some();
    let transcript = payload.transcript.clone();

    if has_fresh_audio {
        update_microphone_permission(&state, PermissionState::Granted);
    }

    let mut saved = storage::save_dictation_entry(&root, &session_id, payload, transcript)
        .map_err(stringify_error)?;

    let target_entry_id = payload_id.or_else(|| {
        saved.entries.iter().rev().find_map(|entry| match entry {
            model::TimelineEntry::Dictation(item) => Some(item.id.clone()),
            _ => None,
        })
    });

    if let Some(target_entry_id) = target_entry_id {
        if let Some(entry) = saved.entries.iter_mut().find_map(|entry| match entry {
            model::TimelineEntry::Dictation(item) if item.id == target_entry_id => Some(item),
            _ => None,
        }) {
            let audio_abs = storage::asset_absolute_path(&root, &session_id, &entry.audio_path);
            match transcription::transcribe_audio(&app, &audio_abs) {
                Ok(transcript) => {
                    entry.transcript = transcript.clone();
                    entry.corrected_transcript = Some(transcript.clone());
                }
                Err(error) => {
                    let message = format!("Transcription failed: {}", error);
                    entry.transcript = message.clone();
                    entry.corrected_transcript = Some(message);
                }
            }
            saved.updated_at = chrono::Utc::now().to_rfc3339();
            storage::persist_session(&root, &saved).map_err(stringify_error)?;
        }
    }

    session_to_view(&root, saved).map_err(stringify_error)
}

#[tauri::command]
fn start_native_recording(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    session_id: String,
) -> Result<(), String> {
    let root = sessions_root(&app).map_err(stringify_error)?;
    let mut recording = state.recording.lock().unwrap();
    if recording.is_some() {
        let _ = debug_log::append(&app, "start_native_recording:already_recording");
        return Err("A recording is already in progress.".to_string());
    }

    let entry_id = uuid::Uuid::new_v4().to_string();
    let output_path = session_root(&root, &session_id)
        .join(storage::SESSION_ASSETS_DIR)
        .join(format!("{entry_id}-clip.wav"));

    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent).map_err(stringify_error)?;
    }

    let _ = debug_log::append(
        &app,
        &format!(
            "start_native_recording:request session_id={} output={}",
            session_id,
            output_path.display()
        ),
    );

    let child = sidecar::start_recording(&app, &output_path).map_err(|error| {
        let _ = debug_log::append(&app, &format!("start_native_recording:error {}", error));
        let message = error.to_string();
        if message.to_lowercase().contains("permission") {
            update_microphone_permission(&state, PermissionState::Denied);
        }
        stringify_error(error)
    })?;
    let created_at = chrono::Utc::now().to_rfc3339();

    *recording = Some(RecordingState {
        session_id,
        entry_id,
        created_at,
        output_path,
        child,
    });

    let _ = debug_log::append(&app, "start_native_recording:started");

    Ok(())
}

#[tauri::command]
fn stop_native_recording(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    session_id: String,
) -> Result<model::SessionView, String> {
    let root = sessions_root(&app).map_err(stringify_error)?;
    let _ = debug_log::append(
        &app,
        &format!("stop_native_recording:request session_id={session_id}"),
    );
    let recording = {
        let mut guard = state.recording.lock().unwrap();
        match guard.take() {
            Some(recording) if recording.session_id == session_id => recording,
            Some(recording) => {
                let _ = debug_log::append(
                    &app,
                    &format!(
                        "stop_native_recording:wrong_session active={} requested={}",
                        recording.session_id, session_id
                    ),
                );
                *guard = Some(recording);
                return Err("A different session is currently recording.".to_string());
            }
            None => {
                let _ = debug_log::append(&app, "stop_native_recording:none_active");
                return Err("No recording is currently in progress.".to_string());
            }
        }
    };

    sidecar::stop_recording(recording.child).map_err(|error| {
        let _ = debug_log::append(&app, &format!("stop_native_recording:error {}", error));
        let message = error.to_string();
        if message.to_lowercase().contains("permission") {
            update_microphone_permission(&state, PermissionState::Denied);
        }
        stringify_error(error)
    })?;
    let _ = debug_log::append(
        &app,
        &format!(
            "stop_native_recording:sidecar_stopped entry_id={} output={}",
            recording.entry_id,
            recording.output_path.display()
        ),
    );
    update_microphone_permission(&state, PermissionState::Granted);

    let mut saved = storage::save_dictation_entry(
        &root,
        &session_id,
        DictationSavePayload {
            id: Some(recording.entry_id.clone()),
            created_at: Some(recording.created_at.clone()),
            audio_base64: None,
            transcript: None,
            corrected_transcript: None,
            audio_path: Some(recording.output_path.display().to_string()),
        },
        None,
    )
    .map_err(stringify_error)?;

    if let Some(entry) = saved.entries.iter_mut().find_map(|entry| match entry {
        model::TimelineEntry::Dictation(item) if item.id == recording.entry_id => Some(item),
        _ => None,
    }) {
        match transcription::transcribe_audio(&app, &recording.output_path) {
            Ok(transcript) => {
                let _ = debug_log::append(
                    &app,
                    &format!(
                        "stop_native_recording:transcription_ok chars={}",
                        transcript.chars().count()
                    ),
                );
                entry.transcript = transcript.clone();
                entry.corrected_transcript = Some(transcript.clone());
            }
            Err(error) => {
                let _ = debug_log::append(
                    &app,
                    &format!("stop_native_recording:transcription_error {}", error),
                );
                let message = format!("Transcription failed: {}", error);
                entry.transcript = message.clone();
                entry.corrected_transcript = Some(message);
            }
        }
        saved.updated_at = chrono::Utc::now().to_rfc3339();
        storage::persist_session(&root, &saved).map_err(stringify_error)?;
    }

    let _ = debug_log::append(
        &app,
        &format!("stop_native_recording:done session_id={session_id}"),
    );

    let session_dir = session_root(&root, &session_id);
    let _ = Command::new("open").arg(&session_dir).status();

    session_to_view(&root, saved).map_err(stringify_error)
}

#[tauri::command]
fn save_text_note(
    app: AppHandle,
    session_id: String,
    text: TextNotePayload,
) -> Result<model::SessionView, String> {
    let root = sessions_root(&app).map_err(stringify_error)?;
    let session = storage::save_text_note(&root, &session_id, text).map_err(stringify_error)?;
    session_to_view(&root, session).map_err(stringify_error)
}

#[tauri::command]
fn open_session_folder(app: AppHandle, session_id: String) -> Result<(), String> {
    let root = sessions_root(&app).map_err(stringify_error)?;
    let session_dir = session_root(&root, &session_id);
    Command::new("open")
        .arg(session_dir)
        .status()
        .context("failed to open session folder")
        .map_err(stringify_error)?;
    Ok(())
}

#[tauri::command]
fn start_main_window_drag(window: WebviewWindow) -> Result<(), String> {
    window
        .start_dragging()
        .context("failed to drag main window")
        .map_err(stringify_error)
}

#[tauri::command]
fn get_permission_status(state: tauri::State<'_, AppState>) -> PermissionStatus {
    state.permissions.lock().unwrap().clone()
}

#[tauri::command]
fn request_permissions(state: tauri::State<'_, AppState>) -> Result<PermissionStatus, String> {
    let _ = Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture")
        .status();
    let _ = Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone")
        .status();

    Ok(state.permissions.lock().unwrap().clone())
}

#[tauri::command]
fn get_transcription_status(app: AppHandle) -> Result<TranscriptionStatus, String> {
    transcription::transcription_status(&app).map_err(stringify_error)
}

fn stringify_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn sessions_root(app: &AppHandle) -> Result<PathBuf> {
    let home = std::env::var_os("HOME").context("failed to resolve HOME directory")?;
    let root = PathBuf::from(home)
        .join("Documents")
        .join("Feedback")
        .join("sessions");
    migrate_legacy_sessions(app, &root)?;
    storage::ensure_sessions_root(&root)?;
    Ok(root)
}

fn migrate_legacy_sessions(app: &AppHandle, root: &PathBuf) -> Result<()> {
    if root.exists() {
        return Ok(());
    }

    let home = std::env::var_os("HOME").context("failed to resolve HOME directory")?;
    let documents_root = PathBuf::from(home)
        .join("Documents")
        .join("ImageDiction")
        .join("sessions");

    if documents_root.exists() {
        if let Some(parent) = root.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("failed to create session directory {}", parent.display())
            })?;
        }

        copy_dir_all(&documents_root, root)?;
        return Ok(());
    }

    let legacy_root = app
        .path()
        .app_data_dir()
        .context("failed to resolve legacy app data directory")?
        .join("sessions");

    if !legacy_root.exists() {
        return Ok(());
    }

    if let Some(parent) = root.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create session directory {}", parent.display()))?;
    }

    copy_dir_all(&legacy_root, root)?;
    Ok(())
}

fn copy_dir_all(source: &PathBuf, destination: &PathBuf) -> Result<()> {
    std::fs::create_dir_all(destination)
        .with_context(|| format!("failed to create {}", destination.display()))?;

    for entry in
        std::fs::read_dir(source).with_context(|| format!("failed to read {}", source.display()))?
    {
        let entry = entry?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());

        if entry.file_type()?.is_dir() {
            copy_dir_all(&source_path, &destination_path)?;
        } else {
            std::fs::copy(&source_path, &destination_path).with_context(|| {
                format!(
                    "failed to copy {} to {}",
                    source_path.display(),
                    destination_path.display()
                )
            })?;
        }
    }

    Ok(())
}

fn style_main_window<R: Runtime>(window: &WebviewWindow<R>) -> Result<()> {
    let scale_factor = window
        .scale_factor()
        .context("failed to read main window scale factor")?;

    window
        .set_size(LogicalSize::new(140_f64, 44_f64))
        .context("failed to size main window")?;
    window
        .set_background_color(Some(Color(0, 0, 0, 0)))
        .context("failed to clear main window background")?;
    window
        .set_always_on_top(true)
        .context("failed to pin main window")?;
    window
        .set_decorations(false)
        .context("failed to hide main window decorations")?;
    window
        .set_shadow(false)
        .context("failed to disable main window shadow")?;

    if let Some(monitor) = window
        .primary_monitor()
        .context("failed to read primary monitor")?
    {
        let work_area = monitor.work_area();
        let work_area_width = work_area.size.width as f64 / scale_factor;
        let work_area_height = work_area.size.height as f64 / scale_factor;
        let work_area_x = work_area.position.x as f64 / scale_factor;
        let work_area_y = work_area.position.y as f64 / scale_factor;
        let window_size = window
            .outer_size()
            .context("failed to read main window size")?
            .to_logical::<f64>(scale_factor);
        let x = work_area_x + ((work_area_width - window_size.width) / 2.0);
        let y = work_area_y + work_area_height - window_size.height - 24.0;
        window
            .set_position(LogicalPosition::new(x, y))
            .context("failed to position main window")?;
    }

    Ok(())
}

fn build_app_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let app_name = app.package_info().name.clone();
    let settings_item = MenuItem::with_id(
        app,
        SETTINGS_MENU_ID,
        "Settings…",
        true,
        Some("CmdOrCtrl+,"),
    )?;

    let app_menu = Submenu::with_items(
        app,
        &app_name,
        true,
        &[
            &PredefinedMenuItem::about(
                app,
                None,
                Some(AboutMetadata {
                    name: Some(app_name.clone()),
                    version: Some(app.package_info().version.to_string()),
                    ..Default::default()
                }),
            )?,
            &PredefinedMenuItem::separator(app)?,
            &settings_item,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::services(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::hide(app, None)?,
            &PredefinedMenuItem::hide_others(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::quit(app, None)?,
        ],
    )?;

    let edit_menu = Submenu::with_items(
        app,
        "Edit",
        true,
        &[
            &PredefinedMenuItem::undo(app, None)?,
            &PredefinedMenuItem::redo(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::cut(app, None)?,
            &PredefinedMenuItem::copy(app, None)?,
            &PredefinedMenuItem::paste(app, None)?,
            &PredefinedMenuItem::select_all(app, None)?,
        ],
    )?;

    Menu::with_items(app, &[&app_menu, &edit_menu])
}

fn show_settings_window<R: Runtime>(app: &AppHandle<R>) -> Result<()> {
    if let Some(window) = app.get_webview_window(SETTINGS_WINDOW_LABEL) {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
        return Ok(());
    }

    WebviewWindowBuilder::new(
        app,
        SETTINGS_WINDOW_LABEL,
        WebviewUrl::App("index.html".into()),
    )
    .title("Feedback Settings")
    .inner_size(560.0, 520.0)
    .resizable(true)
    .decorations(true)
    .transparent(false)
    .always_on_top(false)
    .visible(true)
    .center()
    .build()
    .context("failed to open settings window")?;

    Ok(())
}

fn update_screen_permission(state: &tauri::State<'_, AppState>, next: PermissionState) {
    if let Ok(mut status) = state.permissions.lock() {
        status.screen_recording = next;
    }
}

fn update_microphone_permission(state: &tauri::State<'_, AppState>, next: PermissionState) {
    if let Ok(mut status) = state.permissions.lock() {
        status.microphone = next;
    }
}

fn capture_shortcut() -> Shortcut {
    Shortcut::new(Some(Modifiers::SUPER | Modifiers::SHIFT), Code::Digit4)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    openai::load_env();

    tauri::Builder::default()
        .menu(build_app_menu)
        .on_menu_event(|app, event| {
            if event.id() == SETTINGS_MENU_ID {
                let _ = show_settings_window(app);
            }
        })
        .manage(AppState {
            permissions: Mutex::new(PermissionStatus {
                screen_recording: PermissionState::Unknown,
                microphone: PermissionState::Unknown,
            }),
            recording: Mutex::new(None),
        })
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_shortcut(capture_shortcut())
                .expect("failed to configure global capture shortcut")
                .with_handler(|app, _, event| {
                    if event.state() == ShortcutState::Pressed {
                        let _ = app.emit(
                            "shortcut://capture",
                            serde_json::json!({ "shortcut": CAPTURE_SHORTCUT }),
                        );
                    }
                })
                .build(),
        )
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            if let Some(main) = app.get_webview_window("main") {
                style_main_window(&main)?;
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            load_app_settings,
            save_app_settings,
            open_settings_window,
            append_debug_log,
            read_debug_log,
            clear_debug_log,
            create_session,
            load_sessions,
            load_session,
            capture_interactive,
            save_capture_entry,
            save_dictation_entry,
            start_native_recording,
            stop_native_recording,
            save_text_note,
            open_session_folder,
            start_main_window_drag,
            get_permission_status,
            request_permissions,
            get_transcription_status
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
