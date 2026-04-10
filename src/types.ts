export type SessionMode = "dictation" | "capture_notes";
export type ShapeKind = "arrow" | "rectangle" | "highlight";

export interface Point {
  x: number;
  y: number;
}

export interface AnnotationShape {
  id: string;
  kind: ShapeKind;
  start: Point;
  end: Point;
}

export interface SessionSummary {
  id: string;
  title: string;
  mode: SessionMode;
  createdAt: string;
  updatedAt: string;
  entryCount: number;
  storagePath: string;
}

export interface SessionView {
  id: string;
  title: string;
  mode: SessionMode;
  createdAt: string;
  updatedAt: string;
  entries: TimelineEntry[];
  storagePath: string;
  markdownPath: string;
  shortcut: string;
}

export interface CaptureEntry {
  kind: "capture";
  id: string;
  createdAt: string;
  originalImagePath: string;
  annotatedImagePath: string | null;
  shapes: AnnotationShape[];
  bubbleNote: string | null;
  bubbleAnchor: Point | null;
  originalImageDataUrl: string;
  annotatedImageDataUrl: string | null;
  displayImageDataUrl: string;
}

export interface DictationEntry {
  kind: "dictation";
  id: string;
  createdAt: string;
  audioPath: string;
  transcript: string;
  correctedTranscript: string | null;
  audioDataUrl: string;
}

export interface TextNoteEntry {
  kind: "text_note";
  id: string;
  createdAt: string;
  text: string;
}

export type TimelineEntry = CaptureEntry | DictationEntry | TextNoteEntry;

export interface CaptureDraft {
  id: string;
  createdAt: string;
  mode: SessionMode;
  originalImagePath: string;
  originalImageDataUrl: string;
}

export interface PermissionStatus {
  screenRecording: PermissionState;
  microphone: PermissionState;
}

export type PermissionState = "unknown" | "granted" | "denied";

export interface TranscriptionStatus {
  configured: boolean;
  provider: string;
  model: string;
}

export type TranscriptionProvider = "openai" | "local_whisper";

export interface AppSettingsView {
  transcriptionProvider: TranscriptionProvider;
  openaiModel: string;
  openaiBaseUrl: string;
  openaiPrompt: string;
  whisperBinaryPath: string;
  whisperModelPath: string;
  whisperLanguage: string;
  hasOpenAiApiKey: boolean;
  configPath: string;
}

export interface AppSettingsSavePayload {
  transcriptionProvider: TranscriptionProvider;
  openaiModel: string;
  openaiBaseUrl: string;
  openaiPrompt: string;
  openaiApiKey?: string;
  clearOpenAiApiKey: boolean;
  whisperBinaryPath: string;
  whisperModelPath: string;
  whisperLanguage: string;
}

export interface CaptureSavePayload {
  id: string;
  createdAt: string;
  originalImagePath: string;
  shapes: AnnotationShape[];
  bubbleNote: string | null;
  bubbleAnchor: Point | null;
  annotatedImageDataUrl: string | null;
}

export interface TextNotePayload {
  id?: string;
  createdAt?: string;
  text: string;
}

export interface DictationSavePayload {
  id?: string;
  createdAt?: string;
  audioBase64?: string;
  transcript?: string;
  correctedTranscript?: string;
  audioPath?: string;
}
