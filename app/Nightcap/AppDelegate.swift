import AppKit
import NightcapCore
import ServiceManagement
import Sparkle
import UserNotifications
import os

@main
final class AppDelegate: NSObject, NSApplicationDelegate {
    private let log = Logger(subsystem: Bundle.main.bundleIdentifier!, category: "app")
    private var core: Core?
    private var nightShift: NightShiftAdapter?
    private var terminationSignal: DispatchSourceSignal?
    private var wakeup: DispatchSourceTimer?
    private var updater: SPUStandardUpdaterController?
    private var browserWatcher: BrowserWatcher?
    private var statusMenu: StatusMenu?
    private var accessibility: AccessibilityPermission?
    private var onboarding: Onboarding?
    private let dockIcon = DockIcon()
    private lazy var updateReminders = UpdateReminders(dockIcon: dockIcon)
    private var observers: [NSObjectProtocol] = []

    static func main() {
        let app = NSApplication.shared
        let delegate = AppDelegate()
        app.delegate = delegate
        app.mainMenu = MainMenu.make()
        withExtendedLifetime(delegate) { app.run() }
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        quitOnTerminationSignal()
        let waker = MainThreadWaker { [weak self] in self?.tick() }
        let bundleFolder = Bundle.main.bundleURL.deletingLastPathComponent().path
        let core: Core
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
        self.core = core
        accessibility = AccessibilityPermission { [weak self] _ in
            self?.onboarding?.accessibilityChanged()
            Task { await self?.reportPermissions() }
        }
        browserWatcher = BrowserWatcher { [weak self] generation, host in
            self?.reportHost(generation: generation, host: host)
        }
        // The context first, so a force still in place after a crash holds without a flash.
        reportFrontmostApp(NSWorkspace.shared.frontmostApplication)
        startNightShift()
        apply(core.handle(event: .launched))
        // The menu-bar item appears once onboarding is done.
        if onboarding == nil { showStatusMenu() }
        watchPermissions()
        watchSystem()
    }

    /// Clicking the Dock icon brings onboarding forward.
    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
        guard let onboarding else { return true }
        onboarding.present()
        return false
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
        // Fast user switching: while another user is in front, the core writes nothing and
        // ignores status callbacks, then waits for a fresh read once this session is back.
        observers.append(NSWorkspace.shared.notificationCenter.addObserver(
            forName: NSWorkspace.sessionDidResignActiveNotification, object: nil, queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated {
                guard let self, let core = self.core else { return }
                self.apply(core.handle(event: .sessionChanged(active: false)))
            }
        })
        observers.append(NSWorkspace.shared.notificationCenter.addObserver(
            forName: NSWorkspace.sessionDidBecomeActiveNotification, object: nil, queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated {
                guard let self, let core = self.core else { return }
                self.reportSunTimes()
                self.apply(core.handle(event: .sessionChanged(active: true)))
                if let status = self.nightShift?.status {
                    self.reportNightShift(status)
                }
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
        let bundleId = app?.bundleIdentifier
        let isBrowser = bundleId.map(BrowserWatcher.browsers.contains) ?? false
        apply(core.handle(event: .appActivated(
            bundleId: bundleId, name: app?.localizedName, isBrowser: isBrowser)))
    }

    /// The host is logged, as private, only when it changed Night Shift (ADR-0012).
    private func reportHost(generation: UInt64, host: String?) {
        guard let core else { return }
        let effects = core.handle(event: .hostChanged(generation: generation, host: host))
        if effects.contains(where: { if case .setNightShift = $0 { true } else { false } }) {
            log.info("the browser's host \(host ?? "(unknown)", privacy: .private) changed Night Shift")
        }
        apply(effects)
    }

    private func watchBrowser(bundleId: String, generation: UInt64) {
        guard let pid = NSRunningApplication.runningApplications(withBundleIdentifier: bundleId)
            .first?.processIdentifier
        else {
            reportHost(generation: generation, host: nil)
            return
        }
        browserWatcher?.watch(pid: pid, generation: generation)
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

    /// Onboarding asks for notifications itself; without it, they're asked for at launch.
    private func watchPermissions() {
        Task {
            // Once now, so status is right while the notification prompt waits for an answer.
            await reportPermissions()
            guard onboarding == nil else { return }
            do {
                _ = try await UNUserNotificationCenter.current()
                    .requestAuthorization(options: [.alert, .sound])
            } catch {
                log.error("couldn't request notification permission: \(error, privacy: .public)")
            }
            await reportPermissions()
        }
    }

    private func showOnboarding() {
        guard let accessibility, let core else { return }
        let onboarding = Onboarding(
            accessibility: accessibility,
            onNotificationsRead: { [weak self] in
                Task { await self?.reportPermissions() }
            },
            onFinish: { [weak self] websites in
                guard let self else { return }
                apply(core.handle(event: .onboardingFinished(websites: websites)))
                self.onboarding = nil
                dockIcon.hide(for: .onboarding)
                showStatusMenu()
            })
        self.onboarding = onboarding
        dockIcon.show(for: .onboarding)
        onboarding.present()
    }

    /// Sparkle starts with the launch's effects, so by now it runs if it ever will.
    private func showStatusMenu() {
        guard let core, let accessibility else { return }
        let checkForUpdates: (() -> Void)? = updater == nil ? nil : { [weak self] in self?.checkForUpdates() }
        statusMenu = StatusMenu(
            current: { core.menu() },
            choose: { [weak self] command in
                self?.apply(core.handle(event: .menuItemChosen(command: command)))
            },
            askForAccessibility: { accessibility.ask() },
            checkForUpdates: checkForUpdates)
    }

    private func reportPermissions() async {
        let settings = await UNUserNotificationCenter.current().notificationSettings()
        let notifications: NotificationPermission =
            switch settings.authorizationStatus {
            case .notDetermined: .notRequested
            case .denied: .denied
            default: .granted
            }
        guard let core, let accessibility else { return }
        apply(core.handle(event: .permissionsChanged(
            accessibility: accessibility.granted, notifications: notifications)))
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
            case .startUpdater:
                updater = SPUStandardUpdaterController(
                    startingUpdater: true, updaterDelegate: nil, userDriverDelegate: updateReminders)
            case .checkForUpdates:
                checkForUpdates()
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
            // Version 1 is the only onboarding so far, so `completed` is always 0.
            case .showOnboarding:
                showOnboarding()
            case .askForAccessibility:
                accessibility?.ask()
            case .watchBrowser(let bundleId, let generation):
                watchBrowser(bundleId: bundleId, generation: generation)
            case .stopWatching:
                browserWatcher?.stop()
            }
        }
        // Whatever changed the core's state changed the icon too, CLI commands included.
        statusMenu?.refresh()
    }

    private func checkForUpdates() {
        guard let updater else {
            log.error("the updater isn't running; dropped a check for updates")
            return
        }
        // A menu-bar-only app has to come forward for Sparkle's window to show in front.
        NSApp.activate()
        updater.checkForUpdates(nil)
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
