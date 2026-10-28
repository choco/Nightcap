import AppKit
import NightcapCore
import ServiceManagement
import Sparkle
import Synchronization
import UserNotifications
import os

/// Between macOS and the core (ADR-0002). It starts the core, reports what macOS observes as
/// facts, and sends the UI's commands. It's the core's one listener: the outcome of every fact
/// and command, the CLI's too, arrives on the core's thread and is carried out here on the main
/// queue, in the order the core decided them, its effects by the adapters that drive macOS and its
/// events by whoever subscribed. No sender looks at an outcome.
final class CoreHost: NSObject {
    private let log = Logger(subsystem: Bundle.main.bundleIdentifier!, category: "app")
    /// The core, whose reads answer from the snapshot it published after its last input.
    let core: Core
    private let inbox: Inbox
    private let dockIcon: DockIcon
    private let notifications: Notifications
    private let accessibilityChanged: () -> Void
    private let notificationsChanged: () -> Void
    private let showOnboarding: () -> Void
    private var eventHandlers: [(Event) -> Void] = []
    private var changeHandlers: [() -> Void] = []
    /// Run once, after the next outcome is carried out.
    private var nextChangeHandlers: [() -> Void] = []
    private(set) var nightShift: NightShiftAdapter?
    private(set) var accessibility: AccessibilityPermission?
    private var browserWatcher: BrowserWatcher?
    /// The watched browser's host as last read, which is logged only as Night Shift changes
    /// (ADR-0009).
    private var watchedHost: String?
    private var wakeup: DispatchSourceTimer?
    private var updater: SPUStandardUpdaterController?
    private lazy var updateReminders = UpdateReminders(dockIcon: dockIcon) { [notifications] title, body, id in
        notifications.post(Notice(title: title, body: body), id: id)
    }
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
        self.dockIcon = dockIcon
        self.notifications = notifications
        self.accessibilityChanged = accessibilityChanged
        self.notificationsChanged = notificationsChanged
        self.showOnboarding = showOnboarding
        super.init()
        // An outcome that arrives before this waits in the inbox for the main queue, which this
        // holds until it returns.
        inbox.receive = { [weak self] outcome in self?.carryOut(outcome) }
        accessibility = AccessibilityPermission { [weak self] _ in
            self?.accessibilityChanged()
            Task { await self?.reportSystemState() }
        }
        browserWatcher = BrowserWatcher { [weak self] generation, host in
            self?.reportHost(generation: generation, host: host)
        }
        nightShift = NightShiftAdapter { [weak self] status in self?.reportNightShift(status) }
    }

    /// Nightcap's page in System Settings › Notifications.
    static let notificationSettings = URL(
        string: "x-apple.systempreferences:com.apple.Notifications-Settings.extension"
            + "?id=\(Bundle.main.bundleIdentifier!)")!

    /// Whether Sparkle runs, which the launch's outcome decides.
    var updaterRunning: Bool { updater != nil }

    /// Notifications as the core last heard of them; not asked for until the first read, as the
    /// core starts.
    var notificationPermission: NotificationPermission { reportedPermissions?.notifications ?? .notRequested }

    /// The login item as the core last heard of it, which Settings' "Open at login" mirrors
    /// (ADR-0011); nil until the first read, as the core starts.
    var loginItem: LoginItem? { reportedLoginItem }

    // MARK: Subscriptions

    /// Calls `handler` with each event the core tells, as its outcome is carried out.
    func onEvent(_ handler: @escaping (Event) -> Void) {
        eventHandlers.append(handler)
    }

    /// Calls `handler` once each outcome is carried out, so what shows the core's state redraws.
    func onChange(_ handler: @escaping () -> Void) {
        changeHandlers.append(handler)
    }

    /// Calls `handler` once, after the next outcome is carried out: the one a command sent now
    /// leads to, unless another input was already on its way.
    func onNextChange(_ handler: @escaping () -> Void) {
        nextChangeHandlers.append(handler)
    }

    // MARK: Commands in

    /// A command from the menu, Settings, onboarding or the shortcut, the same as the CLI's.
    func send(_ command: Command) {
        core.send(command: command)
    }

    /// A command only the UI has.
    func send(_ command: UiCommand) {
        core.sendUi(command: command)
    }

    /// The shell carries out its own fixes and hands the core's to it, each named, so a new fix
    /// has to be given to one or the other.
    func perform(_ fix: Fix) {
        switch fix {
        case .askForAccessibility:
            accessibility?.ask()
        case .allowNotifications:
            allowNotifications()
        case .openLoginItems:
            SMAppService.openSystemSettingsLoginItems()
        case .openLocationServices:
            let pane = "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_LocationServices"
            NSWorkspace.shared.open(URL(string: pane)!)
        case .stopCheckingWebsites, .keepNotificationsOff, .keepScheduleOff, .openAtLogin, .dontOpenAtLogin, .reload:
            send(.fix(fix))
        }
    }

    /// Releases every force as the app quits (ADR-0008). The core decides what came before, then
    /// what releases them, and both are carried out here, in order, before the app exits.
    func stop() {
        let released = core.stop()
        inbox.handOver()
        carryOut(released)
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

    func checkForUpdates() {
        guard let updater else {
            log.error("the updater isn't running; dropped a check for updates")
            return
        }
        // A menu-bar-only app has to come forward for Sparkle's window to show in front.
        NSApp.activate()
        updater.checkForUpdates(nil)
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
        watchedHost = host
        core.report(fact: .hostChanged(generation: generation, host: host))
    }

    private func reportLoginItem() {
        // A never-registered app reports .notFound, the same as one whose item was removed.
        let item: LoginItem =
            switch SMAppService.mainApp.status {
            case .enabled: .enabled
            case .requiresApproval: .requiresApproval
            default: .notRegistered
            }
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

    // MARK: Outcomes out

    /// Carries out an outcome: each effect by the adapter it's for, in the order the core gave
    /// them, then each event to its subscribers, then one redraw. No effect needs the core to
    /// have heard what the one before it changed.
    private func carryOut(_ outcome: Outcome) {
        outcome.effects.forEach(carryOut)
        for event in outcome.events {
            eventHandlers.forEach { $0(event) }
        }
        changeHandlers.forEach { $0() }
        let next = nextChangeHandlers
        nextChangeHandlers = []
        next.forEach { $0() }
    }

    private func carryOut(_ effect: Effect) {
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
                return
            }
            if let watchedHost {
                log.info("Night Shift turns \(enabled ? "on" : "off") on \(watchedHost, privacy: .private)")
            }
            nightShift.setEnabled(enabled)
        case .setSchedule(let schedule):
            guard let nightShift else {
                log.error("Night Shift is unavailable; dropped setSchedule")
                return
            }
            nightShift.setSchedule(schedule)
        case .setStrength(let value, let commit):
            guard let nightShift else {
                log.error("Night Shift is unavailable; dropped setStrength(value: \(value))")
                return
            }
            nightShift.setStrength(value, commit: commit)
            // A preview isn't the strength, so only a saved one is read back.
            if commit { reportStrength() }
        case .scheduleWakeup(let at):
            scheduleWakeup(at: at)
        case .cancelWakeup:
            wakeup?.cancel()
            wakeup = nil
        // Version 1 is the only onboarding so far, so `completed` is always 0.
        case .showOnboarding:
            showOnboarding()
        case .askForAccessibility:
            accessibility?.ask()
        case .watchBrowser(let bundleId, let generation):
            watchedHost = nil
            watchBrowser(bundleId: bundleId, generation: generation)
        case .stopWatching:
            watchedHost = nil
            browserWatcher?.stop()
        }
    }

    /// Registers or unregisters the login item, as the user chose (ADR-0011), then reports
    /// where that left it.
    private func changeLoginItem(_ change: (SMAppService) throws -> Void) {
        do {
            try change(SMAppService.mainApp)
        } catch {
            log.error("couldn't change the login item: \(error, privacy: .public)")
        }
        reportLoginItem()
    }

    private func watchBrowser(bundleId: String, generation: UInt64) {
        guard
            let pid = NSRunningApplication.runningApplications(withBundleIdentifier: bundleId)
                .first?.processIdentifier
        else {
            reportHost(generation: generation, host: nil)
            return
        }
        browserWatcher?.watch(pid: pid, generation: generation)
    }

    /// A wall-clock timer, so it fires on time after sleep or a clock change (ADR-0002).
    private func scheduleWakeup(at date: Date) {
        wakeup?.cancel()
        let seconds = date.timeIntervalSince1970.rounded(.down)
        let deadline = DispatchWallTime(
            timespec: timespec(
                tv_sec: Int(seconds),
                tv_nsec: Int((date.timeIntervalSince1970 - seconds) * 1_000_000_000)
            ))
        let timer = DispatchSource.makeTimerSource(queue: .main)
        timer.schedule(wallDeadline: deadline, leeway: .seconds(1))
        timer.setEventHandler { [weak self] in
            guard let self else { return }
            wakeup?.cancel()
            wakeup = nil
            reportSunTimes()
            core.report(fact: .timerFired)
        }
        timer.resume()
        wakeup = timer
    }

    /// The macOS prompt while notifications were never asked for; it never shows again after
    /// that, so then Nightcap's page in Notification settings.
    private func allowNotifications() {
        Task {
            await reportSystemState()
            guard notificationPermission == .notRequested else {
                NSWorkspace.shared.open(Self.notificationSettings)
                return
            }
            askForNotifications()
        }
    }
}

/// The core's one listener (ADR-0002). It takes each outcome on the core's thread and hands it to
/// `receive` on the main queue, in the order the core told them; the main queue keeps that order.
// A conformer to a Sendable protocol isn't inferred to the main actor, so it says so.
@MainActor private final class Inbox: Listener {
    var receive: (Outcome) -> Void = { _ in }
    private let arrived = Mutex<[Outcome]>([])

    nonisolated func outcome(outcome: Outcome) {
        arrived.withLock { $0.append(outcome) }
        DispatchQueue.main.async { [self] in
            MainActor.assumeIsolated { handOver() }
        }
    }

    /// Hands over every outcome that arrived, in order. Quitting takes them at once, ahead of the
    /// main queue's turn.
    func handOver() {
        let outcomes = arrived.withLock { arrived in
            defer { arrived.removeAll() }
            return arrived
        }
        outcomes.forEach(receive)
    }
}
