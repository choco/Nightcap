import AppKit
import KeyboardShortcuts
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
    private var settingsWindow: SettingsWindow?
    /// Re-reads the permissions every second while Settings is open, and every 30 seconds
    /// otherwise: macOS posts nothing when notifications are switched in System Settings.
    private var permissionsPoll: Timer?
    private var accessibility: AccessibilityPermission?
    private var onboarding: Onboarding?
    private var shortcut: Shortcut?
    private var tour: Tour?
    /// What the core last heard, so an unchanged re-read sends nothing.
    private var reportedPermissions: (accessibility: Bool, notifications: NotificationPermission)?
    private var reportedLoginItem: LoginItem?
    private var reportedStrength: UInt8?
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
        #if DEBUG
        Snapshot.runIfAsked()
        #endif
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
            Task { await self?.reportSystemState() }
        }
        browserWatcher = BrowserWatcher { [weak self] generation, host in
            self?.reportHost(generation: generation, host: host)
        }
        // The context first, so a force still in place after a crash holds without a flash.
        reportFrontmostApp(NSWorkspace.shared.frontmostApplication)
        startNightShift()
        // Onboarding, which the launch can open, asks about opening at login only where Nightcap
        // opens itself, and starts from where the login item stands.
        reportLoginItem()
        apply(core.handle(event: .launched))
        // The menu-bar item appears once onboarding is done.
        if onboarding == nil { showStatusMenu() }
        listenForShortcut()
        watchPermissions()
        watchSystem()
    }

    /// Clicking the Dock icon brings onboarding forward; opening Nightcap again once it's done
    /// opens Settings.
    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
        if let onboarding {
            onboarding.present()
        } else {
            showSettings()
        }
        return false
    }

    /// The app menu's "Settings…".
    @objc func openSettings(_ sender: Any?) {
        showSettings()
    }

    func applicationWillTerminate(_ notification: Notification) {
        nightShift?.endPeek()
        tour?.end()
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
        reportStrength()
        nightShift?.repairPeek()
    }

    /// The press peeks at once, so a tap that turns Night Shift off shows no flash of warmth. A
    /// tap that leaves it on, as when CoreBrightness refuses the switch, ends the peek after
    /// (ADR-0021).
    private func listenForShortcut() {
        shortcut = Shortcut(
            press: { [weak self] in
                self?.nightShift?.peek()
                self?.tour?.shortcutPressed()
            },
            tap: { [weak self] in
                guard let self, let core else { return }
                apply(core.handle(event: .commandChosen(command: .toggle)))
                nightShift?.endPeek()
                tour?.shortcutTapped()
            },
            release: { [weak self] in
                self?.nightShift?.endPeek()
                self?.tour?.shortcutReleased()
            })
    }

    /// From onboarding's end or Settings, once their window is gone, as the click that started the
    /// tour ends: the panel then keeps the focus. `onShown` runs once it's up.
    private func startTour(onShown: () -> Void = {}) {
        if let tour {
            tour.reveal()
            return
        }
        guard let core else { return }
        let tour = Tour(
            standsIn: nightShift?.status?.enabled == false,
            shortcut: KeyboardShortcuts.getShortcut(for: .nightShift),
            browser: tourBrowser(core.settings()),
            press: { [weak self] in self?.shortcut?.press() },
            release: { [weak self] in self?.shortcut?.release() },
            begin: { [weak self] in self?.apply(core.handle(event: .tourStarted)) },
            onEnd: { [weak self] begun in
                guard let self else { return }
                self.tour = nil
                if begun { apply(core.handle(event: .tourEnded)) }
            })
        self.tour = tour
        tour.start()
        onShown()
    }

    /// The default browser, for the tour's site step, while website checking can watch it.
    private func tourBrowser(_ settings: Settings) -> Tour.Browser? {
        guard settings.websiteChecking.on, accessibility?.granted == true,
              let url = NSWorkspace.shared.urlForApplication(toOpen: URL(string: "https://example.com")!),
              let bundleId = Bundle(url: url)?.bundleIdentifier, BrowserWatcher.browsers.contains(bundleId)
        else { return nil }
        return Tour.Browser(bundleId: bundleId, name: FileManager.default.displayName(atPath: url.path))
    }

    private func reportNightShift(_ status: NightShiftAdapter.Status) {
        guard let core else { return }
        apply(core.handle(event: .statusChanged(enabled: status.enabled, schedule: status.schedule)))
    }

    /// CoreBrightness announces no strength change, so the strength is read whenever something
    /// may be about to show it: at launch, with every re-read of the permissions (the menu
    /// opening, Settings or onboarding open, an app switch), before each tick, which answers the
    /// CLI, and after each saved write. A read costs under a millisecond.
    private func reportStrength() {
        guard let core, let strength = nightShift?.strength, strength != reportedStrength else { return }
        reportedStrength = strength
        apply(core.handle(event: .strengthChanged(value: strength)))
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
            MainActor.assumeIsolated {
                self?.reportFrontmostApp(app)
                // macOS posts nothing when notifications are switched in System Settings, so
                // they're re-read whenever the user moves on to another app.
                Task { await self?.reportSystemState() }
            }
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
        tour?.frontmostChanged(bundleId)
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
        pollPermissions(every: 30)
        Task {
            // Once now, so status is right while the notification prompt waits for an answer.
            await reportSystemState()
            guard onboarding == nil else { return }
            do {
                _ = try await UNUserNotificationCenter.current()
                    .requestAuthorization(options: [.alert, .sound])
            } catch {
                log.error("couldn't request notification permission: \(error, privacy: .public)")
            }
            await reportSystemState()
        }
    }

    private func showOnboarding() {
        guard let accessibility, let core else { return }
        let onboarding = Onboarding(
            steps: core.onboarding(),
            rules: core.settings().rules,
            sun: nightShift?.sunTimes.flatMap { times in
                guard let sunset = times.sunsets.first, let sunrise = times.sunrises.first else { return nil }
                return Sun(sunset: sunset, sunrise: sunrise)
            },
            accessibilityGranted: { accessibility.granted },
            askForAccessibility: { accessibility.ask() },
            choose: { [weak self] command in
                self?.apply(core.handle(event: .commandChosen(command: command)))
            },
            onNotificationsRead: { [weak self] in
                Task { await self?.reportSystemState() }
            },
            onFinish: { [weak self] websites, tour in
                guard let self else { return }
                apply(core.handle(event: .onboardingFinished(websites: websites)))
                self.onboarding = nil
                showStatusMenu()
                // The Dock icon goes once the popover or the tour's panel is up, while Nightcap
                // stays active with it.
                let hideDockIcon: () -> Void = { [weak self] in self?.dockIcon.hide(for: .onboarding) }
                guard tour else {
                    statusMenu?.pointOut(onShown: hideDockIcon)
                    return
                }
                DispatchQueue.main.async { [weak self] in self?.startTour(onShown: hideDockIcon) }
            })
        self.onboarding = onboarding
        dockIcon.show(for: .onboarding)
        onboarding.present()
    }

    /// Sparkle starts with the launch's effects, so by now it runs if it ever will.
    private func showStatusMenu() {
        guard let core else { return }
        let checkForUpdates: (() -> Void)? = updater == nil ? nil : { [weak self] in self?.checkForUpdates() }
        statusMenu = StatusMenu(
            current: { core.menu() },
            choose: { [weak self] command in
                self?.apply(core.handle(event: .commandChosen(command: command)))
                self?.tour?.chose(command)
            },
            openSettings: { [weak self] onProblem in
                self?.showSettings(onProblem ? SettingsWindow.tab(for: core.settings()) : nil)
            },
            onOpen: { [weak self] in
                self?.tour?.menuOpened()
                Task { await self?.reportSystemState() }
            },
            onClose: { [weak self] in self?.tour?.menuClosed() })
        settingsWindow = SettingsWindow(
            current: { core.settings() },
            choose: { [weak self] command in
                self?.apply(core.handle(event: .commandChosen(command: command)))
            },
            perform: { [weak self] fix in self?.perform(fix) },
            checkForUpdates: checkForUpdates,
            export: { core.export(format: $0) },
            takeTour: { [weak self] in
                guard let self else { return }
                settingsWindow?.close()
                DispatchQueue.main.async { [weak self] in
                    self?.startTour { [weak self] in self?.dockIcon.hide(for: .settings) }
                }
            },
            onClose: { [weak self] in
                guard let self else { return }
                dockIcon.hide(for: .settings)
                pollPermissions(every: 30)
            })
    }

    /// On `tab`, or on the tab last shown.
    private func showSettings(_ tab: SettingsWindow.Tab? = nil) {
        guard let settingsWindow else { return }
        dockIcon.show(for: .settings)
        settingsWindow.show(tab)
        Task { await reportSystemState() }
        pollPermissions(every: 1)
    }

    private func pollPermissions(every interval: TimeInterval) {
        permissionsPoll?.invalidate()
        let timer = Timer(timeInterval: interval, repeats: true) { [weak self] _ in
            MainActor.assumeIsolated {
                _ = Task { await self?.reportSystemState() }
            }
        }
        // Lets macOS fold the background poll into other wakeups.
        timer.tolerance = interval / 4
        RunLoop.main.add(timer, forMode: .common)
        permissionsPoll = timer
    }

    /// The shell carries out its own fixes and hands the rest to the core.
    private func perform(_ fix: Fix) {
        switch fix {
        case .askForAccessibility:
            accessibility?.ask()
        case .allowNotifications:
            allowNotifications()
        case .openLoginItems:
            SMAppService.openSystemSettingsLoginItems()
        default:
            guard let core else { return }
            apply(core.handle(event: .fixChosen(fix: fix)))
        }
    }

    /// The macOS prompt while notifications were never asked for; it never shows again after
    /// that, so then Nightcap's page in Notification settings.
    private func allowNotifications() {
        Task {
            let center = UNUserNotificationCenter.current()
            guard await center.notificationSettings().authorizationStatus == .notDetermined else {
                let pane = "x-apple.systempreferences:com.apple.Notifications-Settings.extension?id=\(Bundle.main.bundleIdentifier!)"
                NSWorkspace.shared.open(URL(string: pane)!)
                return
            }
            do {
                _ = try await center.requestAuthorization(options: [.alert, .sound])
            } catch {
                log.error("couldn't request notification permission: \(error, privacy: .public)")
            }
            await reportSystemState()
        }
    }

    /// The login item and the permissions, each reported only when it changed: macOS posts
    /// nothing when the user switches either in System Settings.
    private func reportSystemState() async {
        reportLoginItem()
        reportStrength()
        let settings = await UNUserNotificationCenter.current().notificationSettings()
        let notifications: NotificationPermission =
            switch settings.authorizationStatus {
            case .notDetermined: .notRequested
            case .denied: .denied
            default: .granted
            }
        guard let core, let accessibility else { return }
        let permissions = (accessibility: accessibility.granted, notifications: notifications)
        if let reportedPermissions, reportedPermissions == permissions { return }
        reportedPermissions = permissions
        apply(core.handle(event: .permissionsChanged(
            accessibility: permissions.accessibility, notifications: permissions.notifications)))
    }

    private func tick() {
        guard let core else { return }
        reportStrength()
        apply(core.tick())
    }

    private func apply(_ effects: [Effect]) {
        for effect in effects {
            switch effect {
            case .registerLoginItem:
                changeLoginItem { try $0.register() }
            case .unregisterLoginItem:
                changeLoginItem { try $0.unregister() }
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
            case .setSchedule(let schedule):
                guard let nightShift else {
                    log.error("Night Shift is unavailable; dropped setSchedule")
                    continue
                }
                nightShift.setSchedule(schedule)
            case .setStrength(let value, let commit):
                guard let nightShift else {
                    log.error("Night Shift is unavailable; dropped setStrength(value: \(value))")
                    continue
                }
                nightShift.setStrength(value, commit: commit)
                // A preview isn't the strength, so only a saved one is read back.
                if commit { reportStrength() }
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
        settingsWindow?.refresh()
        if let tour, let core { tour.forcedChanged(core.menu().icon == .forcedOff) }
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

    /// Registers or unregisters the login item, as the user chose (ADR-0015), then reports
    /// where that left it.
    private func changeLoginItem(_ change: (SMAppService) throws -> Void) {
        do {
            try change(SMAppService.mainApp)
        } catch {
            log.error("couldn't change the login item: \(error, privacy: .public)")
        }
        reportLoginItem()
    }

    private func reportLoginItem() {
        guard let core else { return }
        // A never-registered app reports .notFound, the same as one whose item was removed.
        let item: LoginItem =
            switch SMAppService.mainApp.status {
            case .enabled: .enabled
            case .requiresApproval: .requiresApproval
            default: .notRegistered
            }
        guard item != reportedLoginItem else { return }
        reportedLoginItem = item
        apply(core.handle(event: .loginItemChanged(item: item)))
    }
}
