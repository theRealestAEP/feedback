# Feedback

Feedback is a tiny macOS utility for one job:

Dictate product or design feedback, take screenshots while you talk, and save the whole session in an LLM-friendly folder so an agent can use the transcript and images together.

## Why It Exists

When you review UI, you usually do two things at once:

- talk through what feels wrong or what should change
- point at the screen

Feedback turns that into a simple artifact:

- one session folder per review
- audio transcript and screenshots saved locally
- screenshots inserted into the session timeline in the same order you took them
- markdown that is easy to hand to an LLM for implementation

## Core Workflow

1. Open the app. It lives as a small floating pill.
2. Click `Start` to begin a fresh review session.
3. Talk through your feedback.
4. While recording, click the screenshot button or use `Cmd+Shift+4`.
5. Click the stop square when you are done.
6. The pill shows a processing animation while audio is being finalized and transcribed.
7. When processing finishes, the session folder opens automatically in Finder.

## What Gets Saved

Sessions are stored in:

```bash
~/Documents/Feedback/sessions
```

Each session gets its own folder:

```text
sessions/
  2026-04-10-103422-review-apr-10-10-34/
    session.md
    session.meta.json
    assets/
      <capture>.png
      <clip>.wav
```

`session.md` is the human and LLM-friendly export.

`session.meta.json` keeps the structured session data.

## Settings

Open:

```text
Feedback -> Settings…
```

Available settings:

- transcription provider: `OpenAI` or `Local Whisper`
- OpenAI API key, stored in macOS Keychain
- OpenAI model and optional prompt
- local `whisper-cli` path
- local Whisper model path
- optional Whisper language

Important:

- `.env` is only a dev fallback
- real app usage should go through the Settings window
- OpenAI secrets are stored in Keychain, not in `settings.json`

## OpenAI Setup

1. Open `Feedback -> Settings…`
2. Leave provider on `OpenAI`
3. Paste your API key
4. Keep the default model unless you want to override it
5. Click `Save settings`

Recommended default model:

```text
gpt-4o-transcribe
```

## Local Whisper Setup

Feedback supports a local `whisper.cpp` style workflow through `whisper-cli`.

To test it:

1. Install `whisper-cli`
2. Download a local Whisper model
3. Open `Feedback -> Settings…`
4. Switch provider to `Local Whisper`
5. Set the binary path if auto-detect does not find it
6. Set the model path
7. Optionally set the language, like `en`
8. Save settings

Or let Feedback do the basic setup for you:

```bash
bun run install:whisper
```

That installs `whisper-cpp` through Homebrew and downloads the `ggml-base.en.bin` model into Feedback’s app support folder.

The app runs `whisper-cli` roughly like this:

```bash
whisper-cli -m /path/to/model.bin -f /path/to/audio.wav -otxt -of /tmp/output -nt -np
```

If Local Whisper says transcription is unavailable, usually one of these is missing:

- `whisper-cli` is not installed
- the binary path is wrong
- the model path is empty or points to a missing file

## Install And Run

Install dependencies:

```bash
bun install
```

Build, install into `/Applications`, seed the dev OpenAI key from `.env` into Keychain when present, and launch:

```bash
bun run app
```

Open the already-installed app:

```bash
bun run open:app
```

Reset Screen Recording and Microphone permissions for dev testing, then reopen the installed app:

```bash
bun run fresh:app
```

## Development

Frontend build:

```bash
bun run build
```

Rust tests:

```bash
bun run test:rust
```

Open the debug log:

```bash
bun run logs:app
```

Clear the debug log:

```bash
bun run logs:clear
```

## GitHub Releases

A release workflow lives at:

[`release.yml`](./.github/workflows/release.yml)

To create a draft release:

```bash
git tag v0.1.0
git push origin v0.1.0
```

Or run the workflow manually from GitHub Actions.

Current release caveat:

- builds are fine for testing
- public distribution still needs proper Apple signing and notarization
