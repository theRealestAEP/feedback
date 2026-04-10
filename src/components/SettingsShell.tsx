import { type FormEvent, useEffect, useState } from "react";

import { loadAppSettings, saveAppSettings } from "../api";
import type {
  AppSettingsSavePayload,
  AppSettingsView,
  TranscriptionProvider,
} from "../types";

const DEFAULT_SETTINGS: AppSettingsView = {
  transcriptionProvider: "openai",
  openaiModel: "gpt-4o-transcribe",
  openaiBaseUrl: "https://api.openai.com/v1",
  openaiPrompt: "",
  whisperBinaryPath: "",
  whisperModelPath: "",
  whisperLanguage: "",
  hasOpenAiApiKey: false,
  configPath: "",
};

function getErrorMessage(error: unknown, fallback: string) {
  if (error instanceof Error && error.message.trim()) {
    return error.message;
  }

  if (typeof error === "string" && error.trim()) {
    return error;
  }

  if (
    error &&
    typeof error === "object" &&
    "message" in error &&
    typeof error.message === "string" &&
    error.message.trim()
  ) {
    return error.message;
  }

  return fallback;
}

export function SettingsShell() {
  const [settings, setSettings] = useState<AppSettingsView>(DEFAULT_SETTINGS);
  const [apiKeyInput, setApiKeyInput] = useState("");
  const [clearOpenAiApiKey, setClearOpenAiApiKey] = useState(false);
  const [isLoading, setIsLoading] = useState(true);
  const [isSaving, setIsSaving] = useState(false);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [statusMessage, setStatusMessage] = useState<string | null>(null);

  useEffect(() => {
    let isMounted = true;

    void loadAppSettings()
      .then((loaded) => {
        if (!isMounted) {
          return;
        }

        setSettings(loaded);
        setClearOpenAiApiKey(false);
      })
      .catch((error) => {
        if (!isMounted) {
          return;
        }

        setErrorMessage(getErrorMessage(error, "Could not load Feedback settings."));
      })
      .finally(() => {
        if (isMounted) {
          setIsLoading(false);
        }
      });

    return () => {
      isMounted = false;
    };
  }, []);

  function updateField<Key extends keyof AppSettingsView>(key: Key, value: AppSettingsView[Key]) {
    setSettings((current) => ({ ...current, [key]: value }));
  }

  async function handleSave(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setIsSaving(true);
    setErrorMessage(null);
    setStatusMessage(null);

    const payload: AppSettingsSavePayload = {
      transcriptionProvider: settings.transcriptionProvider,
      openaiModel: settings.openaiModel,
      openaiBaseUrl: settings.openaiBaseUrl,
      openaiPrompt: settings.openaiPrompt,
      whisperBinaryPath: settings.whisperBinaryPath,
      whisperModelPath: settings.whisperModelPath,
      whisperLanguage: settings.whisperLanguage,
      clearOpenAiApiKey,
      openaiApiKey: apiKeyInput.trim() || undefined,
    };

    try {
      const saved = await saveAppSettings(payload);
      setSettings(saved);
      setApiKeyInput("");
      setClearOpenAiApiKey(false);
      setStatusMessage("Saved.");
    } catch (error) {
      setErrorMessage(getErrorMessage(error, "Could not save Feedback settings."));
    } finally {
      setIsSaving(false);
    }
  }

  const usingOpenAi = settings.transcriptionProvider === "openai";

  return (
    <main className="settings-shell">
      <section className="settings-card">
        <header className="settings-header">
          <div className="settings-header-copy">
            <p className="settings-eyebrow">Feedback</p>
            <h1 className="settings-title">Transcription Settings</h1>
            <p className="settings-copy">
              Pick your transcription engine, keep secrets in Keychain, and point the app at any
              local Whisper tools you want to use.
            </p>
          </div>
          <div className="settings-header-meta">
            <span className="settings-meta-label">Config JSON</span>
            <code>{settings.configPath || "Created on first save"}</code>
          </div>
        </header>

        {isLoading ? (
          <p className="settings-feedback">Loading settings…</p>
        ) : (
          <form className="settings-form" onSubmit={handleSave}>
            <section className="settings-panel">
              <div className="settings-panel-header">
                <div>
                  <h2 className="settings-panel-title">Provider</h2>
                  <p className="settings-panel-copy">
                    Switch between OpenAI transcription and a fully local Whisper pipeline.
                  </p>
                </div>
              </div>

              <div className="settings-provider-switch" role="tablist" aria-label="Provider">
                <button
                  type="button"
                  className={`settings-provider-option ${usingOpenAi ? "is-active" : ""}`}
                  onClick={() => {
                    updateField("transcriptionProvider", "openai" as TranscriptionProvider);
                    setStatusMessage(null);
                  }}
                >
                  <strong>OpenAI</strong>
                  <span>Keychain-backed API key</span>
                </button>
                <button
                  type="button"
                  className={`settings-provider-option ${!usingOpenAi ? "is-active" : ""}`}
                  onClick={() => {
                    updateField("transcriptionProvider", "local_whisper" as TranscriptionProvider);
                    setStatusMessage(null);
                  }}
                >
                  <strong>Local Whisper</strong>
                  <span>`whisper-cli` + local model</span>
                </button>
              </div>
            </section>

            {usingOpenAi ? (
              <section className="settings-panel">
                <div className="settings-panel-header">
                  <div>
                    <h2 className="settings-panel-title">OpenAI</h2>
                    <p className="settings-panel-copy">
                      Good default when you want less setup and strong speech quality.
                    </p>
                  </div>
                  <div className={`settings-badge ${settings.hasOpenAiApiKey ? "is-ready" : ""}`}>
                    {settings.hasOpenAiApiKey ? "Key in Keychain" : "No API key saved"}
                  </div>
                </div>

                <div className="settings-grid">
                  <label className="settings-field settings-field-full">
                    <span>OpenAI API key</span>
                    <input
                      type="password"
                      value={apiKeyInput}
                      placeholder={settings.hasOpenAiApiKey ? "Stored in Keychain" : "sk-..."}
                      onChange={(event) => {
                        setApiKeyInput(event.target.value);
                        if (event.target.value.trim()) {
                          setClearOpenAiApiKey(false);
                        }
                        setStatusMessage(null);
                      }}
                    />
                  </label>

                  <label className="settings-toggle settings-field-full">
                    <input
                      type="checkbox"
                      checked={clearOpenAiApiKey}
                      onChange={(event) => {
                        setClearOpenAiApiKey(event.target.checked);
                        if (event.target.checked) {
                          setApiKeyInput("");
                        }
                        setStatusMessage(null);
                      }}
                    />
                    <span>Clear the saved OpenAI key from Keychain</span>
                  </label>

                  <label className="settings-field">
                    <span>Model</span>
                    <input
                      type="text"
                      value={settings.openaiModel}
                      onChange={(event) => {
                        updateField("openaiModel", event.target.value);
                        setStatusMessage(null);
                      }}
                    />
                  </label>

                  <label className="settings-field">
                    <span>Base URL</span>
                    <input
                      type="text"
                      value={settings.openaiBaseUrl}
                      onChange={(event) => {
                        updateField("openaiBaseUrl", event.target.value);
                        setStatusMessage(null);
                      }}
                    />
                  </label>

                  <label className="settings-field settings-field-full">
                    <span>Prompt</span>
                    <textarea
                      rows={3}
                      value={settings.openaiPrompt}
                      placeholder="Optional context for product, UI, or review language."
                      onChange={(event) => {
                        updateField("openaiPrompt", event.target.value);
                        setStatusMessage(null);
                      }}
                    />
                  </label>
                </div>
              </section>
            ) : (
              <section className="settings-panel">
                <div className="settings-panel-header">
                  <div>
                    <h2 className="settings-panel-title">Local Whisper</h2>
                    <p className="settings-panel-copy">
                      Fully offline transcription using a `whisper.cpp` style CLI and a local
                      model file.
                    </p>
                  </div>
                </div>

                <div className="settings-grid">
                  <label className="settings-field settings-field-full">
                    <span>`whisper-cli` binary path</span>
                    <input
                      type="text"
                      value={settings.whisperBinaryPath}
                      placeholder="/opt/homebrew/bin/whisper-cli"
                      onChange={(event) => {
                        updateField("whisperBinaryPath", event.target.value);
                        setStatusMessage(null);
                      }}
                    />
                  </label>

                  <label className="settings-field settings-field-full">
                    <span>Whisper model path</span>
                    <input
                      type="text"
                      value={settings.whisperModelPath}
                      placeholder="~/models/ggml-base.en.bin"
                      onChange={(event) => {
                        updateField("whisperModelPath", event.target.value);
                        setStatusMessage(null);
                      }}
                    />
                  </label>

                  <label className="settings-field">
                    <span>Language override</span>
                    <input
                      type="text"
                      value={settings.whisperLanguage}
                      placeholder="Blank = auto-detect"
                      onChange={(event) => {
                        updateField("whisperLanguage", event.target.value);
                        setStatusMessage(null);
                      }}
                    />
                  </label>

                  <div className="settings-note-card">
                    Local Whisper expects a working `whisper-cli` binary plus a local GGML or GGUF
                    model file.
                  </div>
                </div>
              </section>
            )}

            <footer className="settings-footer">
              <div className="settings-footer-messages">
                {errorMessage ? <p className="settings-feedback is-error">{errorMessage}</p> : null}
                {statusMessage ? <p className="settings-feedback is-success">{statusMessage}</p> : null}
              </div>

              <div className="settings-actions">
                <button type="submit" className="settings-save" disabled={isSaving}>
                  {isSaving ? "Saving…" : "Save settings"}
                </button>
              </div>
            </footer>
          </form>
        )}
      </section>
    </main>
  );
}
