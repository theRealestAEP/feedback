import AppKit
import AVFoundation
import CoreGraphics
import Foundation
import ScreenCaptureKit
import Speech

struct SidecarError: Error, CustomStringConvertible {
  let description: String
}

final class OverlayWindow: NSWindow {
  override var canBecomeKey: Bool { true }
  override var canBecomeMain: Bool { true }
}

final class RecordingDelegate: NSObject, AVAudioRecorderDelegate {
  private let completion: (Bool, Error?) -> Void

  init(completion: @escaping (Bool, Error?) -> Void) {
    self.completion = completion
  }

  func audioRecorderDidFinishRecording(_ recorder: AVAudioRecorder, successfully flag: Bool) {
    completion(flag, nil)
  }

  func audioRecorderEncodeErrorDidOccur(_ recorder: AVAudioRecorder, error: Error?) {
    completion(false, error)
  }
}

@MainActor
final class InteractiveCaptureController: NSObject {
  private var window: OverlayWindow?
  private var result: CGRect?

  func runSelection() -> CGRect? {
    let screens = NSScreen.screens
    guard !screens.isEmpty else {
      return nil
    }

    let unionFrame = screens.reduce(into: NSRect.null) { partial, screen in
      partial = partial.union(screen.frame)
    }

    let overlay = CaptureOverlayView(frame: NSRect(origin: .zero, size: unionFrame.size))
    overlay.controller = self

    let window = OverlayWindow(
      contentRect: unionFrame,
      styleMask: [.borderless],
      backing: .buffered,
      defer: false
    )
    window.level = .screenSaver
    window.backgroundColor = .clear
    window.isOpaque = false
    window.hasShadow = false
    window.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .stationary]
    window.ignoresMouseEvents = false
    window.contentView = overlay
    window.makeKeyAndOrderFront(nil)
    window.makeFirstResponder(overlay)
    NSApp.activate(ignoringOtherApps: true)

    self.window = window
    RunLoop.main.run()
    return result
  }

  func finish(with rect: CGRect?) {
    result = rect
    window?.orderOut(nil)
    window = nil
    CFRunLoopStop(CFRunLoopGetMain())
  }
}

@MainActor
final class CaptureOverlayView: NSView {
  weak var controller: InteractiveCaptureController?

  private var startPoint: NSPoint?
  private var currentPoint: NSPoint?

  override var acceptsFirstResponder: Bool { true }

  override func mouseDown(with event: NSEvent) {
    let point = convert(event.locationInWindow, from: nil)
    startPoint = point
    currentPoint = point
    needsDisplay = true
  }

  override func mouseDragged(with event: NSEvent) {
    currentPoint = convert(event.locationInWindow, from: nil)
    needsDisplay = true
  }

  override func mouseUp(with event: NSEvent) {
    currentPoint = convert(event.locationInWindow, from: nil)
    guard let rect = selectionRect, rect.width >= 4, rect.height >= 4, let window else {
      controller?.finish(with: nil)
      return
    }

    let windowRect = convert(rect, to: nil)
    let screenRect = window.convertToScreen(windowRect)
    controller?.finish(with: screenRect)
  }

  override func keyDown(with event: NSEvent) {
    if event.keyCode == 53 {
      controller?.finish(with: nil)
      return
    }

    super.keyDown(with: event)
  }

  override func draw(_ dirtyRect: NSRect) {
    NSColor(calibratedWhite: 0, alpha: 0.18).setFill()
    bounds.fill()

    guard let selectionRect else {
      return
    }

    NSGraphicsContext.current?.saveGraphicsState()
    NSColor.clear.setFill()
    __NSRectFillUsingOperation(selectionRect, .clear)
    NSGraphicsContext.current?.restoreGraphicsState()

    let outline = NSBezierPath(roundedRect: selectionRect, xRadius: 10, yRadius: 10)
    outline.lineWidth = 2
    NSColor(calibratedRed: 1, green: 0.69, blue: 0.36, alpha: 0.98).setStroke()
    outline.stroke()
  }

  private var selectionRect: NSRect? {
    guard let startPoint, let currentPoint else {
      return nil
    }

    return NSRect(
      x: min(startPoint.x, currentPoint.x),
      y: min(startPoint.y, currentPoint.y),
      width: abs(currentPoint.x - startPoint.x),
      height: abs(currentPoint.y - startPoint.y)
    ).integral
  }
}

@main
struct ImageDictionSidecar {
  static func main() async {
    do {
      let app = NSApplication.shared
      app.setActivationPolicy(.accessory)

      let args = Array(CommandLine.arguments.dropFirst())
      guard let command = args.first else {
        throw SidecarError(description: "missing sidecar command")
      }

      let options = parseOptions(Array(args.dropFirst()))

      switch command {
      case "capture":
        try await handleCapture(options: options)
      case "interactive-capture":
        try await handleInteractiveCapture(options: options)
      case "record":
        try await handleRecord(options: options)
      case "transcribe":
        try await handleTranscription(options: options)
      default:
        throw SidecarError(description: "unsupported sidecar command '\(command)'")
      }
    } catch {
      fputs("\(error)\n", stderr)
      exit(1)
    }
  }

  static func handleCapture(options: [String: String]) async throws {
    guard
      let x = Double(options["x"] ?? ""),
      let y = Double(options["y"] ?? ""),
      let width = Double(options["width"] ?? ""),
      let height = Double(options["height"] ?? ""),
      let output = options["output"]
    else {
      throw SidecarError(description: "capture requires --x --y --width --height --output")
    }

    guard width > 0, height > 0 else {
      throw SidecarError(description: "capture width and height must be positive")
    }

    guard CGPreflightScreenCaptureAccess() else {
      throw SidecarError(
        description:
          "Feedback does not currently have Screen Recording permission. Enable it in System Settings > Privacy & Security > Screen Recording."
      )
    }

    let requestedRect = CGRect(x: x, y: y, width: width, height: height)
    let content = try await SCShareableContent.current

    guard let display = content.displays.first(where: { $0.frame.intersects(requestedRect) }) ?? content.displays.first else {
      throw SidecarError(description: "no display was available for screen capture")
    }

    let displayRect = display.frame
    let sourceRect = CGRect(
      x: requestedRect.origin.x - displayRect.origin.x,
      y: requestedRect.origin.y - displayRect.origin.y,
      width: requestedRect.width,
      height: requestedRect.height
    )

    let filter = SCContentFilter(display: display, excludingWindows: [])
    let configuration = SCStreamConfiguration()
    configuration.width = Int(sourceRect.width.rounded(.up))
    configuration.height = Int(sourceRect.height.rounded(.up))
    configuration.sourceRect = sourceRect
    configuration.showsCursor = true

    let image = try await SCScreenshotManager.captureImage(
      contentFilter: filter,
      configuration: configuration
    )

    let representation = NSBitmapImageRep(cgImage: image)
    guard let data = representation.representation(using: .png, properties: [:]) else {
      throw SidecarError(description: "failed to encode screenshot as PNG")
    }

    let outputURL = URL(fileURLWithPath: output)
    try FileManager.default.createDirectory(
      at: outputURL.deletingLastPathComponent(),
      withIntermediateDirectories: true
    )
    try data.write(to: outputURL, options: .atomic)
  }

  static func handleInteractiveCapture(options: [String: String]) async throws {
    guard let output = options["output"] else {
      throw SidecarError(description: "interactive-capture requires --output")
    }

    guard CGPreflightScreenCaptureAccess() else {
      throw SidecarError(
        description:
          "Feedback does not currently have Screen Recording permission. Enable it in System Settings > Privacy & Security > Screen Recording."
      )
    }

    let outputURL = URL(fileURLWithPath: output)
    let controller = await MainActor.run { InteractiveCaptureController() }
    let selection = await MainActor.run {
      controller.runSelection()
    }

    guard let selection else {
      return
    }

    try await Task.sleep(nanoseconds: 120_000_000)
    try await capture(rect: selection, outputURL: outputURL)
  }

  static func handleRecord(options: [String: String]) async throws {
    guard let output = options["output"] else {
      throw SidecarError(description: "record requires --output")
    }

    let authorized = await requestMicrophoneAccess()
    guard authorized else {
      throw SidecarError(description: "microphone permission was denied")
    }

    let outputURL = URL(fileURLWithPath: output)
    try FileManager.default.createDirectory(
      at: outputURL.deletingLastPathComponent(),
      withIntermediateDirectories: true
    )

    let settings: [String: Any]
    if outputURL.pathExtension.lowercased() == "wav" {
      settings = [
        AVFormatIDKey: kAudioFormatLinearPCM,
        AVSampleRateKey: 16_000,
        AVNumberOfChannelsKey: 1,
        AVLinearPCMBitDepthKey: 16,
        AVLinearPCMIsFloatKey: false,
        AVLinearPCMIsBigEndianKey: false
      ]
    } else {
      settings = [
        AVFormatIDKey: kAudioFormatMPEG4AAC,
        AVSampleRateKey: 44_100,
        AVNumberOfChannelsKey: 1,
        AVEncoderBitRateKey: 96_000,
        AVEncoderAudioQualityKey: AVAudioQuality.high.rawValue
      ]
    }

    var recorder: AVAudioRecorder? = try AVAudioRecorder(url: outputURL, settings: settings)
    var recordingError: Error?
    var didFinishRecording = false
    let delegate = RecordingDelegate { success, error in
      didFinishRecording = success
      recordingError = error
      CFRunLoopStop(CFRunLoopGetMain())
    }

    recorder?.delegate = delegate
    recorder?.isMeteringEnabled = false
    recorder?.prepareToRecord()

    guard recorder?.record() == true else {
      throw SidecarError(description: "failed to start microphone recording")
    }

    DispatchQueue.global(qos: .userInitiated).async {
      _ = FileHandle.standardInput.readDataToEndOfFile()
      DispatchQueue.main.async {
        recorder?.stop()
      }
    }

    CFRunLoopRun()
    recorder = nil
    _ = delegate

    if let recordingError {
      throw recordingError
    }

    if !FileManager.default.fileExists(atPath: outputURL.path) {
      throw SidecarError(description: "recording finished without creating an audio file")
    }

    let attributes = try FileManager.default.attributesOfItem(atPath: outputURL.path)
    let size = (attributes[.size] as? NSNumber)?.intValue ?? 0
    if !didFinishRecording && size <= 1024 {
      throw SidecarError(description: "microphone recording did not finish cleanly")
    }

    if size <= 1024 {
      throw SidecarError(description: "recording finished without any audio data")
    }
  }

  static func handleTranscription(options: [String: String]) async throws {
    guard let input = options["input"] else {
      throw SidecarError(description: "transcribe requires --input")
    }

    let authorization = await requestSpeechAuthorization()
    guard authorization == .authorized else {
      throw SidecarError(description: "speech recognition permission was denied")
    }

    let recognizer = SFSpeechRecognizer(locale: Locale.autoupdatingCurrent)
      ?? SFSpeechRecognizer(locale: Locale(identifier: "en-US"))

    guard let recognizer, recognizer.isAvailable else {
      throw SidecarError(description: "speech recognizer is unavailable")
    }

    guard recognizer.supportsOnDeviceRecognition else {
      throw SidecarError(description: "on-device speech recognition is unavailable for the current locale")
    }

    let request = SFSpeechURLRecognitionRequest(url: URL(fileURLWithPath: input))
    request.requiresOnDeviceRecognition = true
    request.shouldReportPartialResults = false

    let transcript = try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<String, Error>) in
      var hasResumed = false
      var recognitionTask: SFSpeechRecognitionTask?

      recognitionTask = recognizer.recognitionTask(with: request) { result, error in
        if hasResumed {
          return
        }

        if let error {
          hasResumed = true
          recognitionTask?.cancel()
          continuation.resume(throwing: error)
          return
        }

        guard let result else {
          return
        }

        if result.isFinal {
          hasResumed = true
          recognitionTask?.cancel()
          continuation.resume(returning: result.bestTranscription.formattedString)
        }
      }
    }

    let payload = [
      "transcript": transcript,
      "onDevice": true
    ] as [String : Any]
    let data = try JSONSerialization.data(withJSONObject: payload, options: [])
    FileHandle.standardOutput.write(data)
  }

  static func requestSpeechAuthorization() async -> SFSpeechRecognizerAuthorizationStatus {
    await withCheckedContinuation { continuation in
      SFSpeechRecognizer.requestAuthorization { status in
        continuation.resume(returning: status)
      }
    }
  }

  static func requestMicrophoneAccess() async -> Bool {
    switch AVCaptureDevice.authorizationStatus(for: .audio) {
    case .authorized:
      return true
    case .notDetermined:
      return await withCheckedContinuation { continuation in
        AVCaptureDevice.requestAccess(for: .audio) { granted in
          continuation.resume(returning: granted)
        }
      }
    case .denied, .restricted:
      return false
    @unknown default:
      return false
    }
  }

  static func parseOptions(_ args: [String]) -> [String: String] {
    var options: [String: String] = [:]
    var index = 0

    while index < args.count {
      let token = args[index]
      if token.hasPrefix("--"), index + 1 < args.count {
        let key = String(token.dropFirst(2))
        options[key] = args[index + 1]
        index += 2
      } else {
        index += 1
      }
    }

    return options
  }

  static func capture(rect: CGRect, outputURL: URL) async throws {
    let content = try await SCShareableContent.current

    guard let display = content.displays.first(where: { $0.frame.intersects(rect) }) ?? content.displays.first else {
      throw SidecarError(description: "no display was available for screen capture")
    }

    let displayRect = display.frame
    let sourceRect = CGRect(
      x: rect.origin.x - displayRect.origin.x,
      y: rect.origin.y - displayRect.origin.y,
      width: rect.width,
      height: rect.height
    )

    let filter = SCContentFilter(display: display, excludingWindows: [])
    let configuration = SCStreamConfiguration()
    configuration.width = Int(sourceRect.width.rounded(.up))
    configuration.height = Int(sourceRect.height.rounded(.up))
    configuration.sourceRect = sourceRect
    configuration.showsCursor = true

    let image = try await SCScreenshotManager.captureImage(
      contentFilter: filter,
      configuration: configuration
    )

    let representation = NSBitmapImageRep(cgImage: image)
    guard let data = representation.representation(using: .png, properties: [:]) else {
      throw SidecarError(description: "failed to encode screenshot as PNG")
    }

    try FileManager.default.createDirectory(
      at: outputURL.deletingLastPathComponent(),
      withIntermediateDirectories: true
    )
    try data.write(to: outputURL, options: .atomic)
  }
}
