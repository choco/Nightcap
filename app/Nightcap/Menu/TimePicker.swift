import AppKit
import NightcapCore

/// Custom…: asks when a change should end, in a small floating window under the moon, and sends
/// `Set` until then (ADR-0018). A time typed into a menu fights the menu's own keys. An earlier
/// time than now means tomorrow.
final class TimePicker: NSObject, NSWindowDelegate {
    private static let padding: CGFloat = 16
    private let panel = FloatingPanel()
    private let picker = NSDatePicker()
    private let hint: NSTextField
    private let place: Place
    private let state: OnOff
    private let words: Wording
    private let choose: (Command) -> Void
    private let onClose: () -> Void

    init(
        title: String, place: Place, state: OnOff, start: Date, words: Wording,
        choose: @escaping (Command) -> Void, onClose: @escaping () -> Void
    ) {
        self.place = place
        self.state = state
        self.words = words
        self.choose = choose
        self.onClose = onClose
        hint = NSTextField.secondary(Self.hintText(until: Self.next(start), words: words), width: 200)
        super.init()
        picker.datePickerStyle = .textFieldAndStepper
        picker.datePickerElements = .hourMinute
        picker.dateValue = start
        picker.setAccessibilityLabel(words.untilTime)
        picker.onAction { [weak self] in self?.updateHint() }
        let heading = NSTextField.label(title, size: 13, weight: .semibold, color: .labelColor, width: 220)
        let until = NSStackView(views: [NSTextField(labelWithString: words.untilTime), picker])
        until.spacing = 8
        let cancel = CapsuleButton(words.cancel, .regular) { [weak self] in self?.close() }
        cancel.keyEquivalent = "\u{1b}"
        let done = CapsuleButton(words.turn(state), .primary) { [weak self] in self?.done() }
        done.keyEquivalent = "\r"
        let buttons = NSStackView(views: [NSView(), cancel, done])
        let stack = NSStackView(views: [heading, until, hint, buttons])
        stack.orientation = .vertical
        stack.alignment = .leading
        stack.spacing = 6
        stack.setCustomSpacing(12, after: until)
        stack.setCustomSpacing(14, after: hint)
        stack.edgeInsets = NSEdgeInsets(
            top: 14, left: Self.padding, bottom: 14, right: Self.padding)
        buttons.widthAnchor.constraint(equalTo: stack.widthAnchor, constant: -2 * Self.padding).isActive = true
        stack.translatesAutoresizingMaskIntoConstraints = false
        panel.glass.addSubview(stack)
        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(equalTo: panel.glass.leadingAnchor),
            stack.topAnchor.constraint(equalTo: panel.glass.topAnchor),
        ])
        panel.setContentSize(stack.fittingSize)
        panel.delegate = self
    }

    /// At the top right, under the menu bar, where the moon's menu was.
    func show() {
        if let screen = NSScreen.main?.visibleFrame {
            panel.setFrameOrigin(
                NSPoint(
                    x: screen.maxX - panel.frame.width - 12, y: screen.maxY - panel.frame.height - 8))
        }
        panel.makeKeyAndOrderFront(nil)
        panel.makeFirstResponder(picker)
    }

    /// Clicking anywhere else puts it away, as a menu would be.
    func windowDidResignKey(_ notification: Notification) {
        close()
    }

    private var end: Date { Self.next(picker.dateValue) }

    /// The next moment the clock reads `time`'s hour and minute: today, or tomorrow once it has
    /// passed.
    private static func next(_ time: Date) -> Date {
        let calendar = Calendar.current
        let components = calendar.dateComponents([.hour, .minute], from: time)
        let now = Date()
        return calendar.nextDate(after: now, matching: components, matchingPolicy: .nextTime) ?? now
    }

    /// "In 1 hour, 15 minutes", to the minute, rounded up.
    private static func hintText(until end: Date, words: Wording) -> String {
        words.inTime((end.timeIntervalSinceNow / 60).rounded(.up) * 60)
    }

    private func updateHint() {
        hint.attributedStringValue = NSAttributedString(
            string: Self.hintText(until: end, words: words),
            attributes: hint.attributedStringValue.attributes(at: 0, effectiveRange: nil))
    }

    private func done() {
        choose(.set(place: place, state: state, length: .until(end)))
        close()
    }

    private func close() {
        panel.delegate = nil
        panel.close()
        onClose()
    }
}
