import { listen } from "@tauri-apps/api/event";
import {
  getCurrentWindow,
  LogicalPosition,
  LogicalSize,
  primaryMonitor,
} from "@tauri-apps/api/window";
import { useEffect, useEffectEvent, useLayoutEffect, useRef, useState } from "react";

import {
  appendDebugLog,
  captureInteractive,
  createSession,
  saveCaptureEntry,
  startMainWindowDrag,
  startNativeRecording,
  stopNativeRecording,
} from "../api";
import type { CaptureDraft, SessionMode, SessionView } from "../types";

const currentWindow = getCurrentWindow();

const INITIAL_POD_WIDTH = 140;
const INITIAL_POD_HEIGHT = 44;
const BOTTOM_MARGIN = 16;
const BUTTON_DEBOUNCE_MS = 800;

async function setWindowToPodSize(width: number, height: number) {
  await currentWindow.setSize(new LogicalSize(width, height));
}

async function dockMainWindow(width: number, height: number) {
  const monitor = await primaryMonitor();
  if (!monitor) {
    return;
  }

  const workAreaPosition = monitor.workArea.position.toLogical(monitor.scaleFactor);
  const workAreaSize = monitor.workArea.size.toLogical(monitor.scaleFactor);
  const x = Math.round(
    workAreaPosition.x + (workAreaSize.width - width) / 2,
  );
  const y = Math.round(
    workAreaPosition.y + workAreaSize.height - height - BOTTOM_MARGIN,
  );

  await setWindowToPodSize(width, height);
  await currentWindow.setPosition(new LogicalPosition(x, y));
}

function formatRecordingDuration(startedAt: number, now: number) {
  const elapsedSeconds = Math.max(0, Math.floor((now - startedAt) / 1000));
  const minutes = Math.floor(elapsedSeconds / 60);
  const seconds = elapsedSeconds % 60;
  return `${minutes}:${seconds.toString().padStart(2, "0")}`;
}

export function MainShell() {
  const [activeSession, setActiveSession] = useState<SessionView | null>(null);
  const [isRecording, setIsRecording] = useState(false);
  const [isStartingRecording, setIsStartingRecording] = useState(false);
  const [isStartingCapture, setIsStartingCapture] = useState(false);
  const [isStoppingRecording, setIsStoppingRecording] = useState(false);
  const [recordingStartedAt, setRecordingStartedAt] = useState<number | null>(null);
  const [recordingNow, setRecordingNow] = useState<number>(() => Date.now());
  const recordLabel = isRecording && recordingStartedAt
    ? `Stop ${formatRecordingDuration(recordingStartedAt, recordingNow)}`
    : "Record";

  const podRef = useRef<HTMLElement | null>(null);
  const recordingSessionRef = useRef<SessionView | null>(null);
  const hasUserMovedWindowRef = useRef(false);
  const recordActionLockRef = useRef(false);
  const captureActionLockRef = useRef(false);

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

  useEffect(() => {
    void dockMainWindow(INITIAL_POD_WIDTH, INITIAL_POD_HEIGHT);
  }, []);

  useLayoutEffect(() => {
    const pod = podRef.current;
    if (!pod) {
      return;
    }

    let frame = 0;

    const syncSize = () => {
      const width = Math.ceil(pod.getBoundingClientRect().width);
      const height = Math.ceil(pod.getBoundingClientRect().height);
      if (width <= 0 || height <= 0) {
        return;
      }

      if (hasUserMovedWindowRef.current) {
        void setWindowToPodSize(width, height);
      } else {
        void dockMainWindow(width, height);
      }
    };

    const scheduleSync = () => {
      window.cancelAnimationFrame(frame);
      frame = window.requestAnimationFrame(syncSize);
    };

    scheduleSync();

    const observer = new ResizeObserver(() => {
      scheduleSync();
    });
    observer.observe(pod);

    return () => {
      window.cancelAnimationFrame(frame);
      observer.disconnect();
    };
  }, [isRecording]);

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
      const pod = podRef.current;
      if (pod) {
        const width = Math.ceil(pod.getBoundingClientRect().width);
        const height = Math.ceil(pod.getBoundingClientRect().height);
        await setWindowToPodSize(width, height);
      }
      const message =
        error instanceof Error ? error.message : "Couldn't start the macOS crop tool.";
      console.error(message);
      window.alert(message);
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
      isStoppingRecording ||
      !tryLockAction(recordActionLockRef)
    ) {
      logDebug("ui:start_recording_ignored_debounce");
      return;
    }

    setIsStartingRecording(true);
    const actionStartedAt = Date.now();

    try {
      const session = await createSession(undefined, "dictation");
      logDebug(`ui:session_created id=${session.id}`);
      setActiveSession(session);
      recordingSessionRef.current = session;

      await startNativeRecording(session.id);
      logDebug(`ui:start_recording_ok id=${session.id}`);

      const startedAt = Date.now();
      setIsRecording(true);
      setRecordingStartedAt(startedAt);
      setRecordingNow(startedAt);
    } catch (error) {
      const message =
        error instanceof Error ? error.message : "Couldn't start recording.";
      logDebug(`ui:start_recording_error ${message}`);
      console.error(message);
      window.alert(message);
      recordingSessionRef.current = null;
      setIsRecording(false);
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
      !tryLockAction(recordActionLockRef)
    ) {
      logDebug("ui:stop_recording_ignored_debounce");
      return;
    }

    setIsStoppingRecording(true);
    logDebug(`ui:stop_recording_begin id=${session.id}`);
    const actionStartedAt = Date.now();

    try {
      const savedSession = await stopNativeRecording(session.id);
      logDebug(`ui:stop_recording_ok id=${savedSession.id}`);
      setActiveSession(savedSession);
      recordingSessionRef.current = null;
      setIsRecording(false);
      setRecordingStartedAt(null);
      setRecordingNow(Date.now());
    } catch (error) {
      const message =
        error instanceof Error ? error.message : "Couldn't finish recording.";
      logDebug(`ui:stop_recording_error ${message}`);
      setIsRecording(false);
      setRecordingStartedAt(null);
      setRecordingNow(Date.now());
      recordingSessionRef.current = null;
      console.error(message);
      window.alert(message);
    } finally {
      setIsStoppingRecording(false);
      releaseActionLock(recordActionLockRef, actionStartedAt);
    }
  }

  return (
    <main className="dock-scene">
      <section
        ref={podRef}
        className={`control-pod ${isRecording ? "is-recording" : "is-idle"}`}
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

        <div className={`pod-actions ${isRecording ? "is-recording" : "is-idle"}`}>
          <button
            type="button"
            className={`utility-button utility-button-record ${isRecording ? "is-recording" : ""}`}
            onPointerDown={(event) => {
              event.preventDefault();
              event.stopPropagation();
              logDebug(`ui:record_button_press isRecording=${String(isRecording)}`);
              if (isRecording) {
                void handleStopRecording();
              } else {
                void handleStartRecording();
              }
            }}
          >
            {recordLabel}
          </button>
          {isRecording ? (
            <button
              type="button"
              className="utility-button utility-button-secondary"
              disabled={isStartingCapture || isStartingRecording || isStoppingRecording}
              onPointerDown={(event) => {
                event.preventDefault();
                event.stopPropagation();
                logDebug("ui:crop_button_press");
                void beginCapture();
              }}
            >
              Crop
            </button>
          ) : null}
        </div>
      </section>
    </main>
  );
}
