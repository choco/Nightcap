import AppKit
import KeyboardShortcuts
import NightcapCore
import os

@main
final class AppDelegate: NSObject, NSApplicationDelegate {
    private let log = Logger(subsystem: Bundle.main.bundleIdentifier!, category: "app")
    private var host: CoreHost?
    private lazy var notifications = Notifications()
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
        Snapshot.runIfAsked()
        #endif
        quitOnTerminationSignal()
        siteIcons.onArrival = { [weak self] site in
            self?.statusMenu?.iconArrived(site)
            self?.settingsWindow?.iconArrived()
            self?.onboarding?.iconArrived()
        }
        let host: CoreHost
        do {
            host = try CoreHost(
                dockIcon: dockIcon,
                notifications: notifications,
                accessibilityChanged: { [weak self] in self?.onboarding?.accessibilityChanged() },
                notificationsChanged: { [weak self] in self?.onboarding?.notificationsChanged() },
                showOnboarding: { [weak self] in self?.showOnboarding() })
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
        self.host = host
        host.onEvent { [notifications] event in notifications.tell(event) }
        host.onChange { [weak self] in self?.refresh() }
        host.launch()
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
        host?.nightShift?.endPeek()
        tour?.end()
        host?.stop()
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
                self?.host?.nightShift?.peek()
                self?.tour?.shortcutPressed()
            },
            tap: { [weak self] in
                guard let host = self?.host else { return }
                host.send(.toggle)
                host.onNextChange { host.nightShift?.endPeek() }
                self?.tour?.shortcutTapped()
            },
            release: { [weak self] in
                self?.host?.nightShift?.endPeek()
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
        guard let host else { return }
        let core = host.core
        let tour = Tour(
            shortcut: KeyboardShortcuts.getShortcut(for: .nightShift),
            browser: tourBrowser(core.settings()),
            current: {
                (
                    core.tourReport(),
                    Menu(.init(asking: core), now: Date(), appName: Apps.runningName(of:), words: .system).moon
                )
            },
            press: { [weak self] in self?.shortcut?.press() },
            release: { [weak self] in self?.shortcut?.release() },
            begin: { host.send($0) },
            onEnd: { [weak self] begun in
                guard let self else { return }
                self.tour = nil
                if begun { host.send(.endTour) }
            })
        self.tour = tour
        tour.start()
        onShown()
    }

    /// The default browser, for the tour's site step, while website checking can watch it.
    private func tourBrowser(_ settings: Settings) -> Tour.Browser? {
        guard settings.websiteChecking.on, host?.accessibility?.granted == true,
            let url = NSWorkspace.shared.urlForApplication(toOpen: URL(string: "https://example.com")!),
            let bundleId = Bundle(url: url)?.bundleIdentifier, BrowserWatcher.browsers.contains(bundleId)
        else { return nil }
        return Tour.Browser(bundleId: bundleId, name: Apps.name(at: url))
    }

    private func showOnboarding() {
        guard let host, let accessibility = host.accessibility else { return }
        let core = host.core
        let onboarding = Onboarding(
            steps: core.onboarding(),
            rules: { core.settings().rules },
            icons: siteIcons,
            sun: host.nightShift?.sunTimes.flatMap { times in
                guard let sunset = times.sunsets.first, let sunrise = times.sunrises.first else { return nil }
                return Sun(sunset: sunset, sunrise: sunrise)
            },
            now: { Date() },
            shortcut: KeyboardShortcuts.getShortcut(for: .nightShift),
            accessibilityGranted: { accessibility.granted },
            askForAccessibility: { accessibility.ask() },
            choose: { command in host.send(command) },
            notifications: { host.notificationPermission },
            askForNotifications: { host.askForNotifications() },
            onFinish: { [weak self] websites in
                guard let self else { return }
                host.send(.finishOnboarding(websites: websites))
                self.onboarding = nil
                host.watchPermissions(closely: false)
                showStatusMenu()
                // The Dock icon goes once the popover is up, while Nightcap stays active with it.
                statusMenu?.pointOut(
                    takeTour: { [weak self] in DispatchQueue.main.async { self?.startTour() } },
                    onShown: { [weak self] in self?.dockIcon.hide(for: .onboarding) })
            })
        self.onboarding = onboarding
        dockIcon.show(for: .onboarding)
        onboarding.present()
        host.watchPermissions(closely: true)
    }

    private func showStatusMenu() {
        guard let host else { return }
        let core = host.core
        statusMenu = StatusMenu(
            current: { Menu(Menu.Answers(asking: core), now: Date(), appName: Apps.runningName(of:), words: .system) },
            icons: siteIcons,
            choose: { command in host.send(command) },
            openSettings: { [weak self] onProblem in
                self?.showSettings(onProblem ? SettingsWindow.tab(for: core.settings()) : nil)
            },
            onOpen: { [weak self] in
                self?.tour?.menuOpened()
                self?.askForSiteIcons()
                Task { await host.reportSystemState() }
            },
            onClose: { [weak self] in self?.tour?.menuClosed() })
    }

    /// Made as Settings first opens, by when the launch's outcome has started Sparkle, if it ever
    /// runs: the item that opens it appears only as the core's first outcomes are carried out.
    private func makeSettingsWindow(_ host: CoreHost) -> SettingsWindow {
        let core = host.core
        return SettingsWindow(
            current: { core.settings() },
            icons: siteIcons,
            choose: { command in host.send(command) },
            perform: { fix in host.perform(fix) },
            checkForUpdates: host.updaterRunning ? { host.checkForUpdates() } : nil,
            export: { core.export(format: $0) },
            takeTour: { [weak self] in
                guard let self else { return }
                settingsWindow?.close()
                DispatchQueue.main.async { [weak self] in
                    self?.startTour { [weak self] in self?.dockIcon.hide(for: .settings) }
                }
            },
            onClose: { [weak self] in
                self?.dockIcon.hide(for: .settings)
                host.watchPermissions(closely: false)
            })
    }

    /// While website checking is on, the icons of the site in front and every site with a rule,
    /// which the menu and the Rules tab show (ADR-0019).
    private func askForSiteIcons() {
        guard let core = host?.core else { return }
        let settings = core.settings()
        guard settings.websiteChecking.on else { return }
        let front = core.front().site.map { [$0] } ?? []
        let ruled = settings.rules.compactMap { rule -> String? in
            if case .domain(let domain) = rule.identity { domain } else { nil }
        }
        siteIcons.ask(front + ruled)
    }

    /// On `tab`, or on the tab last shown.
    private func showSettings(_ tab: SettingsWindow.Tab? = nil) {
        guard let host, statusMenu != nil else { return }
        let settingsWindow = self.settingsWindow ?? makeSettingsWindow(host)
        self.settingsWindow = settingsWindow
        dockIcon.show(for: .settings)
        settingsWindow.show(tab)
        host.watchPermissions(closely: true)
    }

    /// After each outcome, whichever fact or command it answers, the CLI's included. The menu-bar
    /// item appears once onboarding is finished; onboarding asks for notifications itself, and
    /// without it they're asked for as the item first appears.
    private func refresh() {
        if statusMenu == nil, onboarding == nil, let host, host.core.onboarding().finished {
            showStatusMenu()
            host.askForNotifications()
        }
        statusMenu?.refresh()
        settingsWindow?.refresh()
        tour?.refresh()
    }
}
