import AppKit
import KeyboardShortcuts
import NightcapCore
import os

@main
final class AppDelegate: NSObject, NSApplicationDelegate {
    private let log = Logger(subsystem: Bundle.main.bundleIdentifier!, category: "app")
    private var core: Core?
    private var mac: Mac?
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
        let waker = MainThreadWaker { [weak self] in self?.mac?.tick() }
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
        let mac = Mac(
            core: core,
            dockIcon: dockIcon,
            accessibilityChanged: { [weak self] in self?.onboarding?.accessibilityChanged() },
            notificationsChanged: { [weak self] in self?.onboarding?.notificationsChanged() },
            showOnboarding: { [weak self] in self?.showOnboarding() },
            changed: { [weak self] in self?.refresh() })
        self.mac = mac
        mac.launch()
        // The menu-bar item appears once onboarding is done. Onboarding asks for notifications
        // itself; without it, they're asked for at launch.
        if onboarding == nil {
            showStatusMenu()
            mac.askForNotifications()
        }
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
        mac?.nightShift?.endPeek()
        tour?.end()
        mac?.handle(.terminating)
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

    /// The press peeks at once, so a tap that turns Night Shift off shows no flash of warmth. A
    /// tap that leaves it on, as when CoreBrightness refuses the switch, ends the peek after
    /// (ADR-0015).
    private func listenForShortcut() {
        shortcut = Shortcut(
            press: { [weak self] in
                self?.mac?.nightShift?.peek()
                self?.tour?.shortcutPressed()
            },
            tap: { [weak self] in
                self?.mac?.handle(.commandChosen(command: .toggle))
                self?.mac?.nightShift?.endPeek()
                self?.tour?.shortcutTapped()
            },
            release: { [weak self] in
                self?.mac?.nightShift?.endPeek()
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
        guard let core, let mac else { return }
        let tour = Tour(
            shortcut: KeyboardShortcuts.getShortcut(for: .nightShift),
            browser: tourBrowser(core.settings()),
            current: { (core.tour(), core.menu().icon) },
            press: { [weak self] in self?.shortcut?.press() },
            release: { [weak self] in self?.shortcut?.release() },
            begin: { mac.handle($0) },
            onEnd: { [weak self] begun in
                guard let self else { return }
                self.tour = nil
                if begun { mac.handle(.tourEnded) }
            })
        self.tour = tour
        tour.start()
        onShown()
    }

    /// The default browser, for the tour's site step, while website checking can watch it.
    private func tourBrowser(_ settings: Settings) -> Tour.Browser? {
        guard settings.websiteChecking.on, mac?.accessibility?.granted == true,
            let url = NSWorkspace.shared.urlForApplication(toOpen: URL(string: "https://example.com")!),
            let bundleId = Bundle(url: url)?.bundleIdentifier, BrowserWatcher.browsers.contains(bundleId)
        else { return nil }
        return Tour.Browser(bundleId: bundleId, name: Apps.name(at: url))
    }

    private func showOnboarding() {
        guard let core, let mac, let accessibility = mac.accessibility else { return }
        let onboarding = Onboarding(
            steps: core.onboarding(),
            rules: { core.settings().rules },
            icons: siteIcons,
            sun: mac.nightShift?.sunTimes.flatMap { times in
                guard let sunset = times.sunsets.first, let sunrise = times.sunrises.first else { return nil }
                return Sun(sunset: sunset, sunrise: sunrise)
            },
            now: { Date() },
            shortcut: KeyboardShortcuts.getShortcut(for: .nightShift),
            accessibilityGranted: { accessibility.granted },
            askForAccessibility: { accessibility.ask() },
            choose: { command in mac.handle(.commandChosen(command: command)) },
            notifications: { mac.notifications },
            askForNotifications: { mac.askForNotifications() },
            onFinish: { [weak self] websites in
                guard let self else { return }
                mac.handle(.onboardingFinished(websites: websites))
                self.onboarding = nil
                mac.watchPermissions(closely: false)
                showStatusMenu()
                // The Dock icon goes once the popover is up, while Nightcap stays active with it.
                statusMenu?.pointOut(
                    takeTour: { [weak self] in DispatchQueue.main.async { self?.startTour() } },
                    onShown: { [weak self] in self?.dockIcon.hide(for: .onboarding) })
            })
        self.onboarding = onboarding
        dockIcon.show(for: .onboarding)
        onboarding.present()
        mac.watchPermissions(closely: true)
    }

    /// Sparkle starts with the launch's effects, so by now it runs if it ever will.
    private func showStatusMenu() {
        guard let core, let mac else { return }
        let checkForUpdates: (() -> Void)? = mac.updaterRunning ? { mac.checkForUpdates() } : nil
        statusMenu = StatusMenu(
            current: { core.menu() },
            icons: siteIcons,
            choose: { command in mac.handle(.commandChosen(command: command)) },
            openSettings: { [weak self] onProblem in
                self?.showSettings(onProblem ? SettingsWindow.tab(for: core.settings()) : nil)
            },
            onOpen: { [weak self] in
                self?.tour?.menuOpened()
                self?.askForSiteIcons()
                Task { await mac.reportSystemState() }
            },
            onClose: { [weak self] in self?.tour?.menuClosed() })
        settingsWindow = SettingsWindow(
            current: { core.settings() },
            icons: siteIcons,
            choose: { command in mac.handle(.commandChosen(command: command)) },
            perform: { fix in mac.perform(fix) },
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
                self?.dockIcon.hide(for: .settings)
                mac.watchPermissions(closely: false)
            })
    }

    /// While website checking is on, the icons of the site in front and every site with a rule,
    /// which the menu and the Rules tab show (ADR-0019).
    private func askForSiteIcons() {
        guard let core else { return }
        let settings = core.settings()
        guard settings.websiteChecking.on else { return }
        let front = core.menu().entries.compactMap { entry -> String? in
            if case .row(let row) = entry { row.site } else { nil }
        }
        let ruled = settings.rules.compactMap { rule -> String? in
            if case .domain(let domain) = rule.identity { domain } else { nil }
        }
        siteIcons.ask(front + ruled)
    }

    /// On `tab`, or on the tab last shown.
    private func showSettings(_ tab: SettingsWindow.Tab? = nil) {
        guard let settingsWindow else { return }
        dockIcon.show(for: .settings)
        settingsWindow.show(tab)
        mac?.watchPermissions(closely: true)
    }

    /// Whatever changed the core's state changed the icon too, CLI commands included.
    private func refresh() {
        statusMenu?.refresh()
        settingsWindow?.refresh()
        tour?.refresh()
    }
}
