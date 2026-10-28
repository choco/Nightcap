import AppKit
import Sparkle
import os

/// Sparkle, which updates a self-managed install (ADR-0011), with its gentle reminders.
final class Updater: Updating {
    private let log = Logger(subsystem: Bundle.main.bundleIdentifier!, category: "app")
    /// Sparkle holds its user driver's delegate weakly.
    private let reminders: UpdateReminders
    private var controller: SPUStandardUpdaterController?

    /// `notifications` posts Sparkle's reminders.
    init(dockIcon: DockIcon, notifications: Notifications) {
        reminders = UpdateReminders(dockIcon: dockIcon) { title, body, id in
            notifications.post(Notice(title: title, body: body), id: id)
        }
    }

    /// Whether Sparkle runs, which the launch's outcome decides.
    var running: Bool { controller != nil }

    func start() {
        controller = SPUStandardUpdaterController(
            startingUpdater: true, updaterDelegate: nil, userDriverDelegate: reminders)
    }

    func checkForUpdates() {
        guard let controller else {
            log.error("the updater isn't running; dropped a check for updates")
            return
        }
        // A menu-bar-only app has to come forward for Sparkle's window to show in front.
        NSApp.activate()
        controller.checkForUpdates(nil)
    }
}
