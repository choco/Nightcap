import AppKit
import ApplicationServices
import NightcapCore
import os

@main
final class AppDelegate: NSObject, NSApplicationDelegate {
    private let log = Logger(subsystem: Bundle.main.bundleIdentifier!, category: "app")
    private var core: Core?
    private var terminationSignal: DispatchSourceSignal?
    private var accessibilityObserver: NSObjectProtocol?

    static func main() {
        let app = NSApplication.shared
        let delegate = AppDelegate()
        app.delegate = delegate
        withExtendedLifetime(delegate) { app.run() }
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        quitOnTerminationSignal()
        let waker = MainThreadWaker { [weak self] in self?.tick() }
        do {
            core = try Core(bundleId: Bundle.main.bundleIdentifier!, waker: waker)
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
        watchPermissions()
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

    private func watchPermissions() {
        reportPermissions()
        // An undocumented notification macOS posts when the Accessibility list changes.
        accessibilityObserver = DistributedNotificationCenter.default().addObserver(
            forName: Notification.Name("com.apple.accessibility.api"), object: nil, queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated { self?.reportPermissions() }
        }
    }

    private func reportPermissions() {
        guard let core else { return }
        apply(core.handle(event: .permissionsChanged(accessibility: AXIsProcessTrusted())))
    }

    private func tick() {
        guard let core else { return }
        apply(core.tick())
    }

    private func apply(_ effects: [Effect]) {
        for effect in effects {
            switch effect {
            case .setNightShift(let enabled):
                log.error("there's no Night Shift adapter yet; dropped setNightShift(enabled: \(enabled))")
            }
        }
    }
}
