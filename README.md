# Feedback

Local-first desktop review tool for collecting screenshots, boxed callouts, dictation clips, and markdown session notes with a much lighter utility-style UI.

## What it does

- Opens as a small floating control pod instead of a full dashboard.
- Starts a fresh dictation session every time you hit record.
- Opens a fullscreen capture overlay from the app or the global shortcut `Cmd+Shift+4`.
- Saves every session locally as:
  - `session.md`
  - `session.meta.json`
  - `assets/*.png`
  - `assets/*.wav`
- Lets you draw simple boxes over captured frames and attach one piece of feedback text.
- Records short dictation clips in the app and transcribes them through OpenAI when `OPENAI_API_KEY` is set.

## Tech stack

- Bun + React + TypeScript
- Tauri v2
- Rust backend for persistence and window orchestration
- Swift sidecar for screen capture and speech transcription

## Commands

```bash
bun install
bun run tauri dev
```

For native mic and screenshot testing on macOS, use the actual app bundle instead of `tauri dev`:

```bash
bun run app
```

Add your API key for transcription in `.env`:

```bash
OPENAI_API_KEY=your_key_here
OPENAI_TRANSCRIPTION_MODEL=gpt-4o-transcribe
```

Useful checks:

```bash
bun run build
bun run test:rust
cargo tauri build --debug
```

## Build outputs

The last verified desktop bundle was produced at:

- `src-tauri/target/debug/bundle/macos/Feedback.app`
- `src-tauri/target/debug/bundle/dmg/Feedback_0.1.0_aarch64.dmg`
