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

private enum AnchorSource: String {
    case selectedTextMarkerRange
    case selectedTextRange
    case adjacentCharacterRange
    case focusedElement
    case mouseFallback
    case screenFallback
}

private struct HudAnchor {
    let rect: NSRect
    let source: AnchorSource
    let isPrecise: Bool
}

private struct HudPlacement {
    let frame: NSRect
    let anchor: HudAnchor
}

private struct ElementSearchItem {
    let element: AXUIElement
    let depth: Int
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
    private var lastPlacement: HudPlacement?
    private var lastPlacementUpdate = Date.distantPast

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
        let stateChanged = command.state != view.state
        view.state = command.state
        view.level = command.level ?? 0

        if command.state == .hidden {
            lastPlacement = nil
            appendLog(command, placement: nil)
            window.orderOut(nil)
            return
        }

        let placement = placementForCurrentContext(force: stateChanged)
        appendLog(command, placement: placement)
        window.setFrame(placement.frame, display: true)
        window.orderFrontRegardless()
    }

    private func appendLog(_ command: HudCommand, placement: HudPlacement?) {
        guard let logPath else { return }
        let anchor = placement.map {
            " anchor=\($0.anchor.source.rawValue) precise=\($0.anchor.isPrecise) x=\(Int($0.anchor.rect.minX)) y=\(Int($0.anchor.rect.minY))"
        } ?? ""
        let line = "\(Date().timeIntervalSince1970) state=\(command.state.rawValue) level=\(command.level ?? 0)\(anchor)\n"
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

    private func placementForCurrentContext(force: Bool) -> HudPlacement {
        let now = Date()
        if !force,
           let lastPlacement,
           now.timeIntervalSince(lastPlacementUpdate) < 0.20 {
            return lastPlacement
        }

        let placement = positionedFrame()
        lastPlacement = placement
        lastPlacementUpdate = now
        return placement
    }

    private func positionedFrame() -> HudPlacement {
        let size = view.frame.size
        let visible = NSScreen.main?.visibleFrame ?? NSRect(x: 0, y: 0, width: 1440, height: 900)
        let anchor = bestAnchorFromAccessibility() ?? mouseFallbackAnchor(in: visible) ?? HudAnchor(
            rect: NSRect(x: visible.midX, y: visible.minY + 72, width: 1, height: 1),
            source: .screenFallback,
            isPrecise: false
        )

        let screen = NSScreen.screens.first { $0.visibleFrame.intersects(anchor.rect) } ?? NSScreen.main
        let frame = screen?.visibleFrame ?? visible
        let gap: CGFloat = 10

        var x: CGFloat
        var y: CGFloat
        if anchor.isPrecise {
            x = anchor.rect.maxX + gap
            y = anchor.rect.minY - size.height - 6
        } else {
            x = anchor.rect.maxX - size.width - 12
            y = anchor.rect.minY + 12
        }

        if x + size.width > frame.maxX - 8 {
            x = anchor.rect.minX - size.width - gap
        }
        if y < frame.minY + 8 {
            y = anchor.rect.maxY + 6
        }
        if x < frame.minX + 8 {
            x = frame.minX + 8
        }
        if y + size.height > frame.maxY - 8 {
            y = frame.maxY - size.height - 8
        }

        return HudPlacement(
            frame: NSRect(x: x, y: y, width: size.width, height: size.height),
            anchor: anchor
        )
    }
}

private func bestAnchorFromAccessibility() -> HudAnchor? {
    guard AXIsProcessTrusted() else { return nil }
    guard let focused = focusedAccessibilityElement() else { return nil }

    if let anchor = preciseCaretAnchor(focused) {
        return anchor
    }
    if let anchor = descendantPreciseCaretAnchor(focused) {
        return anchor
    }
    if let rect = focusedElementRect(focused), isUsefulFocusedElementFallback(rect) {
        return HudAnchor(rect: rect, source: .focusedElement, isPrecise: false)
    }

    return nil
}

private func preciseCaretAnchor(_ element: AXUIElement) -> HudAnchor? {
    if let rect = caretRectFromTextMarkerRange(element) {
        return HudAnchor(rect: rect, source: .selectedTextMarkerRange, isPrecise: true)
    }
    if let rect = caretRectFromSelectedTextRange(element) {
        return HudAnchor(rect: rect, source: .selectedTextRange, isPrecise: true)
    }
    if let rect = caretRectFromAdjacentCharacter(element) {
        return HudAnchor(rect: rect, source: .adjacentCharacterRange, isPrecise: true)
    }
    return nil
}

private func descendantPreciseCaretAnchor(_ root: AXUIElement) -> HudAnchor? {
    var queue = childElements(of: root).map { ElementSearchItem(element: $0, depth: 1) }
    var visited = 0
    let maxVisited = 80
    let maxDepth = 4

    while !queue.isEmpty && visited < maxVisited {
        let item = queue.removeFirst()
        visited += 1

        if let anchor = preciseCaretAnchor(item.element) {
            return anchor
        }
        if item.depth < maxDepth {
            queue.append(contentsOf: childElements(of: item.element).map {
                ElementSearchItem(element: $0, depth: item.depth + 1)
            })
        }
    }

    return nil
}

private func childElements(of element: AXUIElement) -> [AXUIElement] {
    var value: CFTypeRef?
    guard AXUIElementCopyAttributeValue(element, kAXChildrenAttribute as CFString, &value) == .success,
          let children = value as? [AXUIElement] else { return [] }
    return children
}

private func caretRectFromTextMarkerRange(_ element: AXUIElement) -> NSRect? {
    var markerRange: CFTypeRef?
    guard AXUIElementCopyAttributeValue(element, "AXSelectedTextMarkerRange" as CFString, &markerRange) == .success,
          let markerRange else { return nil }

    var boundsValue: CFTypeRef?
    guard AXUIElementCopyParameterizedAttributeValue(
        element,
        "AXBoundsForTextMarkerRange" as CFString,
        markerRange,
        &boundsValue
    ) == .success,
    let rect = cgRect(from: boundsValue),
    isUsableAccessibilityRect(rect) else { return nil }

    return normalizedCaretRect(accessibilityRectToAppKit(rect))
}

private func caretRectFromSelectedTextRange(_ element: AXUIElement) -> NSRect? {
    guard let range = selectedTextRange(element) else { return nil }
    var caretRange = range
    caretRange.length = 0

    guard let rect = boundsForCharacterRange(caretRange, in: element) else { return nil }
    return normalizedCaretRect(accessibilityRectToAppKit(rect))
}

private func caretRectFromAdjacentCharacter(_ element: AXUIElement) -> NSRect? {
    guard let range = selectedTextRange(element) else { return nil }
    guard range.location != kCFNotFound else { return nil }

    let characterCount = textCharacterCount(element)
    let probeLocation: CFIndex
    let useTrailingEdge: Bool
    if characterCount == 0 {
        return nil
    } else if range.location < characterCount {
        probeLocation = range.location
        useTrailingEdge = false
    } else if range.location > 0 {
        probeLocation = range.location - 1
        useTrailingEdge = true
    } else {
        return nil
    }

    guard let rect = boundsForCharacterRange(CFRange(location: probeLocation, length: 1), in: element) else {
        return nil
    }

    let appKitRect = accessibilityRectToAppKit(rect)
    let x = useTrailingEdge ? appKitRect.maxX : appKitRect.minX
    return NSRect(x: x, y: appKitRect.minY, width: 2, height: max(appKitRect.height, 16))
}

private func selectedTextRange(_ element: AXUIElement) -> CFRange? {
    var rangeValue: CFTypeRef?
    guard AXUIElementCopyAttributeValue(element, kAXSelectedTextRangeAttribute as CFString, &rangeValue) == .success,
          let rangeAXValue = axValue(from: rangeValue) else { return nil }

    var range = CFRange()
    guard AXValueGetValue(rangeAXValue, .cfRange, &range) else { return nil }
    return range
}

private func boundsForCharacterRange(_ range: CFRange, in element: AXUIElement) -> CGRect? {
    var range = range
    guard let axRange = AXValueCreate(.cfRange, &range) else { return nil }
    var boundsValue: CFTypeRef?
    guard AXUIElementCopyParameterizedAttributeValue(
        element,
        kAXBoundsForRangeParameterizedAttribute as CFString,
        axRange,
        &boundsValue
    ) == .success,
    let rect = cgRect(from: boundsValue),
    isUsableAccessibilityRect(rect) else { return nil }

    return rect
}

private func textCharacterCount(_ element: AXUIElement) -> CFIndex {
    var value: CFTypeRef?
    if AXUIElementCopyAttributeValue(element, kAXNumberOfCharactersAttribute as CFString, &value) == .success,
       let number = value as? NSNumber {
        return number.intValue
    }

    if AXUIElementCopyAttributeValue(element, kAXValueAttribute as CFString, &value) == .success,
       let string = value as? String {
        return string.utf16.count
    }

    return 0
}

private func focusedElementRect(_ element: AXUIElement) -> NSRect? {
    var positionValue: CFTypeRef?
    var sizeValue: CFTypeRef?
    guard AXUIElementCopyAttributeValue(element, kAXPositionAttribute as CFString, &positionValue) == .success,
          AXUIElementCopyAttributeValue(element, kAXSizeAttribute as CFString, &sizeValue) == .success,
          let positionAXValue = axValue(from: positionValue),
          let sizeAXValue = axValue(from: sizeValue) else { return nil }

    var position = CGPoint.zero
    var size = CGSize.zero
    guard AXValueGetValue(positionAXValue, .cgPoint, &position),
          AXValueGetValue(sizeAXValue, .cgSize, &size) else {
        return nil
    }

    let rect = CGRect(origin: position, size: size)
    guard isUsableElementRect(rect) else { return nil }

    return accessibilityRectToAppKit(rect)
}

private func isUsableAccessibilityRect(_ rect: CGRect) -> Bool {
    rect.origin.x.isFinite
        && rect.origin.y.isFinite
        && rect.size.width.isFinite
        && rect.size.height.isFinite
        && !rect.isNull
        && rect.width >= 0
        && rect.height > 0
}

private func isUsableElementRect(_ rect: CGRect) -> Bool {
    isUsableAccessibilityRect(rect) && rect.width > 0
}

private func normalizedCaretRect(_ rect: NSRect) -> NSRect {
    NSRect(x: rect.minX, y: rect.minY, width: max(rect.width, 2), height: max(rect.height, 16))
}

private func isUsefulFocusedElementFallback(_ rect: NSRect) -> Bool {
    rect.width <= 900 && rect.height <= 180
}

private func mouseFallbackAnchor(in visible: NSRect) -> HudAnchor? {
    let location = NSEvent.mouseLocation
    let screen = NSScreen.screens.first { $0.frame.contains(location) }
    let frame = screen?.visibleFrame ?? visible
    guard frame.contains(location) else { return nil }
    return HudAnchor(
        rect: NSRect(x: location.x, y: location.y, width: 1, height: 1),
        source: .mouseFallback,
        isPrecise: true
    )
}

private func focusedAccessibilityElement() -> AXUIElement? {
    let systemWide = AXUIElementCreateSystemWide()
    var focused: CFTypeRef?
    guard AXUIElementCopyAttributeValue(systemWide, kAXFocusedUIElementAttribute as CFString, &focused) == .success else {
        return nil
    }
    return focused as! AXUIElement?
}

private func axValue(from value: CFTypeRef?) -> AXValue? {
    guard let value, CFGetTypeID(value) == AXValueGetTypeID() else { return nil }
    return (value as! AXValue)
}

private func cgRect(from value: CFTypeRef?) -> CGRect? {
    guard let value = axValue(from: value) else { return nil }
    var rect = CGRect.zero
    guard AXValueGetValue(value, .cgRect, &rect) else { return nil }
    return rect
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
