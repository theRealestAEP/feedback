#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
BUILD_APP="$ROOT_DIR/src-tauri/target/debug/bundle/macos/Feedback.app"
INSTALL_APP="/Applications/Feedback.app"
LEGACY_INSTALL_APP="/Applications/ImageDiction.app"
APP_BUNDLE_ID="com.alexpickett.imagediction"
SIDECAR_BUNDLE_ID="com.alexpickett.imagediction.sidecar"
APP_REQUIREMENT="designated => identifier \"$APP_BUNDLE_ID\""
SIDECAR_REQUIREMENT="designated => identifier \"$SIDECAR_BUNDLE_ID\""

if [ ! -d "$BUILD_APP" ]; then
  echo "Built app not found at: $BUILD_APP" >&2
  echo "Run 'cargo tauri build --debug --bundles app' first." >&2
  exit 1
fi

APP_EXECUTABLE="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$BUILD_APP/Contents/Info.plist")"

pkill imagediction >/dev/null 2>&1 || true
rm -rf "$LEGACY_INSTALL_APP"
rm -rf "$INSTALL_APP"
cp -R "$BUILD_APP" "$INSTALL_APP"

if [ -f "$INSTALL_APP/Contents/MacOS/imagediction-sidecar" ]; then
  codesign --force --sign - --identifier "$SIDECAR_BUNDLE_ID" \
    -r="$SIDECAR_REQUIREMENT" \
    "$INSTALL_APP/Contents/MacOS/imagediction-sidecar"
fi

codesign --force --sign - --identifier "$APP_BUNDLE_ID" \
  -r="$APP_REQUIREMENT" \
  "$INSTALL_APP/Contents/MacOS/$APP_EXECUTABLE"

codesign --force --deep --sign - --identifier "$APP_BUNDLE_ID" \
  -r="$APP_REQUIREMENT" \
  "$INSTALL_APP"

if [ -f "$ROOT_DIR/.env" ]; then
  set -a
  # shellcheck disable=SC1091
  source "$ROOT_DIR/.env" >/dev/null 2>&1 || true
  set +a

  if [ -n "${OPENAI_API_KEY:-}" ]; then
    printf '%s' "$OPENAI_API_KEY" | /usr/bin/swift - \
      "com.alexpickett.imagediction.openai-api-key" \
      "openai" <<'SWIFT' >/dev/null 2>&1 || true
import Foundation
import Security

let service = CommandLine.arguments.dropFirst().first ?? ""
let account = CommandLine.arguments.dropFirst(2).first ?? ""
let password = String(
  data: FileHandle.standardInput.readDataToEndOfFile(),
  encoding: .utf8
)?
  .trimmingCharacters(in: .whitespacesAndNewlines) ?? ""

guard !service.isEmpty, !account.isEmpty, !password.isEmpty else {
  exit(0)
}

let passwordData = Data(password.utf8)
let query: [String: Any] = [
  kSecClass as String: kSecClassGenericPassword,
  kSecAttrService as String: service,
  kSecAttrAccount as String: account
]

let status = SecItemCopyMatching(query as CFDictionary, nil)
if status == errSecSuccess {
  let updateStatus = SecItemUpdate(
    query as CFDictionary,
    [kSecValueData as String: passwordData] as CFDictionary
  )
  guard updateStatus == errSecSuccess else { exit(1) }
} else if status == errSecItemNotFound {
  var newItem = query
  newItem[kSecValueData as String] = passwordData
  guard SecItemAdd(newItem as CFDictionary, nil) == errSecSuccess else { exit(1) }
} else {
  exit(1)
}
SWIFT
    echo "Seeded OpenAI API key into Keychain for $APP_BUNDLE_ID without argv exposure"
  fi
fi

echo "Installed: $INSTALL_APP"
codesign -dv --verbose=4 "$INSTALL_APP" 2>&1 | sed -n '1,20p'

open "$INSTALL_APP"
