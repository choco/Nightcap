import AppKit
import Sparkle
import UserNotifications

/// Sparkle's gentle reminders for a menu-bar-only app: while an update session runs, Nightcap
/// shows in the Dock so its update window can be found, and a scheduled update also posts a
/// notification, since its window opens behind whatever the user is doing.
final class UpdateReminders: NSObject, SPUStandardUserDriverDelegate {
    var supportsGentleScheduledUpdateReminders: Bool { true }

    func standardUserDriverWillHandleShowingUpdate(
        _ handleShowingUpdate: Bool, forUpdate update: SUAppcastItem, state: SPUUserUpdateState
    ) {
        NSApp.setActivationPolicy(.regular)
        guard !state.userInitiated else { return }
        let content = UNMutableNotificationContent()
        content.title = "Nightcap \(update.displayVersionString) is ready to install"
        content.body = "Its update window is open behind your other windows."
        UNUserNotificationCenter.current().add(
            UNNotificationRequest(identifier: "update", content: content, trigger: nil))
    }

    func standardUserDriverWillFinishUpdateSession() {
        NSApp.setActivationPolicy(.accessory)
    }
}
