import Foundation
import KeyboardShortcuts
import os

extension KeyboardShortcuts.Name {
    /// Nightcap's one global shortcut (ADR-0021), kept in its user defaults.
    static let nightShift = Self("nightShift", initial: .init(.n, modifiers: [.control, .option, .command]))
}

/// The shortcut tells a tap from a hold by how long the key stays down: the press starts a peek
/// at once, a quick release is a tap, which turns Night Shift on or off, and a later one ends the
/// peek (ADR-0021).
final class Shortcut {
    private static let tap: TimeInterval = 0.3
    private let log = Logger(subsystem: Bundle.main.bundleIdentifier!, category: "shortcut")
    private let onPress: () -> Void
    private let onTap: () -> Void
    private let onRelease: () -> Void
    private var pressed: Date?

    /// `press` runs as the key goes down, then either `tap` or `release` as it comes up.
    init(press: @escaping () -> Void, tap: @escaping () -> Void, release: @escaping () -> Void) {
        onPress = press
        onTap = tap
        onRelease = release
        KeyboardShortcuts.onKeyDown(for: .nightShift) { [weak self] in self?.press() }
        KeyboardShortcuts.onKeyUp(for: .nightShift) { [weak self] in self?.release() }
    }

    /// The key going down, on the keyboard or on the tour's keycaps.
    func press() {
        guard pressed == nil else { return }
        pressed = Date()
        onPress()
    }

    func release() {
        guard let pressed else { return }
        self.pressed = nil
        let held = Date().timeIntervalSince(pressed)
        log.info("shortcut \(held < Self.tap ? "tapped" : "held", privacy: .public) for \(held, format: .fixed(precision: 2), privacy: .public) s")
        if held < Self.tap { onTap() } else { onRelease() }
    }
}
