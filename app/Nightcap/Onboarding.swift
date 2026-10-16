import AppKit
import UserNotifications
import os

/// Onboarding, version 1 (ADR-0015, ADR-0016): a line on what Nightcap does over a checklist,
/// one row per choice. The current row explains itself and asks; an answered one shows its
/// answer and can be changed. Closing the window finishes onboarding with what was chosen so
/// far. Quitting Nightcap leaves it unfinished.
final class Onboarding: NSObject, NSWindowDelegate {
    enum Row: CaseIterable {
        case notifications, websites, accessibility, openAtLogin
    }

    struct Answers: Equatable {
        enum Notifications { case allowed, notAllowed, notNow }

        var notifications: Notifications?
        var websites: Bool?
        var accessibility: Bool?
        var openAtLogin: Bool?
    }

    private static let width: CGFloat = 560
    /// A row's text: the group, less its padding, the mark and the gap after it.
    private static let textWidth: CGFloat = width - 48 - 28 - 18 - 12

    private let window = NSWindow(
        contentRect: NSRect(x: 0, y: 0, width: width, height: 400),
        styleMask: [.titled, .closable],
        backing: .buffered,
        defer: false
    )
    private let log = Logger(subsystem: Bundle.main.bundleIdentifier!, category: "app")
    private let accessibilityGranted: () -> Bool
    private let askForAccessibility: () -> Void
    /// Nil where Nightcap doesn't open itself at login, so the row doesn't show.
    private let opensAtLogin: Bool?
    /// The config's `websites`, which answers its row for good; nil when the config leaves it
    /// to the user.
    private let websitesFromConfig: Bool?
    private let chooseOpenAtLogin: (Bool) -> Void
    private let onNotificationsRead: () -> Void
    private let onFinish: (_ websites: Bool) -> Void
    private var answers = Answers()
    private var current: Row?
    private var notifications = UNAuthorizationStatus.notDetermined
    private var waitingForAccessibility = false
    private var closedByUser = false
    private var poll: Timer?
    #if DEBUG
    /// Set by snapshots, so the real notification permission can't change what they show.
    private var frozen = false
    #endif

    /// `opensAtLogin` is whether Nightcap opens at login now, or nil where it never opens
    /// itself. `onNotificationsRead` runs each time the notification permission is read, which
    /// can find it changed; `onFinish` once onboarding ends, either way.
    init(
        accessibilityGranted: @escaping () -> Bool,
        askForAccessibility: @escaping () -> Void,
        opensAtLogin: Bool?,
        websitesFromConfig: Bool?,
        chooseOpenAtLogin: @escaping (Bool) -> Void,
        onNotificationsRead: @escaping () -> Void,
        onFinish: @escaping (_ websites: Bool) -> Void
    ) {
        self.accessibilityGranted = accessibilityGranted
        self.askForAccessibility = askForAccessibility
        self.opensAtLogin = opensAtLogin
        self.websitesFromConfig = websitesFromConfig
        self.chooseOpenAtLogin = chooseOpenAtLogin
        self.onNotificationsRead = onNotificationsRead
        self.onFinish = onFinish
        super.init()
        window.title = "Welcome to Nightcap"
        window.isReleasedWhenClosed = false
        window.delegate = self
        window.backgroundColor = .nightcapWindow
        // An install that already opens at login has its answer.
        if opensAtLogin == true { answers.openAtLogin = true }
        if let websitesFromConfig {
            answers.websites = websitesFromConfig
            if websitesFromConfig, accessibilityGranted() { answers.accessibility = true }
        }
        advance()
        draw()
        window.center()
        Task { await readNotifications() }
        // macOS posts nothing when notifications are switched in System Settings, so the row
        // re-reads them every second while onboarding is open.
        let poll = Timer(timeInterval: 1, repeats: true) { [weak self] _ in
            MainActor.assumeIsolated {
                _ = Task { await self?.readNotifications() }
            }
        }
        poll.tolerance = 0.25
        RunLoop.main.add(poll, forMode: .common)
        self.poll = poll
    }

    /// Opening a menu-bar-only app doesn't activate it, and macOS refuses the app's own request
    /// at launch, however long it waits (measured on macOS 27). Only the forcing call, which
    /// Apple means to deprecate, works. A click on Nightcap's menu or Dock icon activates it
    /// without one.
    func present() {
        NSApp.activate(ignoringOtherApps: true)
        window.makeKeyAndOrderFront(nil)
    }

    /// The Accessibility row turns green the moment the grant arrives.
    func accessibilityChanged() {
        guard answers.websites == true, answers.accessibility != true, accessibilityGranted() else { return }
        answers.accessibility = true
        waitingForAccessibility = false
        advance()
        draw()
    }

    /// Asked only when the user closes the window, never when Nightcap quits.
    func windowShouldClose(_ sender: NSWindow) -> Bool {
        closedByUser = true
        return true
    }

    func windowWillClose(_ notification: Notification) {
        poll?.invalidate()
        if closedByUser { onFinish(answers.websites == true) }
    }

    #if DEBUG
    /// The window's content as a PNG, in the given state, for comparing against the mockup.
    func snapshot(_ answers: Answers, current: Row?) -> Data? {
        frozen = true
        self.answers = answers
        self.current = current
        window.alphaValue = 0
        draw()
        window.orderFront(nil)
        guard let view = window.contentView else { return nil }
        view.layoutSubtreeIfNeeded()
        view.displayIfNeeded()
        guard let bitmap = view.bitmapImageRepForCachingDisplay(in: view.bounds) else { return nil }
        view.cacheDisplay(in: view.bounds, to: bitmap)
        return bitmap.representation(using: .png, properties: [:])
    }
    #endif

    // MARK: The checklist

    private var rows: [Row] {
        Row.allCases.filter { row in
            switch row {
            case .accessibility: answers.websites == true
            case .openAtLogin: opensAtLogin != nil
            default: true
            }
        }
    }

    private func answered(_ row: Row) -> Bool {
        switch row {
        case .notifications: answers.notifications != nil
        case .websites: answers.websites != nil
        case .accessibility: answers.accessibility != nil
        case .openAtLogin: answers.openAtLogin != nil
        }
    }

    /// The first row still to answer becomes current; with none left, onboarding is complete.
    private func advance() {
        current = rows.first { !answered($0) }
    }

    private func readNotifications() async {
        let status = await UNUserNotificationCenter.current().notificationSettings().authorizationStatus
        #if DEBUG
        if frozen { return }
        #endif
        notifications = status
        onNotificationsRead()
        // Notifications allowed already, or since, have their answer.
        if status == .authorized || status == .provisional, answers.notifications != .allowed {
            answers.notifications = .allowed
            if current == .notifications { advance() }
        }
        draw()
    }

    private func answer(_ change: (inout Answers) -> Void) {
        change(&answers)
        advance()
        draw()
    }

    // MARK: Drawing

    private var drawn: (Answers, Row?, UNAuthorizationStatus, Bool)?

    /// Rebuilds the window when what it shows changed, then fits the window to it at once,
    /// keeping its top edge: animating the frame would show the new rows in the old size.
    private func draw() {
        let state = (answers, current, notifications, waitingForAccessibility)
        if let drawn, drawn == state { return }
        drawn = state
        let content = NSStackView(views: [intro(), checklist(), footer()])
        content.orientation = .vertical
        content.alignment = .leading
        content.spacing = 18
        content.edgeInsets = NSEdgeInsets(top: 18, left: 24, bottom: 20, right: 24)
        content.setCustomSpacing(22, after: content.views[1])
        for view in content.views {
            view.widthAnchor.constraint(equalToConstant: Self.width - 48).isActive = true
        }
        let size = content.fittingSize
        let chrome = window.frame.height - window.contentLayoutRect.height
        let backdrop = Backdrop()
        content.translatesAutoresizingMaskIntoConstraints = false
        backdrop.addSubview(content)
        NSLayoutConstraint.activate([
            content.leadingAnchor.constraint(equalTo: backdrop.leadingAnchor),
            content.trailingAnchor.constraint(equalTo: backdrop.trailingAnchor),
            content.topAnchor.constraint(equalTo: backdrop.topAnchor),
        ])
        // The new rows go in before the window takes their size: resizing first would draw
        // the old rows squeezed into the new size.
        window.contentView = backdrop
        window.setFrame(
            NSRect(
                x: window.frame.minX, y: window.frame.maxY - size.height - chrome,
                width: size.width, height: size.height + chrome),
            display: true)
    }

    /// The app icon beside what Nightcap does.
    private func intro() -> NSView {
        // The icon's tile fills 80% of its image, so a 64-point image shows the mockup's
        // 52-point tile, 6 points in.
        let icon = NSImageView(image: NSApp.applicationIconImage)
        icon.imageScaling = .scaleProportionallyUpOrDown
        icon.translatesAutoresizingMaskIntoConstraints = false
        let title = Look.label("Nightcap", font: .nightcapWordmark(size: 26), color: .labelColor, width: 400)
        let text = Look.label(
            "Nightcap switches Night Shift off while the apps and websites you choose are in front, and gives it back when you leave them. The rest of the time it follows your Night Shift schedule.",
            size: 13, weight: .regular, color: .nightcapSecondary, width: Self.width - 48 - 66)
        let words = NSStackView(views: [title, text])
        words.orientation = .vertical
        words.alignment = .leading
        words.spacing = 4
        words.translatesAutoresizingMaskIntoConstraints = false
        let intro = NSView()
        intro.addSubview(icon)
        intro.addSubview(words)
        NSLayoutConstraint.activate([
            icon.widthAnchor.constraint(equalToConstant: 64),
            icon.heightAnchor.constraint(equalToConstant: 64),
            icon.leadingAnchor.constraint(equalTo: intro.leadingAnchor, constant: -6),
            icon.topAnchor.constraint(equalTo: intro.topAnchor, constant: -6),
            words.leadingAnchor.constraint(equalTo: intro.leadingAnchor, constant: 66),
            words.trailingAnchor.constraint(equalTo: intro.trailingAnchor),
            words.topAnchor.constraint(equalTo: intro.topAnchor),
            words.bottomAnchor.constraint(lessThanOrEqualTo: intro.bottomAnchor),
            intro.heightAnchor.constraint(greaterThanOrEqualToConstant: 52),
        ])
        return intro
    }

    private func checklist() -> NSView {
        var views: [NSView] = []
        for (index, row) in rows.enumerated() {
            if index > 0 { views.append(Hairline()) }
            views.append(view(for: row))
        }
        let stack = NSStackView(views: views)
        stack.orientation = .vertical
        stack.alignment = .leading
        stack.spacing = 0
        for view in views {
            view.widthAnchor.constraint(equalTo: stack.widthAnchor).isActive = true
        }
        return Group(stack)
    }

    private func footer() -> NSView {
        let note = Look.secondary("You can change all of these later in Settings.", width: 360)
        note.setContentHuggingPriority(.defaultLow, for: .horizontal)
        let done = Look.button("Done", current == nil ? .primary : .regular) { [weak self] in
            self?.window.performClose(nil)
        }
        let footer = NSStackView(views: [note, done])
        footer.orientation = .horizontal
        footer.distribution = .fill
        footer.alignment = .centerY
        footer.spacing = 12
        return footer
    }

    /// A row's mark, title and either its question or its answer.
    private func view(for row: Row) -> NSView {
        let isCurrent = row == current
        let (title, result, positive) = summary(of: row)
        let mark: Look.Mark =
            isCurrent ? .current : result == nil ? .todo : positive ? .ok : .off
        let heading = Look.label(
            title, size: 13, weight: .medium,
            color: result == nil && !isCurrent ? .nightcapSecondary : .labelColor, width: 300)
        heading.setContentHuggingPriority(.defaultLow, for: .horizontal)
        var line: [NSView] = [heading]
        if let result, !isCurrent {
            if row == .websites, websitesFromConfig != nil {
                line.append(Look.secondary(result, width: 180))
            } else {
                line.append(Look.secondary(result, width: 120))
                line.append(Look.link("Change") { [weak self] in
                    self?.current = row
                    self?.draw()
                })
            }
        }
        let header = NSStackView(views: line)
        header.orientation = .horizontal
        header.distribution = .fill
        header.alignment = .centerY
        header.spacing = 8
        var column: [NSView] = [header]
        if isCurrent {
            let (body, actions) = question(for: row)
            let text = Look.secondary(body, width: Self.textWidth)
            let spacer = NSView()
            spacer.setContentHuggingPriority(.defaultLow, for: .horizontal)
            let buttons = NSStackView(views: [spacer] + actions)
            buttons.orientation = .horizontal
            buttons.distribution = .fill
            buttons.spacing = 8
            column += [text, buttons]
        }
        let words = NSStackView(views: column)
        words.orientation = .vertical
        words.alignment = .leading
        words.spacing = 0
        words.widthAnchor.constraint(equalToConstant: Self.textWidth).isActive = true
        if column.count > 1 {
            words.setCustomSpacing(6, after: column[0])
            words.setCustomSpacing(12, after: column[1])
        }
        for view in column {
            view.widthAnchor.constraint(equalTo: words.widthAnchor).isActive = true
        }
        // A point lower, centring the mark on the title's line.
        let markView = Look.padded(Look.mark(mark), top: 1, left: 0, bottom: 0, right: 0)
        let content = NSStackView(views: [markView, words])
        content.orientation = .horizontal
        content.alignment = .top
        content.spacing = 12
        return Tint(Look.padded(content, top: 12, bottom: 12), tinted: isCurrent)
    }

    /// A row's title, and its answer once it has one, with whether that answer is a yes.
    private func summary(of row: Row) -> (String, String?, Bool) {
        switch row {
        case .notifications:
            let result: String? =
                switch answers.notifications {
                case .allowed: "Allowed"
                case .notAllowed: "Not allowed"
                case .notNow: "Not now"
                case nil: nil
                }
            return ("Notifications", result, answers.notifications == .allowed)
        case .websites:
            let result = answers.websites.map { $0 ? "On" : "Off" }
            return (
                "Website checking",
                websitesFromConfig == nil ? result : result.map { "\($0), from your config" },
                answers.websites == true
            )
        case .accessibility:
            return ("Accessibility", answers.accessibility.map { $0 ? "Allowed" : "Not now" }, answers.accessibility == true)
        case .openAtLogin:
            return ("Open at login", answers.openAtLogin.map { $0 ? "On" : "Not now" }, answers.openAtLogin == true)
        }
    }

    /// The current row's explanation and its buttons, the one to press last.
    private func question(for row: Row) -> (String, [NSButton]) {
        switch row {
        case .notifications:
            let purpose = "When a change made outside Nightcap overrides one of your rules, Nightcap tells you what it took the change to mean."
            let notNow = Look.button("Not Now", .regular) { [weak self] in
                self?.answer { $0.notifications = .notNow }
            }
            // macOS shows its prompt only once, so after a refusal only System Settings can.
            guard notifications != .denied else {
                return (
                    purpose + " Notifications are switched off for Nightcap in System Settings.",
                    [notNow, Look.button("Open Notification Settings…", .primary) {
                        let pane = "x-apple.systempreferences:com.apple.Notifications-Settings.extension?id=\(Bundle.main.bundleIdentifier!)"
                        NSWorkspace.shared.open(URL(string: pane)!)
                    }]
                )
            }
            return (purpose, [notNow, Look.button("Allow Notifications", .primary) { [weak self] in
                self?.allowNotifications()
            }])
        case .websites:
            return (
                "Nightcap can also force Night Shift off on websites you choose, in Safari and Chrome. It reads the front tab’s address with Accessibility and keeps only the site’s name.",
                [
                    Look.button("Not Now", .regular) { [weak self] in
                        self?.answer {
                            $0.websites = false
                            $0.accessibility = nil
                        }
                    },
                    Look.button("Turn On", .primary) { [weak self] in
                        guard let self else { return }
                        let granted = accessibilityGranted()
                        answer {
                            $0.websites = true
                            if granted { $0.accessibility = true }
                        }
                    },
                ]
            )
        case .accessibility:
            guard !waitingForAccessibility else {
                let waiting = Look.button("Waiting…", .regular) {}
                waiting.isEnabled = false
                return (
                    "Waiting for System Settings. Switch Nightcap on under Privacy & Security → Accessibility; this row turns green the moment it’s allowed.",
                    [waiting]
                )
            }
            return (
                "Website checking needs Accessibility. Allow Nightcap in System Settings → Privacy & Security → Accessibility.",
                [
                    Look.button("Not Now", .regular) { [weak self] in
                        self?.answer { $0.accessibility = false }
                    },
                    Look.button("Allow Accessibility…", .primary) { [weak self] in
                        guard let self else { return }
                        waitingForAccessibility = true
                        askForAccessibility()
                        draw()
                    },
                ]
            )
        case .openAtLogin:
            return (
                "Nightcap can open when you log in, so your rules apply from the start. You can change this later in Settings.",
                [
                    Look.button("Not Now", .regular) { [weak self] in
                        self?.chooseOpenAtLogin(false)
                        self?.answer { $0.openAtLogin = false }
                    },
                    Look.button("Open at Login", .primary) { [weak self] in
                        self?.chooseOpenAtLogin(true)
                        self?.answer { $0.openAtLogin = true }
                    },
                ]
            )
        }
    }

    private func allowNotifications() {
        Task {
            do {
                _ = try await UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound])
            } catch {
                log.error("couldn't request notification permission: \(error, privacy: .public)")
            }
            let status = await UNUserNotificationCenter.current().notificationSettings().authorizationStatus
            notifications = status
            onNotificationsRead()
            answer { $0.notifications = status == .authorized || status == .provisional ? .allowed : .notAllowed }
        }
    }
}
