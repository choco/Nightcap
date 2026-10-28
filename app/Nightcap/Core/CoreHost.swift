import AppKit
import NightcapCore
import UserNotifications
import os

/// Between macOS and the core (ADR-0002). It starts the core, reports what macOS observes as
/// facts, and sends the UI's commands. The executor carries out the outcome of every fact and
/// command, the CLI's too. No sender looks at an outcome.
final class CoreHost: NSObject {
    private let log = Logger(subsystem: Bundle.main.bundleIdentifier!, category: "app")
    /// The core, whose reads answer from the snapshot it published after its last input.
    let core: Core
    private let inbox: Inbox
    private let accessibilityChanged: () -> Void
    private let notificationsChanged: () -> Void
    private let showOnboarding: () -> Void
    private(set) var nightShift: NightShiftAdapter?
    private(set) var accessibility: AccessibilityPermission?
    private let loginItem = LoginItemAdapter()
    private let updater: Updater
    private var executor: Executor?
    /// Re-reads the permissions every second while onboarding or Settings is open, and every 30
    /// seconds otherwise: macOS posts nothing when notifications are switched in System Settings.
    private var permissionsPoll: Timer?
    /// What the core last heard, so an unchanged re-read sends nothing.
    private var reportedPermissions: (accessibility: Bool, notifications: NotificationPermission)?
    private var reportedLoginItem: LoginItem?
    private var reportedStrength: UInt8?
    private var observers: [NSObjectProtocol] = []

    /// Starts the core, which throws `CoreError.AlreadyRunning` while another Nightcap serves this
    /// instance's socket. `notifications` posts Sparkle's reminders; `accessibilityChanged` runs
    /// as the grant flips, `notificationsChanged` as a read finds notifications changed, and
    /// `showOnboarding` when the core asks for onboarding.
    init(
        dockIcon: DockIcon,
        notifications: Notifications,
        accessibilityChanged: @escaping () -> Void,
        notificationsChanged: @escaping () -> Void,
        showOnboarding: @escaping () -> Void
    ) throws {
        let inbox = Inbox()
        let bundleFolder = Bundle.main.bundleURL.deletingLastPathComponent().path
        core = try Core.start(
            bundleId: Bundle.main.bundleIdentifier!,
            bundleWritable: FileManager.default.isWritableFile(atPath: bundleFolder),
            timeZone: TimeZone.current.identifier,
            listener: inbox)
        self.inbox = inbox
        self.accessibilityChanged = accessibilityChanged
        self.notificationsChanged = notificationsChanged
        self.showOnboarding = showOnboarding
        updater = Updater(dockIcon: dockIcon, notifications: notifications)
        super.init()
        let accessibility = AccessibilityPermission { [weak self] _ in
            self?.accessibilityChanged()
            Task { await self?.reportSystemState() }
        }
        self.accessibility = accessibility
        let browser = BrowserWatch { [weak self] generation, host in
            self?.reportHost(generation: generation, host: host)
        }
        nightShift = NightShiftAdapter { [weak self] status in self?.reportNightShift(status) }
        executor = Executor(
            core: core, inbox: inbox, nightShift: nightShift, loginItem: loginItem, updater: updater,
            wakeups: WakeupTimer { [weak self] in self?.wakeupCameDue() }, browser: browser,
            accessibility: accessibility, notifications: self, systemSettings: SystemSettings(), rereading: self,
            showOnboarding: showOnboarding)
    }

    /// Whether Sparkle runs, which the launch's outcome decides.
    var updaterRunning: Bool { updater.running }

    /// Notifications as the core last heard of them; not asked for until the first read, as the
    /// core starts.
    var notificationPermission: NotificationPermission { reportedPermissions?.notifications ?? .notRequested }

    // MARK: Subscriptions

    func onEvent(_ handler: @escaping (Event) -> Void) {
        executor?.onEvent(handler)
    }

    func onChange(_ handler: @escaping () -> Void) {
        executor?.onChange(handler)
    }

    func onNextChange(_ handler: @escaping () -> Void) {
        executor?.onNextChange(handler)
    }

    // MARK: Commands in

    /// A command from the menu, Settings, onboarding or the shortcut, the same as the CLI's.
    func send(_ command: Command) {
        core.send(command: command)
    }

    /// A command only the app has.
    func send(_ command: AppCommand) {
        core.sendApp(command: command)
    }

    func perform(_ fix: AppFix) {
        executor?.perform(fix)
    }

    func stop() {
        executor?.stop()
    }

    // MARK: Facts in

    /// The launch's facts, then the launch itself, whose outcome can open onboarding.
    func launch() {
        // The context first, so a force still in place after a crash holds without a flash.
        reportFrontmostApp(NSWorkspace.shared.frontmostApplication)
        // Sun times first, so the core can place the schedule the moment it learns it.
        reportSunTimes()
        if let status = nightShift?.status {
            reportNightShift(status)
            reportStrength()
            nightShift?.repairPeek()
        } else {
            log.fault("Night Shift is unavailable")
        }
        // Onboarding, which the launch can open, asks about opening at login only where Nightcap
        // opens itself, and starts from where the login item stands.
        reportLoginItem()
        // Every 30 seconds from here; onboarding, which the launch can open, has the permissions
        // read every second.
        pollPermissions(every: 30)
        core.report(fact: .launched)
        // Once now, so status is right while the notification prompt waits for an answer.
        Task { await reportSystemState() }
        watchSystem()
    }

    /// `closely` while onboarding or Settings, which show the permissions, is open: they're read
    /// at once, then every second, and every 30 seconds otherwise.
    func watchPermissions(closely: Bool) {
        if closely { Task { await reportSystemState() } }
        pollPermissions(every: closely ? 1 : 30)
    }

    /// The macOS prompt, which shows only while notifications were never asked for, then a read
    /// of the answer.
    func askForNotifications() {
        Task {
            do {
                _ = try await UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound])
            } catch {
                log.error("couldn't request notification permission: \(error, privacy: .public)")
            }
            await reportSystemState()
        }
    }

    /// The login item, the strength and the permissions, each reported only when it changed:
    /// macOS posts nothing when the user switches the login item or notifications in System
    /// Settings. Notifications are read here alone, for the core and onboarding both.
    func reportSystemState() async {
        reportLoginItem()
        reportStrength()
        let settings = await UNUserNotificationCenter.current().notificationSettings()
        let read: NotificationPermission =
            switch settings.authorizationStatus {
            case .notDetermined: .notRequested
            case .denied: .denied
            default: .granted
            }
        guard let accessibility else { return }
        let permissions = (accessibility: accessibility.granted, notifications: read)
        if let reportedPermissions, reportedPermissions == permissions { return }
        let notificationsDiffer = read != notificationPermission
        reportedPermissions = permissions
        core.report(
            fact: .permissionsChanged(
                accessibility: permissions.accessibility, notifications: permissions.notifications))
        if notificationsDiffer { notificationsChanged() }
    }

    private func reportNightShift(_ status: NightShiftAdapter.Status) {
        core.report(
            fact: .statusChanged(
                enabled: status.enabled, schedule: status.schedule, sunScheduleSetAside: status.sunScheduleSetAside))
    }

    /// CoreBrightness announces no strength change, so the strength is read whenever something
    /// may be about to show it: at launch, with every re-read of the permissions (the menu
    /// opening, Settings or onboarding open, an app switch, and every 30 seconds otherwise), and
    /// after each saved write. A read costs under a millisecond.
    private func reportStrength() {
        guard let strength = nightShift?.strength, strength != reportedStrength else { return }
        reportedStrength = strength
        core.report(fact: .strengthChanged(value: strength))
    }

    /// CoreBrightness reports sun times only a day either side of now, so they're re-read
    /// whenever time may have moved on: at launch, on waking and before each wakeup.
    private func reportSunTimes() {
        guard let sunTimes = nightShift?.sunTimes else { return }
        core.report(fact: .sunTimesChanged(sunrises: sunTimes.sunrises, sunsets: sunTimes.sunsets))
    }

    private func reportFrontmostApp(_ app: NSRunningApplication?) {
        let bundleId = app?.bundleIdentifier
        let isBrowser = bundleId.map(BrowserWatcher.browsers.contains) ?? false
        core.report(fact: .appActivated(bundleId: bundleId, isBrowser: isBrowser))
    }

    private func reportHost(generation: UInt64, host: String?) {
        core.report(fact: .hostChanged(generation: generation, host: host))
    }

    /// The sun times, then the wakeup itself, as one comes due.
    private func wakeupCameDue() {
        reportSunTimes()
        core.report(fact: .timerFired)
    }

    private func reportLoginItem() {
        let item = loginItem.item
        guard item != reportedLoginItem else { return }
        reportedLoginItem = item
        core.report(fact: .loginItemChanged(item: item))
    }

    private func watchSystem() {
        observers.append(
            NSWorkspace.shared.notificationCenter.addObserver(
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
        observers.append(
            NSWorkspace.shared.notificationCenter.addObserver(
                forName: NSWorkspace.didWakeNotification, object: nil, queue: .main
            ) { [weak self] _ in
                MainActor.assumeIsolated {
                    self?.reportSunTimes()
                    self?.core.report(fact: .woke)
                }
            })
        // Fast user switching: while another user is in front, the core writes nothing and
        // ignores status callbacks, then waits for a fresh read once this session is back.
        observers.append(
            NSWorkspace.shared.notificationCenter.addObserver(
                forName: NSWorkspace.sessionDidResignActiveNotification, object: nil, queue: .main
            ) { [weak self] _ in
                MainActor.assumeIsolated {
                    self?.core.report(fact: .sessionChanged(active: false))
                }
            })
        observers.append(
            NSWorkspace.shared.notificationCenter.addObserver(
                forName: NSWorkspace.sessionDidBecomeActiveNotification, object: nil, queue: .main
            ) { [weak self] _ in
                MainActor.assumeIsolated {
                    guard let self else { return }
                    self.reportSunTimes()
                    self.core.report(fact: .sessionChanged(active: true))
                    if let status = self.nightShift?.status {
                        self.reportNightShift(status)
                    }
                }
            })
        observers.append(
            NotificationCenter.default.addObserver(
                forName: .NSSystemTimeZoneDidChange, object: nil, queue: .main
            ) { [weak self] _ in
                MainActor.assumeIsolated {
                    // TimeZone.current caches the zone it first read.
                    NSTimeZone.resetSystemTimeZone()
                    self?.core.report(fact: .timeZoneChanged(ianaId: TimeZone.current.identifier))
                }
            })
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
}

extension CoreHost: Rereading {
    func rereadLoginItem() {
        reportLoginItem()
    }

    func rereadStrength() {
        reportStrength()
    }
}

extension CoreHost: NotificationsAsking {
    /// The macOS prompt while notifications were never asked for; it never shows again after
    /// that, so then Nightcap's page in Notification settings.
    func allowNotifications() {
        Task {
            await reportSystemState()
            guard notificationPermission == .notRequested else {
                SystemSettings().open(.notifications)
                return
            }
            askForNotifications()
        }
    }
}
