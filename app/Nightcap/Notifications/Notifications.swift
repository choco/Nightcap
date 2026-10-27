import AppKit
import NightcapCore
import UserNotifications
import os

/// Nightcap's notifications: what the core tells, as notices, and Sparkle's reminders. A click on
/// one opens the page its notice names.
final class Notifications: NSObject {
    private let log = Logger(subsystem: Bundle.main.bundleIdentifier!, category: "app")

    override init() {
        super.init()
        UNUserNotificationCenter.current().delegate = self
    }

    /// Tells the user of an event, with a notification where it calls for one. A refusal is only
    /// logged, by its code: the user is looking at what they chose, and its message can name a
    /// domain.
    func tell(_ event: Event) {
        if case .refused(_, let why) = event {
            log.notice("the core refused a command: \(String(describing: why.code), privacy: .public)")
            return
        }
        let front = NSWorkspace.shared.frontmostApplication.flatMap { app in
            app.bundleIdentifier.map { Notice.FrontApp(bundleId: $0, name: app.localizedName) }
        }
        guard let notice = Notice(event, frontApp: front, words: .system) else { return }
        post(notice)
    }

    /// Asks for permission first if it hasn't been asked yet, so a notice sent at launch isn't
    /// dropped while the prompt waits for an answer. A notice posted under an `id` replaces any
    /// earlier one under it.
    func post(_ notice: Notice, id: String = UUID().uuidString) {
        let content = UNMutableNotificationContent()
        content.title = notice.title
        content.body = notice.body
        if let opens = notice.opens { content.userInfo = [pageKey: opens.absoluteString] }
        let request = UNNotificationRequest(identifier: id, content: content, trigger: nil)
        Task {
            let center = UNUserNotificationCenter.current()
            do {
                guard try await center.requestAuthorization(options: [.alert, .sound]) else {
                    log.notice("notifications are off; dropped \"\(notice.title, privacy: .public)\"")
                    return
                }
                try await center.add(request)
            } catch {
                log.error("couldn't post \"\(notice.title, privacy: .public)\": \(error, privacy: .public)")
            }
        }
    }
}

/// Where a notification keeps the page it opens when clicked.
nonisolated private let pageKey = "opens"

extension Notifications: UNUserNotificationCenterDelegate {
    nonisolated func userNotificationCenter(
        _ center: UNUserNotificationCenter, didReceive response: UNNotificationResponse
    ) async {
        guard let page = response.notification.request.content.userInfo[pageKey] as? String,
            let url = URL(string: page)
        else { return }
        await MainActor.run { _ = NSWorkspace.shared.open(url) }
    }
}
