import { listen } from "@tauri-apps/api/event";
import { type CSSProperties, useEffect, useEffectEvent, useRef, useState } from "react";

import {
  appendDebugLog,
  captureInteractive,
  createSession,
  getTranscriptionStatus,
  openSettingsWindow,
  saveCaptureEntry,
  startMainWindowDrag,
  startNativeRecording,
  stopNativeRecording,
  syncMainWindowLayout,
} from "../api";
import type { CaptureDraft, SessionMode, SessionView } from "../types";

const BUTTON_DEBOUNCE_MS = 800;
const NOTICE_TIMEOUT_MS = 6000;

type PillNotice = {
  tone: "error" | "info";
  title: string;
  detail?: string;
  actionLabel?: string;
  action?: () => void;
};

function formatRecordingDuration(startedAt: number, now: number) {
  const elapsedSeconds = Math.max(0, Math.floor((now - startedAt) / 1000));
  const minutes = Math.floor(elapsedSeconds / 60);
  const seconds = elapsedSeconds % 60;
  return `${minutes}:${seconds.toString().padStart(2, "0")}`;
}

function ScreenshotIcon() {
  return (
    <svg viewBox="0 0 16 16" aria-hidden="true">
      <rect
        x="3"
        y="3"
        width="10"
        height="10"
        rx="2"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeDasharray="1.75 1.75"
      />
    </svg>
  );
}

function StopIcon() {
  return (
    <svg viewBox="0 0 16 16" aria-hidden="true">
      <rect x="3.5" y="3.5" width="9" height="9" rx="1.8" fill="currentColor" />
    </svg>
  );
}

export function MainShell() {
  const [activeSession, setActiveSession] = useState<SessionView | null>(null);
  const [isRecording, setIsRecording] = useState(false);
  const [isProcessingRecording, setIsProcessingRecording] = useState(false);
  const [isStartingRecording, setIsStartingRecording] = useState(false);
  const [isStartingCapture, setIsStartingCapture] = useState(false);
  const [isStoppingRecording, setIsStoppingRecording] = useState(false);
  const [recordingStartedAt, setRecordingStartedAt] = useState<number | null>(null);
  const [recordingNow, setRecordingNow] = useState<number>(() => Date.now());
  const [notice, setNotice] = useState<PillNotice | null>(null);
  const isExpanded = isRecording || isProcessingRecording;
  const recordingDuration = isRecording && recordingStartedAt
    ? formatRecordingDuration(recordingStartedAt, recordingNow)
    : null;

  const recordingSessionRef = useRef<SessionView | null>(null);
  const hasUserMovedWindowRef = useRef(false);
  const recordActionLockRef = useRef(false);
  const captureActionLockRef = useRef(false);
  const noticeTimeoutRef = useRef<number | null>(null);

  function logDebug(message: string) {
    void appendDebugLog(message).catch(() => {
      // Ignore debug log failures so the UI path stays responsive.
    });
  }

  function tryLockAction(lockRef: { current: boolean }) {
    if (lockRef.current) {
      return false;
    }

    lockRef.current = true;
    return true;
  }

  function releaseActionLock(lockRef: { current: boolean }, startedAt: number) {
    const remaining = Math.max(0, BUTTON_DEBOUNCE_MS - (Date.now() - startedAt));
    window.setTimeout(() => {
      lockRef.current = false;
    }, remaining);
  }

  function clearNoticeTimeout() {
    if (noticeTimeoutRef.current !== null) {
      window.clearTimeout(noticeTimeoutRef.current);
      noticeTimeoutRef.current = null;
    }
  }

  const showNotice = useEffectEvent((next: PillNotice) => {
    clearNoticeTimeout();
    setNotice(next);
    noticeTimeoutRef.current = window.setTimeout(() => {
      setNotice(null);
      noticeTimeoutRef.current = null;
    }, NOTICE_TIMEOUT_MS);
  });

  useEffect(() => {
    void syncMainWindowLayout(false, true);
  }, []);

  useEffect(() => {
    void syncMainWindowLayout(isExpanded, !hasUserMovedWindowRef.current);
  }, [isExpanded]);

  useEffect(() => {
    if (!recordingStartedAt) {
      return;
    }

    setRecordingNow(Date.now());
    const timer = window.setInterval(() => {
      setRecordingNow(Date.now());
    }, 1000);

    return () => window.clearInterval(timer);
  }, [recordingStartedAt]);

  useEffect(() => () => {
    clearNoticeTimeout();
  }, []);

  const persistCaptureDraft = useEffectEvent(async (draft: CaptureDraft) => {
    const session = recordingSessionRef.current;
    if (!session) {
      throw new Error("Start recording before taking screenshots.");
    }

    if (!activeSession || activeSession.id !== session.id) {
      setActiveSession(session);
    }

    const savedSession = await saveCaptureEntry(session.id, {
      id: draft.id,
      createdAt: draft.createdAt,
      originalImagePath: draft.originalImagePath,
      shapes: [],
      bubbleNote: null,
      bubbleAnchor: null,
      annotatedImageDataUrl: null,
    });

    setActiveSession(savedSession);
    if (recordingSessionRef.current?.id === savedSession.id) {
      recordingSessionRef.current = savedSession;
    }
  });

  const beginCapture = useEffectEvent(async () => {
    if (
      !isRecording ||
      !recordingSessionRef.current ||
      isStartingCapture ||
      isStartingRecording ||
      isProcessingRecording ||
      isStoppingRecording ||
      !tryLockAction(captureActionLockRef)
    ) {
      return;
    }

    setIsStartingCapture(true);
    const actionStartedAt = Date.now();

    try {
      const captureMode: SessionMode = recordingSessionRef.current.mode;
      const draft = await captureInteractive(captureMode);

      if (!draft) {
        return;
      }

      await persistCaptureDraft(draft);
    } catch (error) {
      await syncMainWindowLayout(true, !hasUserMovedWindowRef.current);
      const message =
        error instanceof Error ? error.message : "Couldn't start the macOS crop tool.";
      console.error(message);
      showNotice({
        tone: "error",
        title: "Screenshot failed",
        detail: message,
      });
    } finally {
      setIsStartingCapture(false);
      releaseActionLock(captureActionLockRef, actionStartedAt);
    }
  });

  useEffect(() => {
    let unlistenShortcut: (() => void) | undefined;

    void listen("shortcut://capture", () => {
      if (recordingSessionRef.current) {
        void beginCapture();
      }
    }).then((dispose) => {
      unlistenShortcut = dispose;
    });

    return () => {
      unlistenShortcut?.();
    };
  }, [beginCapture]);

  async function handleStartRecording() {
    logDebug(
      `ui:handleStartRecording isStarting=${isStartingRecording} isCapturing=${isStartingCapture} isStopping=${isStoppingRecording}`,
    );
    if (
      isStartingRecording ||
      isStartingCapture ||
      isProcessingRecording ||
      isStoppingRecording ||
      !tryLockAction(recordActionLockRef)
    ) {
      logDebug("ui:start_recording_ignored_debounce");
      return;
    }

    setIsStartingRecording(true);
    const actionStartedAt = Date.now();

    try {
      const transcriptionStatus = await getTranscriptionStatus();
      if (!transcriptionStatus.configured) {
        logDebug(
          `ui:start_recording_blocked provider=${transcriptionStatus.provider} configured=false`,
        );
        void openSettingsWindow();
        return;
      }

      const session = await createSession(undefined, "dictation");
      logDebug(`ui:session_created id=${session.id}`);
      setActiveSession(session);
      recordingSessionRef.current = session;

      await startNativeRecording(session.id);
      logDebug(`ui:start_recording_ok id=${session.id}`);

      const startedAt = Date.now();
      setIsRecording(true);
      setIsProcessingRecording(false);
      setRecordingStartedAt(startedAt);
      setRecordingNow(startedAt);
    } catch (error) {
      const message =
        error instanceof Error ? error.message : "Couldn't start recording.";
      logDebug(`ui:start_recording_error ${message}`);
      console.error(message);
      showNotice({
        tone: "error",
        title: "Couldn't start recording",
        detail: message,
      });
      recordingSessionRef.current = null;
      setIsRecording(false);
      setIsProcessingRecording(false);
      setRecordingStartedAt(null);
    } finally {
      setIsStartingRecording(false);
      releaseActionLock(recordActionLockRef, actionStartedAt);
    }
  }

  async function handleStopRecording() {
    const session = recordingSessionRef.current;
    logDebug(
      `ui:handleStopRecording session=${session?.id ?? "none"} isStarting=${isStartingRecording} isCapturing=${isStartingCapture} isStopping=${isStoppingRecording}`,
    );
    if (
      !session ||
      isStoppingRecording ||
      isStartingCapture ||
      isStartingRecording ||
      isProcessingRecording ||
      !tryLockAction(recordActionLockRef)
    ) {
      logDebug("ui:stop_recording_ignored_debounce");
      return;
    }

    setIsStoppingRecording(true);
    setIsProcessingRecording(true);
    setIsRecording(false);
    setRecordingStartedAt(null);
    setRecordingNow(Date.now());
    logDebug(`ui:stop_recording_begin id=${session.id}`);
    const actionStartedAt = Date.now();

    try {
      const savedSession = await stopNativeRecording(session.id);
      logDebug(`ui:stop_recording_ok id=${savedSession.id}`);
      setActiveSession(savedSession);
      recordingSessionRef.current = null;
      setIsProcessingRecording(false);
      setRecordingNow(Date.now());
    } catch (error) {
      const message =
        error instanceof Error ? error.message : "Couldn't finish recording.";
      logDebug(`ui:stop_recording_error ${message}`);
      setIsRecording(false);
      setIsProcessingRecording(false);
      setRecordingStartedAt(null);
      setRecordingNow(Date.now());
      recordingSessionRef.current = null;
      console.error(message);
      showNotice({
        tone: "error",
        title: "Couldn't finish recording",
        detail: message,
      });
    } finally {
      setIsStoppingRecording(false);
      releaseActionLock(recordActionLockRef, actionStartedAt);
    }
  }

  return (
    <main className="dock-scene">
      <div className="dock-stack">
        {notice ? (
          <div className={`pill-notice is-${notice.tone}`} role="status" aria-live="polite">
            <div className="pill-notice-copy">
              <strong>{notice.title}</strong>
              {notice.detail ? <span>{notice.detail}</span> : null}
            </div>
            {notice.action && notice.actionLabel ? (
              <button
                type="button"
                className="pill-notice-action"
                onClick={() => {
                  notice.action?.();
                }}
              >
                {notice.actionLabel}
              </button>
            ) : null}
          </div>
        ) : null}

        <section
          className={`control-pod ${isExpanded ? "is-recording" : "is-idle"} ${isProcessingRecording ? "is-processing" : ""}`}
        >
          <div
            className="pod-handle"
            aria-hidden="true"
            onPointerDown={(event) => {
              if (event.button !== 0) {
                return;
              }

              event.preventDefault();
              hasUserMovedWindowRef.current = true;
              void startMainWindowDrag().catch((error) => {
                console.error("Couldn't drag the Feedback pill.", error);
              });
            }}
          >
            <span className="pod-handle-dot" />
            <span className="pod-handle-dot" />
            <span className="pod-handle-dot" />
          </div>

          <div className={`pod-actions ${isExpanded ? "is-recording" : "is-idle"}`}>
            {!isExpanded ? (
              <button
                type="button"
                className="utility-button utility-button-start"
                onPointerDown={(event) => {
                  event.preventDefault();
                  event.stopPropagation();
                  logDebug(`ui:start_button_press isRecording=${String(isRecording)}`);
                  void handleStartRecording();
                }}
              >
                Start
              </button>
            ) : isProcessingRecording ? (
              <div className="processing-status" aria-live="polite" aria-label="Processing recording">
                <div className="processing-trail" aria-hidden="true">
                  <span className="processing-dot" style={{ "--index": 0 } as CSSProperties} />
                  <span className="processing-dot" style={{ "--index": 1 } as CSSProperties} />
                  <span className="processing-dot" style={{ "--index": 2 } as CSSProperties} />
                  <span className="processing-dot" style={{ "--index": 3 } as CSSProperties} />
                  <span className="processing-dot" style={{ "--index": 4 } as CSSProperties} />
                </div>
                <span className="processing-label">Processing</span>
              </div>
            ) : (
              <>
                <span className="recording-timer" aria-live="polite">
                  {recordingDuration}
                </span>
                <button
                  type="button"
                  className="utility-button utility-button-icon utility-button-capture"
                  aria-label="Take screenshot"
                  disabled={isStartingCapture || isStartingRecording || isStoppingRecording}
                  onPointerDown={(event) => {
                    event.preventDefault();
                    event.stopPropagation();
                    logDebug("ui:crop_button_press");
                    void beginCapture();
                  }}
                >
                  <ScreenshotIcon />
                </button>
                <button
                  type="button"
                  className="utility-button utility-button-icon utility-button-stop"
                  aria-label="Stop recording"
                  disabled={isStoppingRecording || isStartingCapture || isStartingRecording}
                  onPointerDown={(event) => {
                    event.preventDefault();
                    event.stopPropagation();
                    logDebug("ui:stop_button_press");
                    void handleStopRecording();
                  }}
                >
                  <StopIcon />
                </button>
              </>
            )}
          </div>
        </section>
      </div>
    </main>
  );
}
