import AppKit
import NightcapCore
import UserNotifications
import os

/// Onboarding (ADR-0011, ADR-0012, ADR-0014): one step at a time under a stage that illustrates
/// it. Night Shift comes first, set up or kept as it is, then Nightcap's own steps. Closing the
/// window finishes onboarding with what was chosen so far. Quitting Nightcap leaves it
/// unfinished.
final class Onboarding: NSObject, NSWindowDelegate {
    enum Step: Equatable {
        /// Night Shift is on a schedule already: the user keeps it, or reviews it.
        case alreadySetUp
        case strength, schedule, apps, websites
        /// Notifications and opening at login.
        case lastThings
        case ready
    }

    /// The websites step asks first, then for Accessibility, then for the sites.
    enum WebsitesPage: Equatable {
        case ask, accessibility, sites
    }

    /// Apps that need true colour, suggested when they're installed, each with a word for what
    /// it's for.
    private static let suggestedApps: [(bundleId: String, kind: String)] = [
        ("com.figma.Desktop", "Design"),
        ("com.bohemiancoding.sketch3", "Design"),
        ("com.adobe.illustrator", "Design"),
        ("com.adobe.Photoshop", "Photo"),
        ("com.adobe.LightroomClassicCC7", "Photo"),
        ("com.pixelmatorteam.pixelmator.x", "Photo"),
        ("com.apple.Photos", "Photo"),
        ("com.apple.FinalCut", "Video editing"),
        ("com.blackmagic-design.DaVinciResolve", "Video editing"),
        ("com.apple.iMovieApp", "Video editing"),
        ("com.apple.TV", "Video"),
        ("com.apple.QuickTimePlayerX", "Video"),
        ("com.colliderli.iina", "Video"),
        ("org.videolan.vlc", "Video"),
        ("com.firecore.infuse", "Video"),
    ]
    /// Websites that need true colour, mostly video streaming, none ticked.
    private static let suggestedSites: [(name: String, host: String)] = [
        ("YouTube", "youtube.com"),
        ("Netflix", "netflix.com"),
        ("Twitch", "twitch.tv"),
        ("Disney+", "disneyplus.com"),
        ("Prime Video", "primevideo.com"),
        ("Vimeo", "vimeo.com"),
        ("Figma", "figma.com"),
        ("Canva", "canva.com"),
    ]
    /// Three rows of three fit the step; one place is kept for an app the user chooses.
    private static let gridSize = 9
    private static let nightShiftSteps: [Step] = [.alreadySetUp, .strength, .schedule]
    private static let nightcapSteps: [Step] = [.apps, .websites, .lastThings]
    private static let size = NSSize(width: 760, height: 540)
    private static let stageHeight: CGFloat = 224
    private static let margin: CGFloat = 36
    private static let textWidth = size.width - margin * 2

    private let window = NSWindow(
        contentRect: NSRect(origin: .zero, size: size),
        styleMask: [.titled, .closable, .fullSizeContentView],
        backing: .buffered,
        defer: false
    )
    private let log = Logger(subsystem: Bundle.main.bundleIdentifier!, category: "app")
    private let setup: OnboardingSteps
    private let accessibilityGranted: () -> Bool
    private let askForAccessibility: () -> Void
    private let choose: (Command) -> Void
    private let onFinish: (_ websites: Bool) -> Void
    private let onNotificationsRead: () -> Void
    private let sun: Sun?
    /// The window's lasting content: the stage, whose loops would restart if it were rebuilt, and
    /// the page under it, which is.
    private var stage: Stage?
    private var pageArea: NSView?
    /// The apps suggested on this Mac, then any the user chose; the ones with a rule already come
    /// ticked and can't be changed.
    private var apps: [GridItem]
    private var tickedApps: Set<String> = []
    private let icons: SiteIcons
    /// Where each existing rule comes from, such as "From your config".
    private let ruled: [RuleIdentity: String]
    private var tickedSites: Set<String> = []
    /// A redraw for arriving icons is on its way.
    private var redrawing = false
    /// The suggested sites, with their icons as far as they've arrived.
    private var sites: [GridItem] {
        Self.suggestedSites.map { site in
            GridItem(
                site: site.name, host: site.host, icon: icons.icon(for: site.host), ruled: ruled[.domain(site.host)])
        }
    }
    private var index = 0
    /// The furthest step reached, whose dot and those before it can be clicked.
    private var reached = 0
    /// Someone who already uses Night Shift chose to look over its settings.
    private var reviewing = false
    private var page = WebsitesPage.ask
    private var waitingForAccessibility = false
    private var websites: Bool?
    private var closedByUser = false
    /// The strength as last set here or read from the Mac, 0 to 100.
    private var strength: UInt8
    /// The schedule the user picked, which Continue writes; it starts at the Mac's, or at sunset to
    /// sunrise where Night Shift has none.
    private var schedule: Schedule
    private var customHours: (from: TimeOfDay, to: TimeOfDay)
    private weak var previewNote: NSTextField?
    private var radios: [(button: NSButton, schedule: () -> Schedule)] = []
    private var addedApps = 0
    /// The first app given a rule here, which the last step's notification names.
    private var firstApp: String?
    private var addedSites = 0
    private var notifications = UNAuthorizationStatus.notDetermined
    private var wantsNotifications = false
    /// Whether Nightcap is to open at login; nil where it never opens itself.
    private var opensAtLogin: Bool?
    private var poll: Timer?

    /// `choose` sends a command to the core, as the menu does; `onFinish` runs once onboarding
    /// ends, by Done or the close button.
    init(
        steps setup: OnboardingSteps,
        rules: [SettingsRule],
        icons: SiteIcons,
        sun: Sun?,
        accessibilityGranted: @escaping () -> Bool,
        askForAccessibility: @escaping () -> Void,
        choose: @escaping (Command) -> Void,
        onNotificationsRead: @escaping () -> Void,
        onFinish: @escaping (_ websites: Bool) -> Void
    ) {
        self.setup = setup
        self.accessibilityGranted = accessibilityGranted
        self.askForAccessibility = askForAccessibility
        self.choose = choose
        self.onFinish = onFinish
        self.onNotificationsRead = onNotificationsRead
        self.sun = sun
        opensAtLogin = setup.opensAtLogin
        self.icons = icons
        let ruled = Dictionary(rules.map { ($0.identity, $0.label) }, uniquingKeysWith: { first, _ in first })
        self.ruled = ruled
        apps = Self.suggestedApps.compactMap { suggestion in
            GridItem(app: suggestion.bundleId, kind: suggestion.kind, ruled: ruled[.app(suggestion.bundleId)])
        }.prefix(Self.gridSize - 1).map { $0 }
        strength = setup.nightShift.strength ?? 50
        let current = setup.nightShift.schedule ?? .off
        if case .custom(let from, let to) = current {
            customHours = (from, to)
        } else {
            customHours = (TimeOfDay(hour: 22, minute: 0), TimeOfDay(hour: 7, minute: 0))
        }
        schedule = current == .off ? .sunsetToSunrise : current
        super.init()
        window.title = "Welcome to Nightcap"
        window.titleVisibility = .hidden
        window.titlebarAppearsTransparent = true
        window.isMovableByWindowBackground = true
        window.isReleasedWhenClosed = false
        window.delegate = self
        // An empty toolbar makes the title bar the stage's title band, with the traffic lights
        // centred in it beside the wordmark.
        window.toolbar = NSToolbar(identifier: "onboarding")
        window.toolbarStyle = .unifiedCompact
        window.titlebarSeparatorStyle = .none
        // The config answers website checking for good; it can still need Accessibility.
        if setup.websitesFromConfig == true {
            websites = true
            page = accessibilityGranted() ? .sites : .accessibility
        }
        draw()
        window.center()
        Task { await readNotifications() }
        // macOS posts nothing when notifications are switched in System Settings, so they're
        // re-read every second while onboarding is open.
        let poll = Timer(timeInterval: 1, repeats: true) { [weak self] _ in
            MainActor.assumeIsolated {
                _ = Task { await self?.readNotifications() }
            }
        }
        poll.tolerance = 0.25
        RunLoop.main.add(poll, forMode: .common)
        self.poll = poll
    }

    #if DEBUG
    /// Notifications as snapshots show them, whatever this Mac's permission is.
    private var frozen = false
    #endif

    private func readNotifications() async {
        let status = await UNUserNotificationCenter.current().notificationSettings().authorizationStatus
        #if DEBUG
        if frozen { return }
        #endif
        onNotificationsRead()
        guard status != notifications else { return }
        notifications = status
        // Only the last step's rows depend on it; redrawing elsewhere could end a drag.
        if step == .lastThings || step == .ready { draw() }
    }

    private var notificationsAllowed: Bool {
        notifications == .authorized || notifications == .provisional
    }

    /// Opening a menu-bar-only app doesn't activate it, and macOS refuses the app's own request
    /// at launch, however long it waits (measured on macOS 27). Only the forcing call, which
    /// Apple means to deprecate, works. A click on Nightcap's menu or Dock icon activates it
    /// without one.
    func present() {
        NSApp.activate(ignoringOtherApps: true)
        window.makeKeyAndOrderFront(nil)
    }

    /// The Accessibility page moves on once the grant arrives, a moment after showing it.
    func accessibilityChanged() {
        guard step == .websites, page == .accessibility, accessibilityGranted() else { return }
        waitingForAccessibility = false
        draw()
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.8) { [weak self] in
            guard let self, step == .websites, page == .accessibility else { return }
            page = .sites
            draw()
        }
    }

    /// Asked only when the user closes the window, never when Nightcap quits.
    func windowShouldClose(_ sender: NSWindow) -> Bool {
        closedByUser = true
        return true
    }

    func windowWillClose(_ notification: Notification) {
        poll?.invalidate()
        if closedByUser { onFinish(websites == true) }
    }

    #if DEBUG
    /// The window as a PNG at `step`, for comparing against the prototype.
    func snapshot(_ step: Step, page: WebsitesPage = .ask, reviewing: Bool = false) -> Data? {
        frozen = true
        notifications = .notDetermined
        self.reviewing = reviewing
        guard let index = steps.firstIndex(of: step) else { return nil }
        self.index = index
        reached = index
        self.page = page
        draw()
        window.alphaValue = 0
        window.orderFront(nil)
        // The frame view, so the traffic lights come out too.
        guard let view = window.contentView?.superview else { return nil }
        view.layoutSubtreeIfNeeded()
        view.displayIfNeeded()
        guard let bitmap = view.bitmapImageRepForCachingDisplay(in: view.bounds) else { return nil }
        view.cacheDisplay(in: view.bounds, to: bitmap)
        return bitmap.representation(using: .png, properties: [:])
    }
    #endif

    // MARK: The steps

    /// The steps this user takes, in order: Night Shift's unless the config sets it, then
    /// Nightcap's.
    private var steps: [Step] {
        let nightShift = setup.nightShift
        var steps: [Step] = []
        if nightShift.alreadySetUp && !reviewing {
            steps.append(.alreadySetUp)
        } else {
            if nightShift.askStrength { steps.append(.strength) }
            if nightShift.askSchedule { steps.append(.schedule) }
        }
        steps.append(.apps)
        if setup.websitesFromConfig != false { steps.append(.websites) }
        if !notificationsAllowed || opensAtLogin != nil { steps.append(.lastThings) }
        steps.append(.ready)
        return steps
    }

    private var step: Step { steps[index] }

    private func go(to index: Int) {
        self.index = index
        reached = max(reached, index)
        draw()
    }

    private func advance() {
        go(to: index + 1)
    }

    private func goToNightcap() {
        go(to: steps.firstIndex { Self.nightcapSteps.contains($0) }!)
    }

    /// Where Back goes: an earlier page of the websites step, or the step before.
    private var back: (() -> Void)? {
        if step == .websites, page != .ask, setup.websitesFromConfig == nil {
            return { [weak self] in
                self?.page = .ask
                self?.draw()
            }
        }
        if index == 0, reviewing {
            return { [weak self] in
                self?.reviewing = false
                self?.go(to: 0)
            }
        }
        guard index > 0 else { return nil }
        return { [weak self] in
            guard let self else { return }
            go(to: index - 1)
        }
    }

    /// Redraws the sites page with the icons that just arrived, which come close together, once.
    func iconArrived() {
        guard step == .websites, page == .sites, !redrawing else { return }
        redrawing = true
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.25) { [weak self] in
            guard let self else { return }
            redrawing = false
            if step == .websites, page == .sites { draw() }
        }
    }

    // MARK: Drawing

    /// Shows where onboarding is now: the stage's scene, which changes only with it, and the
    /// page, rebuilt each time. The window keeps one size.
    private func draw() {
        if stage == nil { build() }
        guard let stage, let pageArea else { return }
        stage.strength = strength
        stage.show(scene)
        pageArea.subviews.forEach { $0.removeFromSuperview() }
        let page = pageView()
        page.translatesAutoresizingMaskIntoConstraints = false
        pageArea.addSubview(page)
        NSLayoutConstraint.activate([
            page.leadingAnchor.constraint(equalTo: pageArea.leadingAnchor),
            page.trailingAnchor.constraint(equalTo: pageArea.trailingAnchor),
            page.topAnchor.constraint(equalTo: pageArea.topAnchor),
            page.bottomAnchor.constraint(equalTo: pageArea.bottomAnchor),
        ])
    }

    private func build() {
        let content = Backdrop()
        let band = window.frame.height - window.contentLayoutRect.height
        let stage = Stage(band: band, height: Self.stageHeight, sun: sun)
        let pageArea = NSView()
        for view in [stage, pageArea] {
            view.translatesAutoresizingMaskIntoConstraints = false
            content.addSubview(view)
        }
        NSLayoutConstraint.activate([
            stage.leadingAnchor.constraint(equalTo: content.leadingAnchor),
            stage.trailingAnchor.constraint(equalTo: content.trailingAnchor),
            stage.topAnchor.constraint(equalTo: content.topAnchor),
            stage.heightAnchor.constraint(equalToConstant: Self.stageHeight),
            pageArea.leadingAnchor.constraint(equalTo: content.leadingAnchor),
            pageArea.trailingAnchor.constraint(equalTo: content.trailingAnchor),
            pageArea.topAnchor.constraint(equalTo: stage.bottomAnchor),
            pageArea.bottomAnchor.constraint(equalTo: content.bottomAnchor),
        ])
        window.contentView = content
        self.stage = stage
        self.pageArea = pageArea
    }

    /// The step's illustration.
    private var scene: Scene {
        switch step {
        case .alreadySetUp: .timeline(setup.nightShift.schedule ?? .sunsetToSunrise)
        case .strength: .strength
        case .schedule: schedule == .off ? .byHand : .timeline(schedule)
        case .apps: .apps
        case .websites:
            switch page {
            case .ask: .address
            case .accessibility: .accessibility(granted: accessibilityGranted())
            case .sites: .sites
            }
        case .lastThings: .lastThings(app: firstApp ?? "Figma")
        case .ready: .ready
        }
    }

    /// The step's words and controls, over its footer.
    private func pageView() -> NSView {
        let words = self.words
        var column: [NSView] = [
            Look.label(words.eyebrow, size: 12, weight: .semibold, color: .nightcapMoon, width: Self.textWidth),
            Look.label(words.heading, size: 20, weight: .semibold, color: .labelColor, width: Self.textWidth),
            Look.label(words.lede, size: 13, weight: .regular, color: .nightcapSecondary, width: Self.textWidth),
        ]
        column += controls
        let stack = NSStackView(views: column)
        stack.orientation = .vertical
        stack.alignment = .leading
        stack.spacing = 8
        stack.setCustomSpacing(2, after: column[0])
        stack.setCustomSpacing(6, after: column[1])
        stack.setCustomSpacing(14, after: column[2])
        let footer = footerView()
        let page = NSView()
        for view in [stack, footer] {
            view.translatesAutoresizingMaskIntoConstraints = false
            page.addSubview(view)
        }
        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(equalTo: page.leadingAnchor, constant: Self.margin),
            stack.trailingAnchor.constraint(equalTo: page.trailingAnchor, constant: -Self.margin),
            stack.topAnchor.constraint(equalTo: page.topAnchor, constant: 20),
            footer.leadingAnchor.constraint(equalTo: page.leadingAnchor, constant: Self.margin),
            footer.trailingAnchor.constraint(equalTo: page.trailingAnchor, constant: -Self.margin),
            footer.bottomAnchor.constraint(equalTo: page.bottomAnchor, constant: -16),
            stack.bottomAnchor.constraint(lessThanOrEqualTo: footer.topAnchor, constant: -12),
        ])
        return page
    }

    /// The step's controls between its words and its footer.
    private var controls: [NSView] {
        switch step {
        case .alreadySetUp:
            return [
                rows([
                    summary("Strength", Self.warmth(strength)),
                    summary("Schedule", Self.describe(schedule)),
                ]),
                Look.secondary(
                    "Skipping leaves them exactly as they are. "
                        + "You can change them any time in System Settings › Displays.",
                    width: Self.textWidth),
            ]
        case .strength:
            return strengthControls()
        case .schedule:
            return scheduleControls()
        case .apps where apps.isEmpty:
            let note = Look.secondary(
                "Nightcap looks for apps like Figma, Photoshop, Final Cut Pro and DaVinci Resolve, "
                    + "and found none on this Mac. "
                    + "Choose one yourself, or add one later from the menu while it’s in front.",
                width: Self.textWidth - 150)
            note.setContentHuggingPriority(.defaultLow, for: .horizontal)
            let line = NSStackView(views: [
                note, Look.button("Choose App…", .regular) { [weak self] in self?.chooseApp() },
            ])
            line.orientation = .horizontal
            line.distribution = .fill
            line.spacing = 14
            return [rows([Look.padded(line, top: 12, left: 14, bottom: 12, right: 14)])]
        case .apps:
            return [
                grid(
                    apps.map { app in
                        GridCell(app, ticked: app.ruled != nil || tickedApps.contains(app.id)) { [weak self] in
                            guard let self else { return }
                            if tickedApps.remove(app.id) == nil { tickedApps.insert(app.id) }
                            draw()
                        }
                    })
            ]
        case .websites:
            return websitesControls()
        case .lastThings:
            var options: [NSView] = []
            if !notificationsAllowed {
                let title = "Tell me when a rule is snoozed"
                let detail =
                    "If you turn Night Shift on in Control Center while a rule holds it off, "
                    + "Nightcap snoozes the rule until the schedule’s next change, and says so in a notification."
                // macOS shows its prompt only once, so after a refusal only System Settings can.
                options.append(
                    notifications == .denied
                        ? option(
                            title, detail + " Notifications are switched off for Nightcap in System Settings.",
                            button: Look.button("Open Notification Settings…", .regular) {
                                let pane =
                                    "x-apple.systempreferences:com.apple.Notifications-Settings.extension"
                                    + "?id=\(Bundle.main.bundleIdentifier!)"
                                NSWorkspace.shared.open(URL(string: pane)!)
                            })
                        : option(title, detail, ticked: wantsNotifications) { [weak self] in
                            self?.wantsNotifications = $0
                        })
            }
            if let opensAtLogin {
                options.append(
                    option(
                        "Open at login",
                        "So your rules apply from the start. Nightcap lives in the menu bar, with no Dock icon.",
                        ticked: opensAtLogin
                    ) { [weak self] in self?.opensAtLogin = $0 })
            }
            return [rows(options)]
        case .ready:
            return [summaryGrid()]
        }
    }

    /// A choice with a checkbox, unticked unless it's on already.
    private func option(_ title: String, _ detail: String, ticked: Bool, toggle: @escaping (Bool) -> Void) -> NSView {
        let box = NSButton(checkboxWithTitle: title, target: nil, action: nil)
        box.font = .systemFont(ofSize: 13)
        box.state = ticked ? .on : .off
        Look.attach(box) { [weak box] in toggle(box?.state == .on) }
        return option(box, detail)
    }

    /// A choice that only a button outside Nightcap can make.
    private func option(_ title: String, _ detail: String, button: NSButton) -> NSView {
        let name = Look.label(title, size: 13, weight: .regular, color: .labelColor, width: 400)
        name.setContentHuggingPriority(.defaultLow, for: .horizontal)
        let line = NSStackView(views: [name, button])
        line.orientation = .horizontal
        line.distribution = .fill
        return option(line, detail)
    }

    private func option(_ top: NSView, _ detail: String) -> NSView {
        let note = Look.padded(
            Look.secondary(detail, width: Self.textWidth - 60), top: 0, left: 20, bottom: 0, right: 0)
        let column = NSStackView(views: [top, note])
        column.orientation = .vertical
        column.alignment = .leading
        column.spacing = 2
        top.widthAnchor.constraint(equalTo: column.widthAnchor).isActive = true
        return Look.padded(column, top: 10, left: 12, bottom: 10, right: 12)
    }

    /// What onboarding set up, two to a row.
    private func summaryGrid() -> NSView {
        let granted = accessibilityGranted()
        var lines: [(String, String, Bool)] = []
        if setup.nightShift.strength != nil { lines.append(("Strength", Self.warmth(strength), true)) }
        if setup.nightShift.schedule != nil { lines.append(("Schedule", Self.describe(schedule), true)) }
        lines.append(("Apps", Self.rules(addedApps, "rule"), addedApps > 0))
        switch websites {
        case true? where !granted: lines.append(("Websites", "Needs Accessibility", false))
        case true?: lines.append(("Websites", addedSites > 0 ? Self.rules(addedSites, "site") : "On", true))
        default: lines.append(("Websites", "Off", false))
        }
        let notificationsOn = notificationsAllowed
        lines.append(("Notifications", notificationsOn ? "On" : "Off", notificationsOn))
        if let opensAtLogin { lines.append(("Open at login", opensAtLogin ? "On" : "Off", opensAtLogin)) }
        var rowViews: [NSView] = []
        for start in stride(from: 0, to: lines.count, by: 2) {
            let cells = lines[start..<min(start + 2, lines.count)].enumerated().map { column, line in
                Divided(summary(line.0, line.1, done: line.2), left: column > 0) as NSView
            }
            let row = NSStackView(views: cells.count == 2 ? cells : cells + [NSView()])
            row.orientation = .horizontal
            row.distribution = .fillEqually
            row.spacing = 0
            rowViews.append(row)
        }
        return rows(rowViews)
    }

    private static func rules(_ count: Int, _ noun: String) -> String {
        count == 0 ? "None" : count == 1 ? "1 \(noun)" : "\(count) \(noun)s"
    }

    private func websitesControls() -> [NSView] {
        switch page {
        case .ask:
            return [
                rows([
                    fact(
                        symbol: "globe", "Safari, Chrome, Firefox and more.",
                        "Also Edge, Arc, Brave, Dia, Zen and Orion, in the front tab."),
                    fact(symbol: "accessibility", "Through Accessibility.", "macOS asks you to allow it, once."),
                    fact(
                        symbol: "lock", "Only the site’s name.",
                        "Like youtube.com; the rest of the address is dropped, and the icon comes from the site."),
                ])
            ]
        case .accessibility:
            let granted = accessibilityGranted()
            let asked = waitingForAccessibility || granted
            return [
                rows([
                    progress(
                        asked ? .ok : .current,
                        "Click Allow Accessibility, then Open System Settings when macOS asks."),
                    progress(
                        granted ? .ok : asked ? .current : .todo,
                        "Switch Nightcap on under Privacy & Security › Accessibility."),
                    progress(
                        granted ? .ok : .todo,
                        "That’s all. This step moves on by itself the moment Nightcap is allowed."),
                ])
            ]
        case .sites:
            // The suggestions are the same for everyone, so asking for their icons tells the
            // sites nothing about the user's browsing (ADR-0019).
            icons.ask(Self.suggestedSites.map(\.host))
            return [
                grid(
                    sites.map { site in
                        GridCell(site, ticked: site.ruled != nil || tickedSites.contains(site.id)) { [weak self] in
                            guard let self else { return }
                            if tickedSites.remove(site.id) == nil { tickedSites.insert(site.id) }
                            draw()
                        }
                    })
            ]
        }
    }

    /// A line of the websites step's facts: a glyph, a few words in bold, then the rest.
    private func fact(symbol: String, _ lead: String, _ rest: String) -> NSView {
        let glyph = NSImageView(
            image: NSImage(systemSymbolName: symbol, accessibilityDescription: nil)!
                .withSymbolConfiguration(NSImage.SymbolConfiguration(pointSize: 13, weight: .regular))!)
        glyph.contentTintColor = .nightcapSecondary
        glyph.widthAnchor.constraint(equalToConstant: 18).isActive = true
        let text = NSMutableAttributedString(
            attributedString: Look.label(
                "\(lead) \(rest)", size: 12, weight: .regular, color: .labelColor, width: Self.textWidth - 60
            )
            .attributedStringValue)
        text.addAttribute(
            .font, value: NSFont.systemFont(ofSize: 12, weight: .semibold),
            range: NSRange(location: 0, length: (lead as NSString).length))
        let label = NSTextField(labelWithAttributedString: text)
        let line = NSStackView(views: [glyph, label])
        line.orientation = .horizontal
        line.spacing = 10
        return Look.padded(line, top: 8, left: 12, bottom: 8, right: 12)
    }

    /// A line of the Accessibility checklist, marked as done, current or to do.
    private func progress(_ mark: Look.Mark, _ text: String) -> NSView {
        let line = NSStackView(views: [
            Look.mark(mark),
            Look.label(
                text, size: 12, weight: .regular, color: mark == .todo ? .nightcapSecondary : .labelColor,
                width: Self.textWidth - 60),
        ])
        line.orientation = .horizontal
        line.spacing = 10
        return Look.padded(line, top: 8, left: 12, bottom: 8, right: 12)
    }

    /// Cells three to a row in a group, divided by hairlines.
    private func grid(_ cells: [GridCell]) -> NSView {
        var lines: [NSView] = []
        for start in stride(from: 0, to: cells.count, by: 3) {
            var line = Array(cells[start..<min(start + 3, cells.count)]) as [NSView]
            while line.count < 3 { line.append(NSView()) }
            for (column, cell) in line.enumerated() {
                (cell as? GridCell)?.dividedOnLeft = column > 0
            }
            let row = NSStackView(views: line)
            row.orientation = .horizontal
            row.distribution = .fillEqually
            row.spacing = 0
            lines.append(row)
        }
        return rows(lines)
    }

    /// An app from Applications, like Settings' "Add App…", added to the grid ticked.
    private func chooseApp() {
        let panel = NSOpenPanel()
        panel.allowedContentTypes = [.application]
        panel.directoryURL = URL(fileURLWithPath: "/Applications")
        panel.prompt = "Choose"
        panel.message = "Choose an app that needs true colour."
        panel.beginSheetModal(for: window) { [weak self] response in
            guard let self, response == .OK, let url = panel.url,
                let bundleId = Bundle(url: url)?.bundleIdentifier
            else { return }
            if !apps.contains(where: { $0.id == bundleId }),
                let item = GridItem(app: bundleId, kind: "Chosen by you", ruled: nil)
            {
                apps.append(item)
            }
            tickedApps.insert(bundleId)
            draw()
        }
    }

    /// The slider previews the strength on the screen while it's dragged, and saves it on release,
    /// as the menu's does.
    private func strengthControls() -> [NSView] {
        let slider = StrengthSlider(value: Double(strength), minValue: 0, maxValue: 100, target: nil, action: nil)
        slider.isContinuous = true
        slider.setAccessibilityLabel("Strength")
        Look.attach(slider) { [weak self, weak slider] in
            guard let self, let slider else { return }
            strength = UInt8(slider.doubleValue.rounded())
            choose(slider.dragging ? .previewStrength(strength) : .strength(strength))
            stage?.strength = strength
            showPreview(slider.dragging)
        }
        slider.onRelease = { [weak self, weak slider] in
            guard let self, let slider else { return }
            strength = UInt8(slider.doubleValue.rounded())
            choose(.strength(strength))
            showPreview(false)
        }
        let less = Look.secondary("Less Warm", width: 80)
        let more = Look.secondary("More Warm", width: 80)
        let line = NSStackView(views: [less, slider, more])
        line.orientation = .horizontal
        line.spacing = 12
        let note = Look.secondary("Drag the slider to try it on your screen.", width: Self.textWidth)
        previewNote = note
        let facts = NSStackView(views: [
            fact("Part of macOS", "In System Settings › Displays, and in Control Center."),
            fact("Colour, not brightness", "It warms the colours. Brightness stays where it is."),
            fact("By hand or on a schedule", "You choose which on the next step."),
        ])
        facts.orientation = .horizontal
        facts.alignment = .top
        facts.distribution = .fillEqually
        facts.spacing = 20
        facts.widthAnchor.constraint(equalToConstant: Self.textWidth).isActive = true
        return [rows([Look.padded(line, top: 10, left: 14, bottom: 10, right: 14)]), note, facts]
    }

    /// Says so while the screen shows the strength, which it does only during a drag.
    private func showPreview(_ previewing: Bool) {
        guard let previewNote else { return }
        let text =
            previewing ? "Your screen is showing this strength now." : "Drag the slider to try it on your screen."
        let replacement = Look.label(
            text, size: 12, weight: .regular, color: previewing ? .nightcapMoon : .nightcapSecondary,
            width: Self.textWidth)
        previewNote.attributedStringValue = replacement.attributedStringValue
    }

    private func fact(_ title: String, _ text: String) -> NSView {
        let width = (Self.textWidth - 40) / 3
        let stack = NSStackView(views: [
            Look.label(title, size: 12, weight: .semibold, color: .labelColor, width: width),
            Look.label(text, size: 11, weight: .regular, color: .nightcapSecondary, width: width),
        ])
        stack.orientation = .vertical
        stack.alignment = .leading
        stack.spacing = 2
        return stack
    }

    /// Radio buttons for the schedule. They sit in rows of their own, so they're kept in step by
    /// hand, without redrawing, which would take the focus from the time fields.
    private func scheduleControls() -> [NSView] {
        radios = []
        let custom = { [weak self] () -> Schedule in
            guard let self else { return .off }
            return .custom(from: customHours.from, to: customHours.to)
        }
        let hours = NSStackView(views: [
            timeField(customHours.from) { [weak self] in self?.customHours.from = $0 },
            Look.secondary("to", width: 20),
            timeField(customHours.to) { [weak self] in self?.customHours.to = $0 },
        ])
        hours.spacing = 6
        return [
            rows([
                radioRow(
                    "On a schedule: sunset to sunrise", detail: Look.secondary("Follows your location", width: 200)
                ) {
                    .sunsetToSunrise
                },
                radioRow("On a schedule: custom hours", detail: hours, custom),
                radioRow("By hand", detail: Look.secondary("Only when you turn it on", width: 200)) { .off },
            ]),
            Look.secondary(
                "Either way, you can turn it on or off any time from Control Center or Nightcap’s menu.",
                width: Self.textWidth),
        ]
    }

    private func radioRow(_ title: String, detail: NSView, _ schedule: @escaping () -> Schedule) -> NSView {
        let button = NSButton(radioButtonWithTitle: title, target: nil, action: nil)
        button.font = .systemFont(ofSize: 13)
        let chosen = schedule()
        button.state = Self.sameKind(chosen, self.schedule) ? .on : .off
        Look.attach(button) { [weak self] in self?.pick(schedule()) }
        radios.append((button, schedule))
        button.setContentHuggingPriority(.defaultLow, for: .horizontal)
        let line = NSStackView(views: [button, detail])
        line.orientation = .horizontal
        line.distribution = .fill
        line.spacing = 12
        return Look.padded(line, top: 8, left: 12, bottom: 8, right: 12)
    }

    private func pick(_ schedule: Schedule) {
        self.schedule = schedule
        for radio in radios {
            radio.button.state = Self.sameKind(radio.schedule(), schedule) ? .on : .off
        }
        stage?.show(scene)
    }

    /// An hour and minute field; editing it picks custom hours.
    private func timeField(_ time: TimeOfDay, set: @escaping (TimeOfDay) -> Void) -> NSDatePicker {
        let picker = NSDatePicker()
        picker.datePickerStyle = .textField
        picker.datePickerElements = .hourMinute
        picker.font = .systemFont(ofSize: 12)
        picker.dateValue = Calendar.current.date(
            bySettingHour: Int(time.hour), minute: Int(time.minute), second: 0, of: Date())!
        Look.attach(picker) { [weak self, weak picker] in
            guard let self, let picker else { return }
            let parts = Calendar.current.dateComponents([.hour, .minute], from: picker.dateValue)
            set(TimeOfDay(hour: UInt8(parts.hour!), minute: UInt8(parts.minute!)))
            pick(.custom(from: customHours.from, to: customHours.to))
        }
        return picker
    }

    private static func sameKind(_ a: Schedule, _ b: Schedule) -> Bool {
        switch (a, b) {
        case (.off, .off), (.sunsetToSunrise, .sunsetToSunrise), (.custom, .custom): true
        default: false
        }
    }

    /// Rows in a group, divided by hairlines, as wide as the text.
    private func rows(_ views: [NSView]) -> NSView {
        var arranged: [NSView] = []
        for (index, view) in views.enumerated() {
            if index > 0 { arranged.append(Hairline()) }
            arranged.append(view)
        }
        let stack = NSStackView(views: arranged)
        stack.orientation = .vertical
        stack.alignment = .leading
        stack.spacing = 0
        for view in arranged {
            view.widthAnchor.constraint(equalTo: stack.widthAnchor).isActive = true
        }
        let group = Group(stack)
        group.widthAnchor.constraint(equalToConstant: Self.textWidth).isActive = true
        return group
    }

    private func summary(_ title: String, _ value: String, done: Bool = true) -> NSView {
        let name = Look.label(title, size: 13, weight: .regular, color: .labelColor, width: 300)
        name.setContentHuggingPriority(.defaultLow, for: .horizontal)
        let line = NSStackView(views: [Look.mark(done ? .ok : .off), name, Look.secondary(value, width: 300)])
        line.orientation = .horizontal
        line.distribution = .fill
        line.spacing = 10
        return Look.padded(line, top: 8, left: 12, bottom: 8, right: 12)
    }

    private func askForNotifications() {
        Task {
            do {
                _ = try await UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound])
            } catch {
                log.error("couldn't request notification permission: \(error, privacy: .public)")
            }
            await readNotifications()
        }
    }

    private static func addRules(_ count: Int) -> String {
        count == 0 ? "Add Rules" : count == 1 ? "Add 1 Rule" : "Add \(count) Rules"
    }

    /// The strength in words, since System Settings shows no number either.
    private static func warmth(_ strength: UInt8) -> String {
        strength < 34 ? "A little warm" : strength < 67 ? "Warm" : "Very warm"
    }

    private static func describe(_ schedule: Schedule) -> String {
        switch schedule {
        case .off: "By hand"
        case .sunsetToSunrise: "Sunset to sunrise"
        case .custom(let from, let to): "\(clock(from)) to \(clock(to))"
        }
    }

    private static func clock(_ time: TimeOfDay) -> String {
        String(format: "%02d:%02d", time.hour, time.minute)
    }

    private var words: (eyebrow: String, heading: String, lede: String) {
        let eyebrow = self.eyebrow
        switch step {
        case .alreadySetUp:
            return (
                eyebrow, "You already use Night Shift",
                "Nightcap found your Night Shift settings. "
                    + "Keep them as they are and go straight to Nightcap, or look over them first."
            )
        case .strength:
            return reviewing
                ? (eyebrow, "How warm Night Shift gets", "This is your current strength. Drag to change it.")
                : (
                    eyebrow, "This is Night Shift",
                    "Night Shift is part of macOS. "
                        + "It shifts your screen toward warmer colours, which many people prefer in the evening. "
                        + "Choose how warm it gets."
                )
        case .schedule:
            return (
                eyebrow, "When should it come on?",
                "Night Shift can come on by itself on a schedule, or only when you turn it on."
            )
        case .apps:
            return (
                eyebrow, "Night Shift, with exceptions",
                apps.isEmpty
                    ? "Warm is wrong for photos, video and design. "
                        + "Nightcap turns Night Shift off while an app you choose is in front, "
                        + "and gives it back when you leave."
                    : "Warm is wrong for photos, video and design. "
                        + "Nightcap turns Night Shift off while an app you tick is in front, "
                        + "and gives it back when you leave. These are on your Mac."
            )
        case .websites:
            switch page {
            case .ask:
                return (
                    eyebrow, "Websites too?",
                    "Nightcap can do the same for websites: "
                        + "Night Shift goes off while a site you choose is in the front tab."
                )
            case .accessibility:
                return (
                    eyebrow, "Allow Accessibility",
                    "Nightcap reads the front tab’s address through Accessibility, which macOS asks you to allow once."
                )
            case .sites:
                return (
                    eyebrow, "Which websites need true colour?",
                    "Tick any that do. You can add any other site later from the menu while it’s open."
                )
            }
        case .lastThings:
            return (eyebrow, "Two last things", "Both are up to you, and you can change either later in Settings.")
        case .ready:
            return (
                eyebrow, "Nightcap is ready",
                "It lives in the menu bar, under the moon. "
                    + "Its menu turns Night Shift on or off, everywhere or just in the app in front, "
                    + "for as long as you choose. "
                    + "You can change any of this later in Settings, along with the shortcut."
            )
        }
    }

    /// The step's group and place in it, in the current step's gold: "Nightcap · 2 of 3".
    private var eyebrow: String {
        guard step != .ready else { return "Done" }
        let isNightShift = Self.nightShiftSteps.contains(step)
        let group = steps.filter { (isNightShift ? Self.nightShiftSteps : Self.nightcapSteps).contains($0) }
        let name = isNightShift ? "Night Shift" : "Nightcap"
        guard group.count > 1, let place = group.firstIndex(of: step) else { return name }
        return "\(name) · \(place + 1) of \(group.count)"
    }

    private func footerView() -> NSView {
        let actions = self.actions
        var leading: [NSView] = []
        if let back { leading.append(Look.button("Back", .regular, back)) }
        if let link = actions.link { leading.append(Look.link(link.title, link.run)) }
        var trailing: [NSView] = []
        if let secondary = actions.secondary { trailing.append(Look.button(secondary.title, .regular, secondary.run)) }
        let primary = Look.button(actions.primary.title, .primary, actions.primary.run)
        primary.isEnabled = actions.primary.enabled
        primary.keyEquivalent = "\r"
        trailing.append(primary)
        let start = NSStackView(views: leading)
        start.spacing = 8
        let end = NSStackView(views: trailing)
        end.spacing = 8
        let footer = NSView()
        for view in [start, end] {
            view.translatesAutoresizingMaskIntoConstraints = false
            footer.addSubview(view)
        }
        NSLayoutConstraint.activate([
            footer.heightAnchor.constraint(equalToConstant: 25),
            start.leadingAnchor.constraint(equalTo: footer.leadingAnchor),
            start.centerYAnchor.constraint(equalTo: footer.centerYAnchor),
            end.trailingAnchor.constraint(equalTo: footer.trailingAnchor),
            end.centerYAnchor.constraint(equalTo: footer.centerYAnchor),
        ])
        // Ready sums up the steps rather than being one, so it has no dots.
        if step != .ready {
            let dots = StepDots(groups: dotGroups)
            dots.translatesAutoresizingMaskIntoConstraints = false
            footer.addSubview(dots)
            NSLayoutConstraint.activate([
                dots.centerXAnchor.constraint(equalTo: footer.centerXAnchor),
                dots.centerYAnchor.constraint(equalTo: footer.centerYAnchor),
            ])
        }
        return footer
    }

    private struct Choice {
        let title: String
        var enabled = true
        let run: () -> Void
    }

    /// The step's buttons: a link beside Back, then the one to press last on the right.
    private var actions: (link: Choice?, secondary: Choice?, primary: Choice) {
        let next: () -> Void = { [weak self] in self?.advance() }
        switch step {
        case .alreadySetUp:
            return (
                nil,
                Choice(title: "Review Settings") { [weak self] in
                    self?.reviewing = true
                    self?.go(to: 0)
                },
                Choice(title: "Skip to Nightcap") { [weak self] in self?.goToNightcap() }
            )
        case .strength:
            return (nil, nil, Choice(title: "Continue", run: next))
        case .lastThings:
            return (
                nil, nil,
                Choice(title: "Continue") { [weak self] in
                    guard let self else { return }
                    if let opensAtLogin { choose(.openAtLogin(opensAtLogin ? .on : .off)) }
                    if wantsNotifications, notifications == .notDetermined { askForNotifications() }
                    advance()
                }
            )
        case .schedule:
            return (
                nil, nil,
                Choice(title: "Continue") { [weak self] in
                    guard let self else { return }
                    if schedule != setup.nightShift.schedule { choose(.setSchedule(schedule)) }
                    advance()
                }
            )
        case .apps:
            let adding = apps.filter { $0.ruled == nil && tickedApps.contains($0.id) }
            return (
                apps.isEmpty || apps.count >= Self.gridSize
                    ? nil : Choice(title: "Choose Another App…") { [weak self] in self?.chooseApp() },
                Choice(title: "Not Now", run: next),
                Choice(title: Self.addRules(adding.count), enabled: !adding.isEmpty) { [weak self] in
                    guard let self else { return }
                    for app in adding { choose(.addRule(identity: .app(app.id), says: .off)) }
                    addedApps += adding.count
                    firstApp = firstApp ?? adding.first?.name
                    advance()
                }
            )
        case .websites:
            switch page {
            case .ask:
                return (
                    nil,
                    Choice(title: "Not Now") { [weak self] in
                        self?.websites = false
                        self?.advance()
                    },
                    Choice(title: "Turn On") { [weak self] in
                        guard let self else { return }
                        websites = true
                        page = accessibilityGranted() ? .sites : .accessibility
                        draw()
                    }
                )
            case .accessibility where waitingForAccessibility || accessibilityGranted():
                return (nil, nil, Choice(title: "Waiting…", enabled: false) {})
            case .accessibility:
                return (
                    nil,
                    Choice(title: "Not Now", run: next),
                    Choice(title: "Allow Accessibility…") { [weak self] in
                        guard let self else { return }
                        waitingForAccessibility = true
                        askForAccessibility()
                        draw()
                    }
                )
            case .sites:
                let adding = sites.filter { $0.ruled == nil && tickedSites.contains($0.id) }
                return (
                    nil,
                    Choice(title: "Not Now", run: next),
                    Choice(title: Self.addRules(adding.count), enabled: !adding.isEmpty) { [weak self] in
                        guard let self else { return }
                        for site in adding { choose(.addRule(identity: .domain(site.id), says: .off)) }
                        addedSites += adding.count
                        advance()
                    }
                )
            }
        case .ready:
            return (nil, nil, Choice(title: "Done") { [weak self] in self?.window.performClose(nil) })
        }
    }

    /// A dot per step in two groups, Night Shift's and Nightcap's.
    private var dotGroups: [[StepDots.Dot]] {
        let steps = self.steps
        return [Self.nightShiftSteps, Self.nightcapSteps].map { group in
            steps.enumerated().filter { group.contains($0.element) }.map { place, step in
                StepDots.Dot(
                    state: place == index ? .current : place < index ? .done : .todo,
                    label: Self.name(of: step),
                    go: place != index && place <= reached ? { [weak self] in self?.go(to: place) } : nil)
            }
        }.filter { !$0.isEmpty }
    }

    private static func name(of step: Step) -> String {
        switch step {
        case .alreadySetUp: "Night Shift"
        case .strength: "Strength"
        case .schedule: "Schedule"
        case .apps: "Apps"
        case .websites: "Websites"
        case .lastThings: "Notifications and login"
        case .ready: "Ready"
        }
    }
}

/// Onboarding's stage: a soft gradient behind the step's illustration, under the title band
/// with the wordmark.
final class Stage: NSView {
    private let sun: Sun?
    /// Where the illustration's centre sits: in the middle of what the title band leaves.
    private let middle: CGFloat
    private var scene: Scene?
    private var art: NSView?

    /// The strength the mocks of a screen show, which the strength step's follows as it moves.
    var strength: UInt8 = 50 {
        didSet { (art as? StrengthArt)?.strength = strength }
    }

    init(band: CGFloat, height: CGFloat, sun: Sun?) {
        self.sun = sun
        middle = band + (height - band) / 2
        super.init(frame: .zero)
        let wordmark = Self.wordmark()
        let line = Hairline()
        for view in [wordmark, line] {
            view.translatesAutoresizingMaskIntoConstraints = false
            addSubview(view)
        }
        NSLayoutConstraint.activate([
            wordmark.centerXAnchor.constraint(equalTo: centerXAnchor),
            wordmark.centerYAnchor.constraint(equalTo: topAnchor, constant: band / 2),
            line.leadingAnchor.constraint(equalTo: leadingAnchor),
            line.trailingAnchor.constraint(equalTo: trailingAnchor),
            line.bottomAnchor.constraint(equalTo: bottomAnchor),
        ])
    }

    required init?(coder: NSCoder) {
        fatalError("not built from a nib")
    }

    /// Replaces the illustration when the scene changes; the same one keeps its loops running.
    func show(_ scene: Scene) {
        guard scene != self.scene else { return }
        self.scene = scene
        art?.removeFromSuperview()
        let art = StageArt.view(for: scene, strength: strength, sun: sun)
        art.translatesAutoresizingMaskIntoConstraints = false
        addSubview(art)
        NSLayoutConstraint.activate([
            art.centerXAnchor.constraint(equalTo: centerXAnchor),
            art.centerYAnchor.constraint(equalTo: topAnchor, constant: middle),
            art.widthAnchor.constraint(equalToConstant: art.frame.width),
            art.heightAnchor.constraint(equalToConstant: art.frame.height),
        ])
        self.art = art
    }

    /// Drawn rather than a gradient layer, which snapshots wouldn't show.
    override func draw(_ dirtyRect: NSRect) {
        NSGradient(starting: .nightcapStageTop, ending: .nightcapStageBottom)?.draw(in: bounds, angle: -90)
    }

    /// The app icon beside "Nightcap" in Caprasimo at 26 points, one of the brand's two moments
    /// in the app (design/README.md). The icon's tile fills 80% of its image, so a 28-point
    /// image shows a 22-point tile.
    private static func wordmark() -> NSView {
        let icon = NSImageView(image: NSApp.applicationIconImage)
        icon.imageScaling = .scaleProportionallyUpOrDown
        icon.widthAnchor.constraint(equalToConstant: 28).isActive = true
        icon.heightAnchor.constraint(equalToConstant: 28).isActive = true
        let name = NSTextField(labelWithString: "Nightcap")
        name.font = .nightcapWordmark(size: 26)
        name.textColor = .labelColor
        let row = NSStackView(views: [icon, name])
        row.orientation = .horizontal
        row.alignment = .centerY
        row.spacing = 5
        return row
    }
}

/// A suggestion for a rule: an installed app, or later a website.
struct GridItem {
    let id: String
    let name: String
    let detail: String
    let icon: Icon
    /// Where the rule comes from when it exists already, such as "From your config".
    let ruled: String?

    /// An app's icon, or a site's own once it has arrived (ADR-0019).
    enum Icon {
        case app(NSImage)
        case site(NSImage?)
    }

    /// A website by its name, with its domain under it.
    init(site name: String, host: String, icon: NSImage?, ruled: String?) {
        id = host
        self.name = name
        detail = ruled ?? host
        self.icon = .site(icon)
        self.ruled = ruled
    }

    /// The app with this bundle ID, by its installed name and icon, or nil when it isn't
    /// installed.
    init?(app bundleId: String, kind: String, ruled: String?) {
        guard let url = NSWorkspace.shared.urlForApplication(withBundleIdentifier: bundleId) else { return nil }
        id = bundleId
        name = FileManager.default.displayName(atPath: url.path).replacingOccurrences(of: ".app", with: "")
        detail = ruled ?? kind
        icon = .app(Self.loaded(NSWorkspace.shared.icon(forFile: url.path)))
        self.ruled = ruled
    }
}

extension GridItem {
    /// An app's icon drawn once at the grid's size, since icons from an asset catalog arrive
    /// blank until something asks for their pixels.
    static func loaded(_ icon: NSImage) -> NSImage {
        var rect = NSRect(x: 0, y: 0, width: 48, height: 48)
        guard let pixels = icon.cgImage(forProposedRect: &rect, context: nil, hints: nil) else { return icon }
        return NSImage(cgImage: pixels, size: NSSize(width: 24, height: 24))
    }
}

/// A suggestion in onboarding's grid: a checkbox, the icon and two lines. A click anywhere on it
/// ticks it, unless its rule exists already.
final class GridCell: NSView {
    var dividedOnLeft = false
    private let checkbox: NSButton

    init(_ item: GridItem, ticked: Bool, toggle: @escaping () -> Void) {
        checkbox = NSButton(checkboxWithTitle: "", target: nil, action: nil)
        super.init(frame: .zero)
        checkbox.state = ticked ? .on : .off
        checkbox.isEnabled = item.ruled == nil
        checkbox.setAccessibilityLabel(item.name)
        Look.attach(checkbox, toggle)
        self.toggle = item.ruled == nil ? toggle : nil
        let icon: NSView
        switch item.icon {
        case .app(let image):
            let view = NSImageView(image: image)
            view.imageScaling = .scaleProportionallyUpOrDown
            view.widthAnchor.constraint(equalToConstant: 24).isActive = true
            view.heightAnchor.constraint(equalToConstant: 24).isActive = true
            icon = view
        case .site(let image):
            icon = image.map { Tile(image: $0) } ?? Tile(symbol: "globe")
        }
        let name = NSTextField(labelWithString: item.name)
        name.font = .systemFont(ofSize: 13)
        name.lineBreakMode = .byTruncatingTail
        let detail = NSTextField(labelWithString: item.detail)
        detail.font = .systemFont(ofSize: 11)
        detail.textColor = .nightcapSecondary
        detail.lineBreakMode = .byTruncatingTail
        for label in [name, detail] {
            label.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        }
        let words = NSStackView(views: [name, detail])
        words.orientation = .vertical
        words.alignment = .leading
        words.spacing = 0
        let line = NSStackView(views: [checkbox, icon, words])
        line.orientation = .horizontal
        line.spacing = 8
        line.translatesAutoresizingMaskIntoConstraints = false
        addSubview(line)
        NSLayoutConstraint.activate([
            line.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 10),
            line.trailingAnchor.constraint(lessThanOrEqualTo: trailingAnchor, constant: -10),
            line.topAnchor.constraint(equalTo: topAnchor, constant: 6),
            line.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -6),
        ])
    }

    required init?(coder: NSCoder) {
        fatalError("not built from a nib")
    }

    private var toggle: (() -> Void)?

    override func mouseDown(with event: NSEvent) {
        toggle?()
    }

    override func draw(_ dirtyRect: NSRect) {
        guard dividedOnLeft else { return }
        NSColor.nightcapSeparator.setFill()
        NSRect(x: 0, y: 0, width: 0.5, height: bounds.height).fill()
    }
}

/// A view with a hairline down its left edge, for cells side by side in a group.
final class Divided: NSView {
    private let left: Bool

    init(_ content: NSView, left: Bool) {
        self.left = left
        super.init(frame: .zero)
        content.translatesAutoresizingMaskIntoConstraints = false
        addSubview(content)
        NSLayoutConstraint.activate([
            content.leadingAnchor.constraint(equalTo: leadingAnchor),
            content.trailingAnchor.constraint(equalTo: trailingAnchor),
            content.topAnchor.constraint(equalTo: topAnchor),
            content.bottomAnchor.constraint(equalTo: bottomAnchor),
        ])
    }

    required init?(coder: NSCoder) {
        fatalError("not built from a nib")
    }

    override func draw(_ dirtyRect: NSRect) {
        guard left else { return }
        NSColor.nightcapSeparator.setFill()
        NSRect(x: 0, y: 0, width: 0.5, height: bounds.height).fill()
    }
}
