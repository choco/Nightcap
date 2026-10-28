import AppKit
import KeyboardShortcuts
import NightcapCore
import os

/// The composition root: it starts the core, wires the reporter, which tells the core what macOS
/// observes, and the executor, which carries out what the core decides, to the adapters that
/// drive macOS, and sends the UI's commands to the core (ADR-0002). No sender looks at an
/// outcome.
@main
final class AppDelegate: NSObject, NSApplicationDelegate {
    private let log = Logger(subsystem: Bundle.main.bundleIdentifier!, category: "app")
    /// The core, whose reads answer from the snapshot it published after its last input.
    private var core: Core?
    private var reporter: Reporter?
    private var executor: Executor?
    private var accessibilityAsk: AccessibilityAsk?
    private let onboardingRecord = OnboardingRecord(defaults: .standard)
    private lazy var notifications = Notifications()
    private lazy var nightShift = NightShiftAdapter { [weak self] status in self?.reporter?.reportNightShift(status) }
    private lazy var accessibility = AccessibilityPermission { [weak self] _ in
        Task { await self?.reporter?.reportSystemState() }
    }
    private lazy var updater = Updater(dockIcon: dockIcon, notifications: notifications)
    private var terminationSignal: DispatchSourceSignal?
    private var statusMenu: StatusMenu?
    private var settingsWindow: SettingsWindow?
    private let siteIcons = SiteIcons()
    private var onboarding: Onboarding?
    private var shortcut: Shortcut?
    private var tour: Tour?
    private let dockIcon = DockIcon()

    static func main() {
        let app = NSApplication.shared
        let delegate = AppDelegate()
        app.delegate = delegate
        app.mainMenu = MainMenu.make()
        withExtendedLifetime(delegate) { app.run() }
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        #if DEBUG
        SnapshotRenderer.runIfAsked()
        #endif
        quitOnTerminationSignal()
        siteIcons.onArrival = { [weak self] site in
            self?.statusMenu?.iconArrived(site)
            self?.settingsWindow?.iconArrived()
            self?.onboarding?.iconArrived()
        }
        let inbox = Inbox()
        let core: Core
        do {
            let bundleFolder = Bundle.main.bundleURL.deletingLastPathComponent().path
            core = try Core.start(
                bundleId: Bundle.main.bundleIdentifier!,
                bundleWritable: FileManager.default.isWritableFile(atPath: bundleFolder),
                timeZone: TimeZone.current.identifier,
                listener: inbox)
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
        let loginItem = LoginItemAdapter()
        let reporter = Reporter(core: core, nightShift: nightShift, accessibility: accessibility, loginItem: loginItem)
        let executor = Executor(
            core: core, inbox: inbox, nightShift: nightShift, loginItem: loginItem, updater: updater,
            wakeups: WakeupTimer { reporter.wakeupCameDue() },
            browser: BrowserWatch { generation, host in reporter.reportHost(generation: generation, host: host) },
            accessibility: accessibility, notifications: reporter, systemSettings: SystemSettings(),
            rereading: reporter)
        self.core = core
        self.reporter = reporter
        self.executor = executor
        accessibilityAsk = AccessibilityAsk(core.snapshot())
        let onboardingRecord = onboardingRecord
        let whatsNew = WhatsNew(defaults: .standard, words: .system)
        let announcer = Announcer(defaults: .standard, words: .system)
        executor.onEvent { [notifications] event in notifications.tell(event) }
        executor.onChange { [notifications] cause, snapshot in
            if cause == .launch, let notice = whatsNew.launched(snapshot.about()) { notifications.post(notice) }
            announcer.heard(cause, snapshot).forEach { notifications.post($0) }
        }
        executor.onChange { [weak self] cause, snapshot in
            if cause == .launch, onboardingRecord.launched(snapshot) { self?.showOnboarding(snapshot) }
            self?.refresh(snapshot)
        }
        reporter.launch()
        listenForShortcut()
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
        guard let executor else { return }
        nightShift?.endPeek()
        tour?.end()
        executor.stop()
    }

    /// launchd stops the app with SIGTERM, whose default action would skip
    /// applicationWillTerminate and leave any force in place (ADR-0008).
    private func quitOnTerminationSignal() {
        signal(SIGTERM, SIG_IGN)
        let source = DispatchSource.makeSignalSource(signal: SIGTERM, queue: .main)
        source.setEventHandler { NSApp.terminate(nil) }
        source.resume()
        terminationSignal = source
    }

    /// The press peeks at once, so a tap that turns Night Shift off shows no flash of warmth:
    /// switching it ends the peek. A tap that leaves it on, as when CoreBrightness refuses the
    /// switch, ends the peek once the toggle's outcome is carried out (ADR-0015).
    private func listenForShortcut() {
        shortcut = Shortcut(
            press: { [weak self] in
                self?.nightShift?.peek()
                self?.tour?.shortcutPressed()
            },
            tap: { [weak self] in
                guard let self, let core, let executor else { return }
                core.send(command: .toggle)
                executor.onNextChange { [weak self] in self?.nightShift?.endPeek() }
                tour?.shortcutTapped()
            },
            release: { [weak self] in
                self?.nightShift?.endPeek()
                self?.tour?.shortcutReleased()
            })
    }

    /// From "Nightcap is up here" or Settings, once the popover or window is gone, as the click
    /// that started the tour ends: the panel then keeps the focus. `onShown` runs once it's up.
    private func startTour(onShown: () -> Void = {}) {
        if let tour {
            tour.reveal()
            return
        }
        guard let core else { return }
        let snapshot = core.snapshot()
        let tour = Tour(
            shortcut: KeyboardShortcuts.getShortcut(for: .nightShift),
            browser: tourBrowser(snapshot),
            press: { [weak self] in self?.shortcut?.press() },
            release: { [weak self] in self?.shortcut?.release() },
            begin: { core.sendApp(command: $0) },
            onEnd: { [weak self] begun in
                guard let self else { return }
                self.tour = nil
                if begun { core.sendApp(command: .endTrial) }
            })
        self.tour = tour
        tour.start(snapshot)
        onShown()
    }

    /// The default browser, for the tour's site step, while website checking can watch it, as
    /// `snapshot` has it.
    private func tourBrowser(_ snapshot: Snapshot) -> Tour.Browser? {
        guard snapshot.preferences().websiteChecking.state == .on, snapshot.checks().permissions.accessibility,
            let url = NSWorkspace.shared.urlForApplication(toOpen: URL(string: "https://example.com")!),
            let bundleId = Bundle(url: url)?.bundleIdentifier, BrowserWatcher.browsers.contains(bundleId)
        else { return nil }
        return Tour.Browser(bundleId: bundleId, name: Apps.name(at: url))
    }

    /// Onboarding, drawn first from `snapshot`, the launch's.
    private func showOnboarding(_ snapshot: Snapshot) {
        guard let core, let reporter, let executor else { return }
        let onboarding = Onboarding(
            snapshot,
            icons: siteIcons,
            sun: nightShift?.sunTimes.flatMap { times in
                guard let sunset = times.sunsets.first, let sunrise = times.sunrises.first else { return nil }
                return Sun(sunset: sunset, sunrise: sunrise)
            },
            now: { Date() },
            shortcut: KeyboardShortcuts.getShortcut(for: .nightShift),
            askForAccessibility: { executor.perform(.askForAccessibility) },
            choose: { command in core.send(command: command) },
            askForNotifications: { reporter.askForNotifications() },
            onFinish: { [weak self] websites in
                guard let self else { return }
                log.info("onboarding finished")
                if websites == .on { accessibilityAsk?.onboardingTurnedOn() }
                onboardingRecord.finish()
                self.onboarding = nil
                reporter.watchPermissions(closely: false)
                showStatusMenu(core.snapshot())
                // The Dock icon goes once the popover is up, while Nightcap stays active with it.
                statusMenu?.pointOut(
                    takeTour: { [weak self] in DispatchQueue.main.async { self?.startTour() } },
                    onShown: { [weak self] in self?.dockIcon.hide(for: .onboarding) })
            })
        self.onboarding = onboarding
        dockIcon.show(for: .onboarding)
        onboarding.present()
        reporter.watchPermissions(closely: true)
    }

    /// The menu-bar item, drawn first from `snapshot`.
    private func showStatusMenu(_ snapshot: Snapshot) {
        guard let core, let reporter else { return }
        statusMenu = StatusMenu(
            snapshot,
            current: { core.snapshot() },
            icons: siteIcons,
            choose: { command in core.send(command: command) },
            openSettings: { [weak self] onProblem in
                self?.showSettings(onProblem ? Settings.tab(for: core.snapshot().checks().checks) : nil)
            },
            onOpen: { [weak self] in
                self?.tour?.menuOpened()
                self?.askForSiteIcons()
                Task { await reporter.reportSystemState() }
            },
            onClose: { [weak self] in self?.tour?.menuClosed() })
    }

    /// Made as Settings first opens, by when the launch's outcome has started Sparkle, if it ever
    /// runs: the item that opens it appears only as the core's first outcomes are carried out.
    private func makeSettingsWindow(core: Core, reporter: Reporter, executor: Executor) -> SettingsWindow {
        SettingsWindow(
            current: { core.snapshot() },
            icons: siteIcons,
            choose: { command in core.send(command: command) },
            perform: { fix in executor.perform(fix) },
            checkForUpdates: updater.running ? { core.send(command: .checkForUpdates) } : nil,
            export: { core.snapshot().export(format: $0) },
            takeTour: { [weak self] in
                guard let self else { return }
                settingsWindow?.close()
                DispatchQueue.main.async { [weak self] in
                    self?.startTour { [weak self] in self?.dockIcon.hide(for: .settings) }
                }
            },
            onClose: { [weak self] in
                self?.dockIcon.hide(for: .settings)
                reporter.watchPermissions(closely: false)
            })
    }

    /// While website checking is on, the icons of the site in front and every site with a rule,
    /// which the menu and the Rules tab show (ADR-0019).
    private func askForSiteIcons() {
        guard let core else { return }
        let snapshot = core.snapshot()
        guard snapshot.preferences().websiteChecking.state == .on else { return }
        let front = snapshot.front().site.map { [$0] } ?? []
        let ruled = snapshot.rules().rules.compactMap { listed -> String? in
            guard case .domain(let domain) = listed.rule.place else { return nil }
            switch listed.rule.setBy {
            case .config, .you: return domain
            case .trial, .outsideChange: return nil
            }
        }
        siteIcons.ask(front + ruled)
    }

    /// On `tab`, or on the tab last shown.
    private func showSettings(_ tab: SettingsWindow.Tab? = nil) {
        guard let core, let reporter, let executor, statusMenu != nil else { return }
        let settingsWindow =
            self.settingsWindow ?? makeSettingsWindow(core: core, reporter: reporter, executor: executor)
        self.settingsWindow = settingsWindow
        dockIcon.show(for: .settings)
        settingsWindow.show(tab)
        reporter.watchPermissions(closely: true)
    }

    /// After each outcome, whichever fact or command it answers, the CLI's included, from the
    /// snapshot its input left, so the menu, Settings, onboarding and the tour all show the same
    /// input's answers, those of the outcome just carried out. Accessibility is asked for as
    /// website checking turns on without it, which each outcome's own snapshot shows even when
    /// another input followed at once. The menu-bar item appears once onboarding is finished;
    /// onboarding asks for notifications itself, and without it they're asked for as the item
    /// first appears.
    private func refresh(_ snapshot: Snapshot) {
        guard let reporter, let executor else { return }
        if accessibilityAsk?.isDue(after: snapshot) == true { executor.perform(.askForAccessibility) }
        if statusMenu == nil, onboarding == nil, onboardingRecord.finished {
            showStatusMenu(snapshot)
            reporter.askForNotifications()
        }
        statusMenu?.refresh(snapshot)
        settingsWindow?.refresh(snapshot)
        onboarding?.refresh(snapshot)
        tour?.refresh(snapshot)
    }
}
