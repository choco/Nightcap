import AppKit
import NightcapCore
import ServiceManagement
import Sparkle
import UserNotifications
import os

/// Between macOS and the core, on the main thread (ADR-0002): what macOS reports, read and handed
/// to the core as events, and the effects the core answers with, carried out. Every event reaches
/// the core through here, the UIs' commands too, so every effect is carried out here.
final class Mac: NSObject {
    private let log = Logger(subsystem: Bundle.main.bundleIdentifier!, category: "app")
    private let core: Core
    private let dockIcon: DockIcon
    private let accessibilityChanged: () -> Void
    private let notificationsChanged: () -> Void
    private let showOnboarding: () -> Void
    private let changed: () -> Void
    private(set) var nightShift: NightShiftAdapter?
    private(set) var accessibility: AccessibilityPermission?
    private var browserWatcher: BrowserWatcher?
    private var wakeup: DispatchSourceTimer?
    private var updater: SPUStandardUpdaterController?
    private lazy var updateReminders = UpdateReminders(dockIcon: dockIcon) { [weak self] title, body, id in
        self?.notify(title: title, body: body, opens: nil, id: id)
    }
    /// Re-reads the permissions every second while onboarding or Settings is open, and every 30
    /// seconds otherwise: macOS posts nothing when notifications are switched in System Settings.
    private var permissionsPoll: Timer?
    /// What the core last heard, so an unchanged re-read sends nothing.
    private var reportedPermissions: (accessibility: Bool, notifications: NotificationPermission)?
    private var reportedLoginItem: LoginItem?
    private var reportedStrength: UInt8?
    private var observers: [NSObjectProtocol] = []
    /// Calls into the core waiting for the effects before them to go out.
    private var waiting: [() -> [Effect]] = []
    private var carryingOut = false

    /// `accessibilityChanged` runs as the grant flips; `notificationsChanged` as a read finds
    /// notifications changed; `showOnboarding` when the core asks for onboarding; and `changed`
    /// once a call into the core and the calls its effects led to are all carried out.
    init(
        core: Core,
        dockIcon: DockIcon,
        accessibilityChanged: @escaping () -> Void,
        notificationsChanged: @escaping () -> Void,
        showOnboarding: @escaping () -> Void,
        changed: @escaping () -> Void
    ) {
        self.core = core
        self.dockIcon = dockIcon
        self.accessibilityChanged = accessibilityChanged
        self.notificationsChanged = notificationsChanged
        self.showOnboarding = showOnboarding
        self.changed = changed
        super.init()
        UNUserNotificationCenter.current().delegate = self
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

    /// Whether Sparkle runs, which the launch's effects decide.
    var updaterRunning: Bool { updater != nil }

    /// Notifications as the core last heard of them; not asked for until the first read, as the
    /// core starts.
    var notifications: NotificationPermission { reportedPermissions?.notifications ?? .notRequested }

    /// The launch's facts, then the launch itself, whose effects can open onboarding.
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
        handle(.launched)
        // Once now, so status is right while the notification prompt waits for an answer.
        Task { await reportSystemState() }
        watchSystem()
    }

    /// Hands `event` to the core and carries out the effects it answers with.
    func handle(_ event: Event) {
        run { [core] in core.handle(event: event) }
    }

    /// The core's own threads have work for it, such as a CLI request (ADR-0002).
    func tick() {
        reportStrength()
        run { [core] in core.tick() }
    }

    /// The shell carries out its own fixes and hands the core's back to it, each named, so a new
    /// fix has to be given to one or the other.
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
            handle(.fixChosen(fix: fix))
        }
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
        let notificationsDiffer = read != notifications
        reportedPermissions = permissions
        handle(
            .permissionsChanged(accessibility: permissions.accessibility, notifications: permissions.notifications))
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

    // MARK: Facts in

    private func reportNightShift(_ status: NightShiftAdapter.Status) {
        handle(
            .statusChanged(
                enabled: status.enabled, schedule: status.schedule, sunScheduleSetAside: status.sunScheduleSetAside))
    }

    /// CoreBrightness announces no strength change, so the strength is read whenever something
    /// may be about to show it: at launch, with every re-read of the permissions (the menu
    /// opening, Settings or onboarding open, an app switch), before each tick, which answers the
    /// CLI, and after each saved write. A read costs under a millisecond.
    private func reportStrength() {
        guard let strength = nightShift?.strength, strength != reportedStrength else { return }
        reportedStrength = strength
        handle(.strengthChanged(value: strength))
    }

    /// CoreBrightness reports sun times only a day either side of now, so they're re-read
    /// whenever time may have moved on: at launch, on waking and before each wakeup.
    private func reportSunTimes() {
        guard let sunTimes = nightShift?.sunTimes else { return }
        handle(.sunTimesChanged(sunrises: sunTimes.sunrises, sunsets: sunTimes.sunsets))
    }

    private func reportFrontmostApp(_ app: NSRunningApplication?) {
        let bundleId = app?.bundleIdentifier
        let isBrowser = bundleId.map(BrowserWatcher.browsers.contains) ?? false
        handle(.appActivated(bundleId: bundleId, name: app?.localizedName, isBrowser: isBrowser))
    }

    /// The host is logged, as private, only when it changed Night Shift (ADR-0009).
    private func reportHost(generation: UInt64, host: String?) {
        run { [core, log] in
            let effects = core.handle(event: .hostChanged(generation: generation, host: host))
            if effects.contains(where: { if case .setNightShift = $0 { true } else { false } }) {
                log.info("the browser's host \(host ?? "(unknown)", privacy: .private) changed Night Shift")
            }
            return effects
        }
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
        handle(.loginItemChanged(item: item))
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
                    self?.handle(.woke)
                }
            })
        // Fast user switching: while another user is in front, the core writes nothing and
        // ignores status callbacks, then waits for a fresh read once this session is back.
        observers.append(
            NSWorkspace.shared.notificationCenter.addObserver(
                forName: NSWorkspace.sessionDidResignActiveNotification, object: nil, queue: .main
            ) { [weak self] _ in
                MainActor.assumeIsolated {
                    self?.handle(.sessionChanged(active: false))
                }
            })
        observers.append(
            NSWorkspace.shared.notificationCenter.addObserver(
                forName: NSWorkspace.sessionDidBecomeActiveNotification, object: nil, queue: .main
            ) { [weak self] _ in
                MainActor.assumeIsolated {
                    guard let self else { return }
                    self.reportSunTimes()
                    self.handle(.sessionChanged(active: true))
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
                    self?.handle(.timeZoneChanged(ianaId: TimeZone.current.identifier))
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

    // MARK: Effects out

    /// Calls the core and carries out the effects it answers with. A call made meanwhile, as when
    /// an effect reads back what it changed, waits until they're all out, so effects go out in the
    /// order the core gave them and the UIs refresh once, after the last. No effect needs the core
    /// to have heard what the one before it changed.
    private func run(_ call: @escaping () -> [Effect]) {
        waiting.append(call)
        guard !carryingOut else { return }
        carryingOut = true
        while !waiting.isEmpty {
            carryOut(waiting.removeFirst()())
        }
        carryingOut = false
        changed()
    }

    private func carryOut(_ effects: [Effect]) {
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
            case .notify(let title, let body, let opens):
                notify(title: title, body: body, opens: opens)
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

    /// A wall-clock timer, so it fires on time after sleep or a clock change.
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
            handle(.timerFired)
        }
        timer.resume()
        wakeup = timer
    }

    /// Asks for permission first if it hasn't been asked yet, so a notice sent at launch isn't
    /// dropped while the prompt waits for an answer. A notification posted under an `id` replaces
    /// any earlier one under it.
    private func notify(title: String, body: String, opens: String?, id: String = UUID().uuidString) {
        let content = UNMutableNotificationContent()
        content.title = title
        content.body = body
        if let opens { content.userInfo = [pageKey: opens] }
        let request = UNNotificationRequest(identifier: id, content: content, trigger: nil)
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

    /// The macOS prompt while notifications were never asked for; it never shows again after
    /// that, so then Nightcap's page in Notification settings.
    private func allowNotifications() {
        Task {
            await reportSystemState()
            guard notifications == .notRequested else {
                NSWorkspace.shared.open(Self.notificationSettings)
                return
            }
            askForNotifications()
        }
    }
}

/// Where a notification keeps the page it opens when clicked.
nonisolated private let pageKey = "opens"

extension Mac: UNUserNotificationCenterDelegate {
    nonisolated func userNotificationCenter(
        _ center: UNUserNotificationCenter, didReceive response: UNNotificationResponse
    ) async {
        guard let page = response.notification.request.content.userInfo[pageKey] as? String,
            let url = URL(string: page)
        else { return }
        await MainActor.run { _ = NSWorkspace.shared.open(url) }
    }
}
