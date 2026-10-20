import AppKit

/// Nightcap lives in the menu bar, and shows in the Dock only while a window of its own needs
/// finding: onboarding's, Settings', or an update's.
final class DockIcon {
    enum Reason {
        case onboarding, settings, update
    }

    private var reasons: Set<Reason> = []
    private var waiting: NSObjectProtocol?

    func show(for reason: Reason) {
        reasons.insert(reason)
        stopWaiting()
        NSApp.setActivationPolicy(.regular)
    }

    /// With no reason left, the icon goes at once while Nightcap isn't the active app, or has a
    /// focused window of its own to stay active with. Otherwise it waits for Nightcap to stop
    /// being active: dropping it then would make macOS hand the focus to another app, and move
    /// the user to whichever Space that app's windows are on.
    func hide(for reason: Reason) {
        reasons.remove(reason)
        guard reasons.isEmpty else { return }
        guard NSApp.isActive, NSApp.keyWindow == nil else {
            NSApp.setActivationPolicy(.accessory)
            return
        }
        guard waiting == nil else { return }
        waiting = NotificationCenter.default.addObserver(
            forName: NSApplication.didResignActiveNotification, object: NSApp, queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated {
                guard let self else { return }
                self.stopWaiting()
                if self.reasons.isEmpty { NSApp.setActivationPolicy(.accessory) }
            }
        }
    }

    private func stopWaiting() {
        guard let waiting else { return }
        NotificationCenter.default.removeObserver(waiting)
        self.waiting = nil
    }
}
