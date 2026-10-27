import AppKit
import Sparkle

/// Sparkle's gentle reminders for a menu-bar-only app: while an update session runs, Nightcap
/// shows in the Dock so its update window can be found, and a scheduled update also posts a
/// notification, since its window opens behind whatever the user is doing.
final class UpdateReminders: NSObject, SPUStandardUserDriverDelegate {
    private let dockIcon: DockIcon
    private let notify: (_ title: String, _ body: String, _ id: String) -> Void

    /// `notify` posts a notification under `id`, replacing any earlier one under it.
    init(dockIcon: DockIcon, notify: @escaping (_ title: String, _ body: String, _ id: String) -> Void) {
        self.dockIcon = dockIcon
        self.notify = notify
    }

    var supportsGentleScheduledUpdateReminders: Bool { true }

    func standardUserDriverWillHandleShowingUpdate(
        _ handleShowingUpdate: Bool, forUpdate update: SUAppcastItem, state: SPUUserUpdateState
    ) {
        dockIcon.show(for: .update)
        guard !state.userInitiated else { return }
        notify(
            "Nightcap \(update.displayVersionString) is ready to install",
            "Its update window is open behind your other windows.", "update")
    }

    func standardUserDriverWillFinishUpdateSession() {
        dockIcon.hide(for: .update)
    }
}
