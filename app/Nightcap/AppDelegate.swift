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
    private var nightShift: NightShiftAdapter?
    private var terminationSignal: DispatchSourceSignal?
    private var wakeup: DispatchSourceTimer?
    private var observers: [NSObjectProtocol] = []

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
                timeZone: TimeZone.current.identifier,
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
        // The context first, so a force still in place after a crash holds without a flash.
        reportFrontmostApp(NSWorkspace.shared.frontmostApplication)
        startNightShift()
        apply(core!.handle(event: .launched))
        watchPermissions()
        watchSystem()
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

    private func startNightShift() {
        nightShift = NightShiftAdapter { [weak self] status in self?.reportNightShift(status) }
        // Sun times first, so the core can place the schedule the moment it learns it.
        reportSunTimes()
        guard let status = nightShift?.status else {
            log.fault("Night Shift is unavailable")
            return
        }
        reportNightShift(status)
    }

    private func reportNightShift(_ status: NightShiftAdapter.Status) {
        guard let core else { return }
        apply(core.handle(event: .statusChanged(enabled: status.enabled, schedule: status.schedule)))
    }

    /// CoreBrightness reports sun times only a day either side of now, so they're re-read
    /// whenever time may have moved on: at launch, on waking and before each wakeup.
    private func reportSunTimes() {
        guard let core, let sunTimes = nightShift?.sunTimes else { return }
        apply(core.handle(event: .sunTimesChanged(sunrises: sunTimes.sunrises, sunsets: sunTimes.sunsets)))
    }

    private func watchSystem() {
        observers.append(NSWorkspace.shared.notificationCenter.addObserver(
            forName: NSWorkspace.didActivateApplicationNotification, object: nil, queue: .main
        ) { [weak self] notification in
            let app = notification.userInfo?[NSWorkspace.applicationUserInfoKey] as? NSRunningApplication
            MainActor.assumeIsolated { self?.reportFrontmostApp(app) }
        })
        observers.append(NSWorkspace.shared.notificationCenter.addObserver(
            forName: NSWorkspace.didWakeNotification, object: nil, queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated {
                guard let self, let core = self.core else { return }
                self.reportSunTimes()
                self.apply(core.handle(event: .woke))
            }
        })
        observers.append(NotificationCenter.default.addObserver(
            forName: .NSSystemTimeZoneDidChange, object: nil, queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated {
                guard let self, let core = self.core else { return }
                // TimeZone.current caches the zone it first read.
                NSTimeZone.resetSystemTimeZone()
                self.apply(core.handle(event: .timeZoneChanged(ianaId: TimeZone.current.identifier)))
            }
        })
    }

    private func reportFrontmostApp(_ app: NSRunningApplication?) {
        guard let core else { return }
        apply(core.handle(event: .appActivated(bundleId: app?.bundleIdentifier)))
    }

    /// Asks for permission first if it hasn't been asked yet, so a notice sent at launch isn't
    /// dropped while the prompt waits for an answer.
    private func notify(title: String, body: String) {
        let content = UNMutableNotificationContent()
        content.title = title
        content.body = body
        let request = UNNotificationRequest(identifier: UUID().uuidString, content: content, trigger: nil)
        Task {
            let center = UNUserNotificationCenter.current()
            do {
                guard try await center.requestAuthorization(options: [.alert, .sound]) else {
                    log.notice("notifications are off; dropped \"\(title, privacy: .public)\"")
                    return
                }
                try await center.add(request)
            } catch {
                log.error("couldn't post \"\(title, privacy: .public)\": \(error, privacy: .public)")
            }
        }
    }

    /// A wall-clock timer, so it fires on time after sleep or a clock change.
    private func scheduleWakeup(at date: Date) {
        wakeup?.cancel()
        let seconds = date.timeIntervalSince1970.rounded(.down)
        let deadline = DispatchWallTime(timespec: timespec(
            tv_sec: Int(seconds),
            tv_nsec: Int((date.timeIntervalSince1970 - seconds) * 1_000_000_000)
        ))
        let timer = DispatchSource.makeTimerSource(queue: .main)
        timer.schedule(wallDeadline: deadline, leeway: .seconds(1))
        timer.setEventHandler { [weak self] in
            guard let self, let core = self.core else { return }
            self.wakeup?.cancel()
            self.wakeup = nil
            self.reportSunTimes()
            self.apply(core.handle(event: .timerFired))
        }
        timer.resume()
        wakeup = timer
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
        observers.append(DistributedNotificationCenter.default().addObserver(
            forName: Notification.Name("com.apple.accessibility.api"), object: nil, queue: .main
        ) { [weak self] _ in
            Task { @MainActor in await self?.reportPermissions() }
        })
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
                guard let nightShift else {
                    log.error("Night Shift is unavailable; dropped setNightShift(enabled: \(enabled))")
                    continue
                }
                nightShift.setEnabled(enabled)
            case .scheduleWakeup(let at):
                scheduleWakeup(at: at)
            case .cancelWakeup:
                wakeup?.cancel()
                wakeup = nil
            case .notify(let title, let body):
                notify(title: title, body: body)
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
