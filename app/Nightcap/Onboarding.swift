import AppKit
import UserNotifications
import os

/// Onboarding, version 1 (ADR-0016): what Nightcap does, notifications, then whether to check
/// websites, asking for Accessibility if so. Every step can be skipped, and closing the window
/// finishes it with whatever was chosen so far. Quitting Nightcap leaves it unfinished.
final class Onboarding: NSObject, NSWindowDelegate {
    private enum Step {
        case welcome, notifications, websites, accessibility
    }

    private let window = NSWindow(
        contentRect: NSRect(x: 0, y: 0, width: 480, height: 260),
        styleMask: [.titled, .closable],
        backing: .buffered,
        defer: false
    )
    private let log = Logger(subsystem: Bundle.main.bundleIdentifier!, category: "app")
    private let accessibility: AccessibilityPermission
    private let onNotificationsRead: () -> Void
    private let onFinish: (_ websites: Bool) -> Void
    private var step = Step.welcome
    private var notifications = UNAuthorizationStatus.notDetermined
    private var websites = false
    private var closedByUser = false
    private var observer: NSObjectProtocol?

    /// `onNotificationsRead` runs each time the notification permission is read, which can find
    /// it changed; `onFinish` once onboarding ends, either way.
    init(
        accessibility: AccessibilityPermission,
        onNotificationsRead: @escaping () -> Void,
        onFinish: @escaping (_ websites: Bool) -> Void
    ) {
        self.accessibility = accessibility
        self.onNotificationsRead = onNotificationsRead
        self.onFinish = onFinish
        super.init()
        window.title = "Welcome to Nightcap"
        window.isReleasedWhenClosed = false
        window.delegate = self
        show(.welcome)
        window.center()
        // macOS posts nothing when notifications are switched on in System Settings, so the
        // step re-reads them when the user comes back.
        observer = NotificationCenter.default.addObserver(
            forName: NSApplication.didBecomeActiveNotification, object: nil, queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated {
                guard let self, self.step == .notifications else { return }
                Task {
                    await self.readNotifications()
                    self.show(.notifications)
                }
            }
        }
    }

    /// Opening a menu-bar-only app doesn't activate it, and macOS refuses the app's own request
    /// at launch, however long it waits (measured on macOS 27). Only the forcing call, which
    /// Apple means to deprecate, works. A click on Nightcap's menu or Dock icon activates it
    /// without one.
    func present() {
        NSApp.activate(ignoringOtherApps: true)
        window.makeKeyAndOrderFront(nil)
    }

    /// Redraws the Accessibility step, whose status follows the grant.
    func accessibilityChanged() {
        if step == .accessibility { show(.accessibility) }
    }

    /// Asked only when the user closes the window, never when Nightcap quits.
    func windowShouldClose(_ sender: NSWindow) -> Bool {
        closedByUser = true
        return true
    }

    func windowWillClose(_ notification: Notification) {
        if let observer { NotificationCenter.default.removeObserver(observer) }
        if closedByUser { onFinish(websites) }
    }

    private func show(_ step: Step) {
        self.step = step
        switch step {
        case .welcome:
            layout(
                title: "Nightcap",
                body: "Nightcap switches Night Shift off while the apps and websites you choose are in front, and gives it back when you leave them. The rest of the time it follows your Night Shift schedule.",
                buttons: [("Continue", #selector(next))]
            )
        case .notifications:
            let purpose = "When a change made outside Nightcap overrides one of your rules, Nightcap tells you what it took the change to mean."
            switch notifications {
            case .notDetermined:
                layout(
                    title: "Notifications",
                    body: purpose,
                    buttons: [("Not Now", #selector(next)), ("Allow Notifications", #selector(allowNotifications))]
                )
            // macOS shows its prompt only once, so after a refusal only System Settings can allow them.
            case .denied:
                layout(
                    title: "Notifications",
                    body: purpose + " Notifications are switched off for Nightcap in System Settings.",
                    buttons: [("Continue", #selector(next)), ("Open Notification Settings…", #selector(openNotificationSettings))]
                )
            default:
                layout(
                    title: "Notifications",
                    body: purpose + "\n\n✓ Notifications are allowed.",
                    buttons: [("Continue", #selector(next))]
                )
            }
        case .websites:
            layout(
                title: "Websites",
                body: "Nightcap can also switch Night Shift off on websites you choose, in Safari and Chrome. To do that it reads the address of the front tab, which needs the Accessibility permission. Only the site's name is ever used.",
                buttons: [("Not Now", #selector(done)), ("Check Websites", #selector(checkWebsites))]
            )
        case .accessibility:
            let granted = accessibility.granted
            layout(
                title: "Accessibility",
                body: granted
                    ? "✓ Accessibility is allowed. Nightcap can check websites now."
                    : "Allow Nightcap in System Settings → Privacy & Security → Accessibility. This step turns green once it's allowed.",
                buttons: granted
                    ? [("Done", #selector(done))]
                    : [("Done", #selector(done)), ("Allow Accessibility…", #selector(allowAccessibility))]
            )
        }
    }

    private func layout(title: String, body: String, buttons: [(String, Selector)]) {
        let heading = NSTextField(labelWithString: title)
        heading.font = .boldSystemFont(ofSize: 17)
        let text = NSTextField(wrappingLabelWithString: body)
        text.preferredMaxLayoutWidth = 432
        let row = NSStackView(views: buttons.map { title, action in
            NSButton(title: title, target: self, action: action)
        })
        row.spacing = 12
        // The last button is the one to press.
        (row.views.last as? NSButton)?.keyEquivalent = "\r"
        let spacer = NSView()
        spacer.setContentHuggingPriority(.defaultLow, for: .vertical)
        let buttonRow = NSStackView(views: [NSView(), row])
        buttonRow.distribution = .fill
        let stack = NSStackView(views: [heading, text, spacer, buttonRow])
        stack.orientation = .vertical
        stack.alignment = .leading
        stack.spacing = 12
        stack.edgeInsets = NSEdgeInsets(top: 24, left: 24, bottom: 20, right: 24)
        buttonRow.widthAnchor.constraint(equalTo: stack.widthAnchor, constant: -48).isActive = true
        window.contentView = stack
    }

    @objc private func next() {
        guard step == .welcome else {
            show(.websites)
            return
        }
        Task {
            await readNotifications()
            show(.notifications)
        }
    }

    private func readNotifications() async {
        notifications = await UNUserNotificationCenter.current().notificationSettings().authorizationStatus
        onNotificationsRead()
    }

    @objc private func allowNotifications() {
        Task {
            do {
                _ = try await UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound])
            } catch {
                log.error("couldn't request notification permission: \(error, privacy: .public)")
            }
            await readNotifications()
            show(.websites)
        }
    }

    @objc private func openNotificationSettings() {
        let pane = "x-apple.systempreferences:com.apple.Notifications-Settings.extension?id=\(Bundle.main.bundleIdentifier!)"
        NSWorkspace.shared.open(URL(string: pane)!)
    }

    @objc private func checkWebsites() {
        websites = true
        show(.accessibility)
        if !accessibility.granted { accessibility.ask() }
    }

    @objc private func allowAccessibility() {
        accessibility.ask()
    }

    @objc private func done() {
        window.performClose(nil)
    }
}
