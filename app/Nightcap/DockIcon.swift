import AppKit

/// Nightcap lives in the menu bar, and shows in the Dock only while a window of its own needs
/// finding: onboarding's, Settings', or an update's.
final class DockIcon {
    enum Reason {
        case onboarding, settings, update
    }

    private var reasons: Set<Reason> = []

    func show(for reason: Reason) {
        reasons.insert(reason)
        NSApp.setActivationPolicy(.regular)
    }

    func hide(for reason: Reason) {
        reasons.remove(reason)
        if reasons.isEmpty { NSApp.setActivationPolicy(.accessory) }
    }
}
