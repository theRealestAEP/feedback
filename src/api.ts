import { invoke } from "@tauri-apps/api/core";

import type {
  AppSettingsSavePayload,
  AppSettingsView,
  CaptureDraft,
  CaptureSavePayload,
  DictationSavePayload,
  PermissionStatus,
  SessionMode,
  SessionSummary,
  SessionView,
  TextNotePayload,
  TranscriptionStatus,
} from "./types";

export function createSession(title: string | undefined, mode: SessionMode) {
  return invoke<SessionView>("create_session", { title, mode });
}

export function loadAppSettings() {
  return invoke<AppSettingsView>("load_app_settings");
}

export function saveAppSettings(payload: AppSettingsSavePayload) {
  return invoke<AppSettingsView>("save_app_settings", { payload });
}

export function installLocalWhisper() {
  return invoke<AppSettingsView>("install_local_whisper");
}

export function openSettingsWindow() {
  return invoke<void>("open_settings_window");
}

export function appendDebugLog(message: string) {
  return invoke<void>("append_debug_log", { message });
}

export function readDebugLog() {
  return invoke<string>("read_debug_log");
}

export function clearDebugLog() {
  return invoke<string>("clear_debug_log");
}

export function loadSessions() {
  return invoke<SessionSummary[]>("load_sessions");
}

export function loadSession(sessionId: string) {
  return invoke<SessionView>("load_session", { sessionId });
}

export function captureInteractive(mode: SessionMode) {
  return invoke<CaptureDraft | null>("capture_interactive", {
    mode,
  });
}

export function saveCaptureEntry(sessionId: string, entryDraft: CaptureSavePayload) {
  return invoke<SessionView>("save_capture_entry", { sessionId, entryDraft });
}

export function saveDictationEntry(sessionId: string, payload: DictationSavePayload) {
  return invoke<SessionView>("save_dictation_entry", { sessionId, payload });
}

export function startNativeRecording(sessionId: string) {
  return invoke<void>("start_native_recording", { sessionId });
}

export function stopNativeRecording(sessionId: string) {
  return invoke<SessionView>("stop_native_recording", { sessionId });
}

export function saveTextNote(sessionId: string, text: TextNotePayload) {
  return invoke<SessionView>("save_text_note", { sessionId, text });
}

export function openSessionFolder(sessionId: string) {
  return invoke<void>("open_session_folder", { sessionId });
}

export function startMainWindowDrag() {
  return invoke<void>("start_main_window_drag");
}

export function syncMainWindowLayout(recording: boolean, docked: boolean) {
  return invoke<void>("sync_main_window_layout", { recording, docked });
}

export function getPermissionStatus() {
  return invoke<PermissionStatus>("get_permission_status");
}

export function requestPermissions() {
  return invoke<PermissionStatus>("request_permissions");
}

export function getTranscriptionStatus() {
  return invoke<TranscriptionStatus>("get_transcription_status");
}
