import AppKit
import ApplicationServices
import CoreGraphics
import Foundation

private enum HudState: String, Decodable {
    case detecting
    case recording
    case transcribing
    case hidden
}

private struct HudCommand: Decodable {
    let state: HudState
    let level: Double?
}

private final class HudView: NSView {
    var state: HudState = .hidden {
        didSet { needsDisplay = true }
    }

    var level: Double = 0 {
        didSet { needsDisplay = true }
    }

    private var phase: Double = 0

    override var isFlipped: Bool { true }

    func tick() {
        phase += 0.18
        needsDisplay = true
    }

    override func draw(_ dirtyRect: NSRect) {
        guard state != .hidden else { return }

        let bounds = self.bounds.insetBy(dx: 0.5, dy: 0.5)
        let background = NSBezierPath(roundedRect: bounds, xRadius: 8, yRadius: 8)
        NSColor(calibratedWhite: 0.08, alpha: 0.88).setFill()
        background.fill()
        NSColor(calibratedWhite: 1.0, alpha: 0.16).setStroke()
        background.lineWidth = 1
        background.stroke()

        drawDot(in: bounds)
        drawTitle(in: bounds)
        drawMeter(in: bounds)
    }

    private func drawDot(in bounds: NSRect) {
        let color: NSColor
        switch state {
        case .detecting:
            color = NSColor.systemTeal
        case .recording:
            color = NSColor.systemRed
        case .transcribing:
            color = NSColor.systemOrange
        case .hidden:
            return
        }

        let dot = NSBezierPath(ovalIn: NSRect(x: bounds.minX + 10, y: bounds.midY - 4, width: 8, height: 8))
        color.setFill()
        dot.fill()
    }

    private func drawTitle(in bounds: NSRect) {
        let title: String
        switch state {
        case .detecting:
            title = "Detecting"
        case .recording:
            title = "Recording"
        case .transcribing:
            title = "Transcribing"
        case .hidden:
            return
        }

        let attributes: [NSAttributedString.Key: Any] = [
            .font: NSFont.systemFont(ofSize: 12, weight: .semibold),
            .foregroundColor: NSColor(calibratedWhite: 1.0, alpha: 0.92),
        ]
        NSString(string: title).draw(at: NSPoint(x: bounds.minX + 24, y: bounds.minY + 7), withAttributes: attributes)
    }

    private func drawMeter(in bounds: NSRect) {
        let startX = bounds.maxX - 47
        let centerY = bounds.midY
        let bars = 5

        for index in 0..<bars {
            let animated = (sin(phase + Double(index) * 0.78) + 1.0) * 0.5
            let activity: Double
            switch state {
            case .detecting:
                activity = 0.12
            case .recording:
                activity = max(0.24, min(1.0, level + animated * 0.42))
            case .transcribing:
                activity = 0.18 + animated * 0.24
            case .hidden:
                activity = 0
            }

            let height = 5 + CGFloat(activity) * 15
            let rect = NSRect(
                x: startX + CGFloat(index * 8),
                y: centerY - height / 2,
                width: 4,
                height: height
            )
            let path = NSBezierPath(roundedRect: rect, xRadius: 2, yRadius: 2)
            NSColor(calibratedWhite: 1.0, alpha: state == .recording ? 0.82 : 0.48).setFill()
            path.fill()
        }
    }
}

private final class HudController {
    private let window: NSWindow
    private let view: HudView
    private let logPath: String?
    private var timer: Timer?

    init(logPath: String?) {
        self.logPath = logPath
        self.view = HudView(frame: NSRect(x: 0, y: 0, width: 138, height: 34))
        self.window = NSWindow(
            contentRect: view.frame,
            styleMask: [.borderless],
            backing: .buffered,
            defer: false
        )

        window.contentView = view
        window.backgroundColor = .clear
        window.isOpaque = false
        window.hasShadow = true
        window.ignoresMouseEvents = true
        window.level = .statusBar
        window.collectionBehavior = [.canJoinAllSpaces, .transient, .ignoresCycle]
        window.isReleasedWhenClosed = false

        timer = Timer.scheduledTimer(withTimeInterval: 1.0 / 30.0, repeats: true) { [weak self] _ in
            self?.view.tick()
        }
    }

    func apply(_ command: HudCommand) {
        appendLog(command)
        view.state = command.state
        view.level = command.level ?? 0

        if command.state == .hidden {
            window.orderOut(nil)
            return
        }

        window.setFrame(positionedFrame(), display: true)
        window.orderFrontRegardless()
    }

    private func appendLog(_ command: HudCommand) {
        guard let logPath else { return }
        let line = "\(Date().timeIntervalSince1970) state=\(command.state.rawValue) level=\(command.level ?? 0)\n"
        guard let data = line.data(using: .utf8) else { return }

        if FileManager.default.fileExists(atPath: logPath),
           let handle = try? FileHandle(forWritingTo: URL(fileURLWithPath: logPath)) {
            _ = try? handle.seekToEnd()
            try? handle.write(contentsOf: data)
            try? handle.close()
        } else {
            try? data.write(to: URL(fileURLWithPath: logPath))
        }
    }

    private func positionedFrame() -> NSRect {
        let size = view.frame.size
        let visible = NSScreen.main?.visibleFrame ?? NSRect(x: 0, y: 0, width: 1440, height: 900)
        let anchor = caretRectFromAccessibility() ?? focusedElementRectFromAccessibility()

        guard let anchor else {
            return NSRect(x: visible.midX - size.width / 2, y: visible.minY + 72, width: size.width, height: size.height)
        }

        let screen = NSScreen.screens.first { $0.visibleFrame.intersects(anchor) } ?? NSScreen.main
        let frame = screen?.visibleFrame ?? visible
        let gap: CGFloat = 10

        var x = anchor.maxX + gap
        var y = anchor.minY - size.height - 6

        if x + size.width > frame.maxX - 8 {
            x = anchor.minX - size.width - gap
        }
        if y < frame.minY + 8 {
            y = anchor.maxY + 6
        }
        if x < frame.minX + 8 {
            x = frame.minX + 8
        }
        if y + size.height > frame.maxY - 8 {
            y = frame.maxY - size.height - 8
        }

        return NSRect(x: x, y: y, width: size.width, height: size.height)
    }
}

private func caretRectFromAccessibility() -> NSRect? {
    guard AXIsProcessTrusted() else { return nil }
    guard let focused = focusedAccessibilityElement() else { return nil }

    var rangeValue: CFTypeRef?
    guard AXUIElementCopyAttributeValue(focused, kAXSelectedTextRangeAttribute as CFString, &rangeValue) == .success,
          let rangeValue else { return nil }

    var range = CFRange()
    guard AXValueGetValue(rangeValue as! AXValue, .cfRange, &range) else { return nil }
    if range.length > 0 {
        range.length = 0
    }

    guard let axRange = AXValueCreate(.cfRange, &range) else { return nil }
    var boundsValue: CFTypeRef?
    guard AXUIElementCopyParameterizedAttributeValue(
        focused,
        kAXBoundsForRangeParameterizedAttribute as CFString,
        axRange,
        &boundsValue
    ) == .success,
    let boundsValue else { return nil }

    var rect = CGRect.zero
    guard AXValueGetValue(boundsValue as! AXValue, .cgRect, &rect), !rect.isNull, !rect.isEmpty else {
        return nil
    }

    return accessibilityRectToAppKit(rect)
}

private func focusedElementRectFromAccessibility() -> NSRect? {
    guard AXIsProcessTrusted() else { return nil }
    guard let focused = focusedAccessibilityElement() else { return nil }

    var positionValue: CFTypeRef?
    var sizeValue: CFTypeRef?
    guard AXUIElementCopyAttributeValue(focused, kAXPositionAttribute as CFString, &positionValue) == .success,
          AXUIElementCopyAttributeValue(focused, kAXSizeAttribute as CFString, &sizeValue) == .success,
          let positionValue,
          let sizeValue else { return nil }

    var position = CGPoint.zero
    var size = CGSize.zero
    guard AXValueGetValue(positionValue as! AXValue, .cgPoint, &position),
          AXValueGetValue(sizeValue as! AXValue, .cgSize, &size) else {
        return nil
    }

    let rect = CGRect(origin: position, size: size)
    guard !rect.isNull, !rect.isEmpty else { return nil }

    return accessibilityRectToAppKit(rect)
}

private func focusedAccessibilityElement() -> AXUIElement? {
    let systemWide = AXUIElementCreateSystemWide()
    var focused: CFTypeRef?
    guard AXUIElementCopyAttributeValue(systemWide, kAXFocusedUIElementAttribute as CFString, &focused) == .success else {
        return nil
    }
    return focused as! AXUIElement?
}

private func accessibilityRectToAppKit(_ rect: CGRect) -> NSRect {
    let screen = NSScreen.screens.first { screen in
        let frame = screen.frame
        return rect.midX >= frame.minX && rect.midX <= frame.maxX
    } ?? NSScreen.main

    guard let screen else { return NSRect(x: rect.minX, y: rect.minY, width: rect.width, height: rect.height) }
    let y = screen.frame.maxY - rect.maxY + screen.frame.minY
    return NSRect(x: rect.minX, y: y, width: rect.width, height: rect.height)
}

private func runStdinLoop(controller: HudController) {
    DispatchQueue.global(qos: .userInitiated).async {
        let decoder = JSONDecoder()
        while let line = readLine() {
            guard let data = line.data(using: .utf8),
                  let command = try? decoder.decode(HudCommand.self, from: data) else {
                continue
            }

            DispatchQueue.main.async {
                controller.apply(command)
            }
        }

        DispatchQueue.main.async {
            NSApp.terminate(nil)
        }
    }
}

private func runDemo(controller: HudController, state: HudState) {
    controller.apply(HudCommand(state: state, level: state == .recording ? 0.7 : nil))
    DispatchQueue.main.asyncAfter(deadline: .now() + 4.0) {
        NSApp.terminate(nil)
    }
}

let app = NSApplication.shared
app.setActivationPolicy(.accessory)
app.finishLaunching()

let logPath = ProcessInfo.processInfo.environment["VOICE_INPUT_RECORDING_HUD_LOG_PATH"]
private let controller = HudController(logPath: logPath)

if CommandLine.arguments.count >= 3, CommandLine.arguments[1] == "--demo",
   let state = HudState(rawValue: CommandLine.arguments[2]) {
    runDemo(controller: controller, state: state)
} else {
    runStdinLoop(controller: controller)
}

app.run()
