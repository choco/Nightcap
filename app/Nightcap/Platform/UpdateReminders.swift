import AppKit
import Sparkle

/// Sparkle's gentle reminders for a menu-bar-only app: while an update session runs, Nightcap
/// shows in the Dock so its update window can be found, and a scheduled update also posts a
/// notification, since its window opens behind whatever the user is doing.
final class UpdateReminders: NSObject, SPUStandardUserDriverDelegate {
    private let dockIcon: DockIcon
    private let post: (Notice, _ id: String) -> Void

    /// `post` posts a notice under `id`, replacing any earlier one under it.
    init(dockIcon: DockIcon, post: @escaping (Notice, _ id: String) -> Void) {
        self.dockIcon = dockIcon
        self.post = post
    }

    var supportsGentleScheduledUpdateReminders: Bool { true }

    func standardUserDriverWillHandleShowingUpdate(
        _ handleShowingUpdate: Bool, forUpdate update: SUAppcastItem, state: SPUUserUpdateState
    ) {
        dockIcon.show(for: .update)
        guard !state.userInitiated else { return }
        let ready = Wording.system.updateReady(update.displayVersionString)
        post(Notice(title: ready.title, body: ready.body), "update")
    }

    func standardUserDriverWillFinishUpdateSession() {
        dockIcon.hide(for: .update)
    }
}
