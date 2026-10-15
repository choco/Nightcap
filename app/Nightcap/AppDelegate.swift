import AppKit
import ApplicationServices
import NightcapCore
import ServiceManagement
import UserNotifications
import os

@main
final class AppDelegate: NSObject, NSApplicationDelegate {
    private let log = Logger(subsystem: Bundle.main.bundleIdentifier!, category: "app")
    private var core: Core?
    private var terminationSignal: DispatchSourceSignal?
    private var accessibilityObserver: NSObjectProtocol?

    static func main() {
        let app = NSApplication.shared
        let delegate = AppDelegate()
        app.delegate = delegate
        withExtendedLifetime(delegate) { app.run() }
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        quitOnTerminationSignal()
        let waker = MainThreadWaker { [weak self] in self?.tick() }
        let bundleFolder = Bundle.main.bundleURL.deletingLastPathComponent().path
        do {
            core = try Core(
                bundleId: Bundle.main.bundleIdentifier!,
                bundleWritable: FileManager.default.isWritableFile(atPath: bundleFolder),
                waker: waker
            )
        } catch CoreError.AlreadyRunning(let path) {
            log.notice("another Nightcap is already running at \(path, privacy: .public); quitting")
            NSApp.terminate(nil)
            return
        } catch {
            log.fault("the core couldn't start: \(String(describing: error), privacy: .public)")
            NSApp.terminate(nil)
            return
        }
        log.info("Nightcap started")
        apply(core!.handle(event: .launched))
        watchPermissions()
    }

    func applicationWillTerminate(_ notification: Notification) {
        guard let core else { return }
        apply(core.handle(event: .terminating))
    }

    /// launchd stops the app with SIGTERM, whose default action would skip
    /// applicationWillTerminate and leave any force in place (ADR-0011).
    private func quitOnTerminationSignal() {
        signal(SIGTERM, SIG_IGN)
        let source = DispatchSource.makeSignalSource(signal: SIGTERM, queue: .main)
        source.setEventHandler { NSApp.terminate(nil) }
        source.resume()
        terminationSignal = source
    }

    private func watchPermissions() {
        Task {
            // Once now, so status is right while the notification prompt waits for an answer.
            await reportPermissions()
            do {
                _ = try await UNUserNotificationCenter.current()
                    .requestAuthorization(options: [.alert, .sound])
            } catch {
                log.error("couldn't request notification permission: \(error, privacy: .public)")
            }
            await reportPermissions()
        }
        // An undocumented notification macOS posts when the Accessibility list changes.
        accessibilityObserver = DistributedNotificationCenter.default().addObserver(
            forName: Notification.Name("com.apple.accessibility.api"), object: nil, queue: .main
        ) { [weak self] _ in
            Task { @MainActor in await self?.reportPermissions() }
        }
    }

    private func reportPermissions() async {
        let settings = await UNUserNotificationCenter.current().notificationSettings()
        let notifications: NotificationPermission =
            switch settings.authorizationStatus {
            case .notDetermined: .notRequested
            case .denied: .denied
            default: .granted
            }
        guard let core else { return }
        apply(core.handle(event: .permissionsChanged(
            accessibility: AXIsProcessTrusted(), notifications: notifications)))
    }

    private func tick() {
        guard let core else { return }
        apply(core.tick())
    }

    private func apply(_ effects: [Effect]) {
        for effect in effects {
            switch effect {
            case .registerLoginItem:
                registerLoginItem()
            case .setNightShift(let enabled):
                log.error("there's no Night Shift adapter yet; dropped setNightShift(enabled: \(enabled))")
            }
        }
    }

    /// The core asks once per install; whatever state the item is in, an item that exists is
    /// reported back so the core never asks again.
    private func registerLoginItem() {
        let service = SMAppService.mainApp
        switch service.status {
        case .enabled, .requiresApproval:
            log.info("the login item already exists")
        default:
            // A never-registered app reports .notFound, the same as one whose item was removed.
            do {
                try service.register()
                log.info("registered the login item")
            } catch {
                log.error("couldn't register the login item: \(error, privacy: .public)")
                return
            }
        }
        guard let core else { return }
        apply(core.handle(event: .loginItemRegistered))
    }
}
