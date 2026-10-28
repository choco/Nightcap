import Foundation
import NightcapCore
import os

/// Carries out what the core decides (ADR-0002). The inbox hands it each outcome, with the
/// snapshot its input left, on the main queue, in the order the core decided them: the outcome's
/// effects go to the adapters that drive macOS, in order, then its events to whoever subscribed,
/// then the UI redraws once, from that snapshot. No effect needs the core to have heard what the
/// one before it changed. The app's own fixes, which never reach the core, are carried out here
/// too.
final class Executor {
    private let log = Logger(subsystem: Bundle.main.bundleIdentifier!, category: "app")
    private let core: Core
    private let inbox: Inbox
    private let nightShift: NightShiftWriting?
    private let loginItem: LoginItemRegistering
    private let updater: Updating
    private let wakeups: WakeupScheduling
    private let browser: BrowserWatching
    private let accessibility: AccessibilityAsking
    private let notifications: NotificationsAsking
    private let systemSettings: SystemSettingsOpening
    private let rereading: Rereading
    private let showOnboarding: () -> Void
    private var eventHandlers: [(Event) -> Void] = []
    private var changeHandlers: [(Cause, Snapshot) -> Void] = []
    /// Run once, after the next outcome is carried out.
    private var nextChangeHandlers: [() -> Void] = []

    /// Carries out what `inbox`, the listener `core` tells, hands over. An outcome that arrives
    /// before this waits there for the main queue's next turn, so the executor is made in the turn
    /// that starts the core. `nightShift` is nil while Night Shift is unavailable, whose effects
    /// are then dropped.
    init(
        core: Core,
        inbox: Inbox,
        nightShift: NightShiftWriting?,
        loginItem: LoginItemRegistering,
        updater: Updating,
        wakeups: WakeupScheduling,
        browser: BrowserWatching,
        accessibility: AccessibilityAsking,
        notifications: NotificationsAsking,
        systemSettings: SystemSettingsOpening,
        rereading: Rereading,
        showOnboarding: @escaping () -> Void
    ) {
        self.core = core
        self.inbox = inbox
        self.nightShift = nightShift
        self.loginItem = loginItem
        self.updater = updater
        self.wakeups = wakeups
        self.browser = browser
        self.accessibility = accessibility
        self.notifications = notifications
        self.systemSettings = systemSettings
        self.rereading = rereading
        self.showOnboarding = showOnboarding
        inbox.receive = { [weak self] outcome, snapshot in self?.carryOut(outcome, snapshot) }
    }

    // MARK: Subscriptions

    /// Calls `handler` with each event the core tells, as its outcome is carried out.
    func onEvent(_ handler: @escaping (Event) -> Void) {
        eventHandlers.append(handler)
    }

    /// Calls `handler` once each outcome is carried out, with what it answers and the snapshot its
    /// input left, so what shows the core's state redraws from it.
    func onChange(_ handler: @escaping (Cause, Snapshot) -> Void) {
        changeHandlers.append(handler)
    }

    /// Calls `handler` once, after the next outcome is carried out: the one a command sent now
    /// leads to, unless another input was already on its way.
    func onNextChange(_ handler: @escaping () -> Void) {
        nextChangeHandlers.append(handler)
    }

    // MARK: Carrying out

    /// One of the app's own fixes, which never reaches the core; the rest are commands, sent as
    /// any other.
    func perform(_ fix: AppFix) {
        switch fix {
        case .askForAccessibility:
            accessibility.ask()
        case .allowNotifications:
            notifications.allowNotifications()
        case .openLoginItems:
            systemSettings.open(.loginItems)
        case .openLocationServices:
            systemSettings.open(.locationServices)
        }
    }

    /// Releases every force as the app quits (ADR-0008). The core decides what came before, then
    /// what releases them, and both are carried out here, in order, before the app exits. The core
    /// decides nothing after, so its snapshot is the one quitting left.
    func stop() {
        let released = core.stop()
        inbox.handOver()
        carryOut(released, core.snapshot())
    }

    private func carryOut(_ outcome: Outcome, _ snapshot: Snapshot) {
        outcome.effects.forEach(carryOut)
        for event in outcome.events {
            eventHandlers.forEach { $0(event) }
        }
        changeHandlers.forEach { $0(outcome.cause, snapshot) }
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
            updater.start()
        case .checkForUpdates:
            updater.checkForUpdates()
        case .setNightShift(let enabled):
            guard let nightShift else {
                log.error("Night Shift is unavailable; dropped setNightShift(enabled: \(enabled))")
                return
            }
            if let host = browser.host {
                log.info("Night Shift turns \(enabled ? "on" : "off") on \(host, privacy: .private)")
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
            if commit { rereading.rereadStrength() }
        case .scheduleWakeup(let at):
            wakeups.schedule(at: at)
        case .cancelWakeup:
            wakeups.cancel()
        // Version 1 is the only onboarding so far, so `completed` is always 0.
        case .showOnboarding:
            showOnboarding()
        case .watchBrowser(let bundleId, let generation):
            browser.watch(bundleId: bundleId, generation: generation)
        case .stopWatching:
            browser.stop()
        }
    }

    /// Registers or unregisters the login item, as the user chose (ADR-0011), then has where that
    /// left it read back.
    private func changeLoginItem(_ change: (LoginItemRegistering) throws -> Void) {
        do {
            try change(loginItem)
        } catch {
            log.error("couldn't change the login item: \(error, privacy: .public)")
        }
        rereading.rereadLoginItem()
    }
}

// MARK: Adapters

/// Writes Night Shift through CoreBrightness.
protocol NightShiftWriting {
    func setEnabled(_ enabled: Bool)
    /// macOS applies a new schedule at once, so it can switch Night Shift on or off.
    func setSchedule(_ schedule: Schedule)
    /// Uncommitted, macOS shows the strength, even with Night Shift off, but doesn't save it.
    func setStrength(_ value: UInt8, commit: Bool)
}

/// The login item, which opens Nightcap at login as the user chose (ADR-0011).
protocol LoginItemRegistering {
    func register() throws
    func unregister() throws
}

/// Sparkle, which updates a self-managed install (ADR-0011).
protocol Updating {
    /// Starts the daily checks.
    func start()
    /// Sparkle's own check, as if the user had asked for one, with its window in front.
    func checkForUpdates()
}

/// The core's wakeups, on a wall-clock timer so one fires on time after sleep or a clock change
/// (ADR-0002). Its firing is a fact, which the reporter reports.
protocol WakeupScheduling {
    /// Replaces any earlier wakeup.
    func schedule(at date: Date)
    func cancel()
}

/// Reads the front browser's host while the core watches it (ADR-0012). Each read is a fact,
/// which the reporter reports.
protocol BrowserWatching {
    /// The host last read under the current watch, which is logged only as Night Shift changes
    /// (ADR-0009).
    var host: String? { get }
    /// Replaces any earlier watch; each read is reported under `generation`.
    func watch(bundleId: String, generation: UInt64)
    func stop()
}

/// Asks for Accessibility: the macOS prompt while Nightcap has no entry in the list, else the
/// pane in System Settings (ADR-0012).
protocol AccessibilityAsking {
    func ask()
}

/// Asks for notifications: the macOS prompt while they were never asked for, which shows only
/// once, else Nightcap's page in Notification settings.
protocol NotificationsAsking {
    func allowNotifications()
}

/// A pane of System Settings Nightcap opens, where the user puts back what they set up.
enum SystemSettingsPane {
    /// Login Items, where macOS asks for its approval.
    case loginItems
    case locationServices
    /// Nightcap's page in Notification settings.
    case notifications
}

protocol SystemSettingsOpening {
    func open(_ pane: SystemSettingsPane)
}

/// Reads again, for the core, what an effect changed that macOS doesn't announce.
protocol Rereading {
    /// The login item, once registered or unregistered.
    func rereadLoginItem()
    /// The strength, once a write is saved.
    func rereadStrength()
}
