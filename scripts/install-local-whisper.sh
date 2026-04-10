#!/bin/sh
set -eu

BREW_BIN=""
for candidate in /opt/homebrew/bin/brew /usr/local/bin/brew; do
  if [ -x "$candidate" ]; then
    BREW_BIN="$candidate"
    break
  fi
done

if [ -z "$BREW_BIN" ]; then
  BREW_BIN="$(command -v brew || true)"
fi

if [ -z "$BREW_BIN" ]; then
  echo "Homebrew is required to install Local Whisper." >&2
  exit 1
fi

if ! "$BREW_BIN" list --versions whisper-cpp >/dev/null 2>&1; then
  "$BREW_BIN" install whisper-cpp
fi

WHISPER_BIN="$(command -v whisper-cli || true)"
if [ -z "$WHISPER_BIN" ]; then
  FORMULA_PREFIX="$("$BREW_BIN" --prefix whisper-cpp 2>/dev/null || true)"
  if [ -n "$FORMULA_PREFIX" ] && [ -x "$FORMULA_PREFIX/bin/whisper-cli" ]; then
    WHISPER_BIN="$FORMULA_PREFIX/bin/whisper-cli"
  fi
fi

if [ -z "$WHISPER_BIN" ]; then
  echo "whisper-cli was not found after installing whisper-cpp." >&2
  exit 1
fi

MODEL_DIR="${1:-$HOME/Library/Application Support/com.alexpickett.imagediction/whisper-models}"
MODEL_PATH="$MODEL_DIR/ggml-base.en.bin"
mkdir -p "$MODEL_DIR"

if [ ! -f "$MODEL_PATH" ]; then
  curl -L --fail --output "$MODEL_PATH" \
    "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin"
fi

printf 'binary=%s\n' "$WHISPER_BIN"
printf 'model=%s\n' "$MODEL_PATH"
