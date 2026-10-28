import AppKit
import KeyboardShortcuts
import NightcapCore

/// Onboarding (ADR-0011, ADR-0012, ADR-0014): one step at a time under a stage that illustrates
/// it. Night Shift comes first, set up or kept as it is, then Nightcap's own steps. Closing the
/// window finishes onboarding with what was chosen so far. Quitting Nightcap leaves it
/// unfinished. Where it is and what it sends is `OnboardingFlow`'s; this draws it.
final class Onboarding: NSObject, NSWindowDelegate {
    /// Apps that need true colour, suggested when they're installed, each with a word for what
    /// it's for.
    private static let suggestedApps: [(bundleId: String, kind: KeyPath<Wording, String>)] = [
        ("com.figma.Desktop", \.design),
        ("com.bohemiancoding.sketch3", \.design),
        ("com.adobe.illustrator", \.design),
        ("com.adobe.Photoshop", \.photo),
        ("com.adobe.LightroomClassicCC7", \.photo),
        ("com.pixelmatorteam.pixelmator.x", \.photo),
        ("com.apple.Photos", \.photo),
        ("com.apple.FinalCut", \.videoEditing),
        ("com.blackmagic-design.DaVinciResolve", \.videoEditing),
        ("com.apple.iMovieApp", \.videoEditing),
        ("com.apple.TV", \.video),
        ("com.apple.QuickTimePlayerX", \.video),
        ("com.colliderli.iina", \.video),
        ("org.videolan.vlc", \.video),
        ("com.firecore.infuse", \.video),
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
    private let flow: OnboardingFlow
    private let words = Wording.system
    private let icons: SiteIcons
    private let sun: Sun?
    private let now: () -> Date
    private let shortcut: KeyboardShortcuts.Shortcut?
    /// The window's lasting content: the stage, whose loops would restart if it were rebuilt, and
    /// the page under it, which is.
    private var stage: Stage?
    private var pageArea: NSView?
    /// The apps in the grid, each made once, since it loads its icon.
    private var appItems: [RuleIdentity: GridItem]
    /// A redraw for arriving icons is on its way.
    private var redrawing = false
    /// The sites page was drawn last, so redrawing it asks for no icons.
    private var showingSites = false
    /// The suggestions as the page last drew them, with where their rules come from.
    private var drawnSuggestions: [OnboardingFlow.Suggestion] = []
    private var closedByUser = false
    private weak var previewNote: NSTextField?
    private var radios: [(button: NSButton, schedule: () -> Schedule)] = []

    /// `setup` is what onboarding asks this user; `rules` are the core's rules as they stand; `now`
    /// is the time the stage's timeline marks; `shortcut` is the one Ready's menu shows; `choose`
    /// sends a command to the core, as the menu does; `notifications` says whether notifications
    /// are allowed, denied or not asked for yet, as last read; `onFinish` runs once onboarding
    /// ends, by Done or the close button, with whether the user chose website checking, or nil
    /// where the config sets it.
    init(
        setup: OnboardingFlow.Setup,
        rules: @escaping () -> [Rule],
        icons: SiteIcons,
        sun: Sun?,
        now: @escaping () -> Date,
        shortcut: KeyboardShortcuts.Shortcut?,
        accessibilityGranted: @escaping () -> Bool,
        askForAccessibility: @escaping () -> Void,
        choose: @escaping (Command) -> Void,
        notifications: @escaping () -> NotificationPermission,
        askForNotifications: @escaping () -> Void,
        onFinish: @escaping (_ websites: Bool?) -> Void
    ) {
        // One place is kept for an app the user chooses.
        let words = self.words
        let apps = Self.suggestedApps.compactMap { GridItem(app: $0.bundleId, kind: words[keyPath: $0.kind]) }
            .prefix(OnboardingFlow.gridSize - 1)
        appItems = Dictionary(uniqueKeysWithValues: apps.map { (RuleIdentity.app($0.id), $0) })
        flow = OnboardingFlow(
            setup: setup, apps: apps.map(\.id), sites: Self.suggestedSites.map(\.host), rules: rules, words: words,
            accessibilityGranted: accessibilityGranted, askForAccessibility: askForAccessibility, choose: choose,
            notifications: notifications, askForNotifications: askForNotifications, onFinish: onFinish)
        self.icons = icons
        self.sun = sun
        self.now = now
        self.shortcut = shortcut
        super.init()
        window.title = words.welcomeToNightcap
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
        draw()
        window.center()
    }

    /// Only the last steps' rows depend on notifications; redrawing elsewhere could end a drag.
    func notificationsChanged() {
        if flow.step == .lastThings || flow.step == .ready { draw() }
    }

    /// Redraws a grid whose suggestions' rules came or went, as from the CLI, so each shows where
    /// its rule comes from as the core has it.
    func refresh() {
        let grid = flow.step == .apps || (flow.step == .websites && flow.page == .sites)
        if grid, flow.apps + flow.sites != drawnSuggestions { draw() }
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
        guard flow.grantArrived() else { return }
        draw()
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.8) { [weak self] in
            guard let self, flow.moveOnToSites() else { return }
            draw()
        }
    }

    /// Asked only when the user closes the window, never when Nightcap quits.
    func windowShouldClose(_ sender: NSWindow) -> Bool {
        closedByUser = true
        return true
    }

    func windowWillClose(_ notification: Notification) {
        if closedByUser { flow.finish() }
    }

    #if DEBUG
    /// The window as a PNG at `step`, for comparing against the prototype.
    func snapshot(
        _ step: OnboardingFlow.Step, page: OnboardingFlow.WebsitesPage = .ask, reviewing: Bool = false
    ) -> Data? {
        guard flow.show(step, page: page, reviewing: reviewing) else { return nil }
        draw()
        window.alphaValue = 0
        window.orderFront(nil)
        // The frame view, so the traffic lights come out too.
        return window.contentView?.superview?.png()
    }
    #endif

    /// Redraws the sites page with the icons that just arrived, which come close together, once.
    func iconArrived() {
        guard flow.step == .websites, flow.page == .sites, !redrawing else { return }
        redrawing = true
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.25) { [weak self] in
            guard let self else { return }
            redrawing = false
            if flow.step == .websites, flow.page == .sites { draw() }
        }
    }

    /// A footer's action, the window's own or the flow's, then the step it leads to.
    private func run(_ action: OnboardingFlow.Action) {
        switch action {
        case .chooseApp: chooseApp()
        case .done: window.performClose(nil)
        default:
            flow.perform(action)
            draw()
        }
    }

    private func go(to index: Int) {
        flow.go(to: index)
        draw()
    }

    // MARK: Drawing

    /// Shows where onboarding is now: the stage's scene, which changes only with it, and the
    /// page, rebuilt each time. The window keeps one size.
    private func draw() {
        if stage == nil { build() }
        guard let stage, let pageArea else { return }
        // The suggestions are the same for everyone, so asking for their icons tells the sites
        // nothing about the user's browsing (ADR-0019).
        let sites = flow.step == .websites && flow.page == .sites
        if sites && !showingSites { icons.ask(Self.suggestedSites.map(\.host)) }
        showingSites = sites
        drawnSuggestions = flow.apps + flow.sites
        stage.strength = flow.strength
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
        let stage = Stage(band: band, height: Self.stageHeight, sun: sun, now: now, shortcut: shortcut)
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
        switch flow.step {
        case .alreadySetUp: .timeline(flow.setup.schedule ?? .sunsetToSunrise)
        case .strength: .strength
        case .schedule: flow.schedule == .off ? .byHand : .timeline(flow.schedule)
        case .apps: .apps
        case .websites:
            switch flow.page {
            case .ask: .address
            case .accessibility: .accessibility(granted: flow.accessibilityGranted)
            case .sites: .sites
            }
        case .lastThings: .lastThings(app: firstApp ?? "Figma")
        case .ready: .ready
        }
    }

    /// The first app given a rule here, which the last step's notification names.
    private var firstApp: String? {
        flow.addedApps.first.flatMap { appItems[$0.identity]?.name }
    }

    /// The suggested sites, with their icons as far as they've arrived.
    private var siteItems: [RuleIdentity: GridItem] {
        Dictionary(
            uniqueKeysWithValues: Self.suggestedSites.map { site in
                let item = GridItem(site: site.name, host: site.host, icon: icons.icon(for: site.host))
                return (RuleIdentity.domain(site.host), item)
            })
    }

    /// The step's words and controls, over its footer.
    private func pageView() -> NSView {
        let page = words.page(flow.step, flow.page, reviewing: flow.reviewing, foundApps: !flow.apps.isEmpty)
        var column: [NSView] = [
            Look.label(
                words.eyebrow(flow.placeInPart), size: 12, weight: .semibold, color: .nightcapMoon,
                width: Self.textWidth),
            Look.label(page.heading, size: 20, weight: .semibold, color: .labelColor, width: Self.textWidth),
            Look.label(page.lede, size: 13, weight: .regular, color: .nightcapSecondary, width: Self.textWidth),
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
        let view = NSView()
        for part in [stack, footer] {
            part.translatesAutoresizingMaskIntoConstraints = false
            view.addSubview(part)
        }
        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: Self.margin),
            stack.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -Self.margin),
            stack.topAnchor.constraint(equalTo: view.topAnchor, constant: 20),
            footer.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: Self.margin),
            footer.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -Self.margin),
            footer.bottomAnchor.constraint(equalTo: view.bottomAnchor, constant: -16),
            stack.bottomAnchor.constraint(lessThanOrEqualTo: footer.topAnchor, constant: -12),
        ])
        return view
    }

    /// The step's controls between its words and its footer.
    private var controls: [NSView] {
        switch flow.step {
        case .alreadySetUp:
            return [
                rows([
                    summary(words.strength, words.warmth(flow.strength)),
                    summary(words.schedule, words.schedule(flow.schedule)),
                ]),
                Look.secondary(words.skippingLeavesThem, width: Self.textWidth),
            ]
        case .strength:
            return strengthControls()
        case .schedule:
            return scheduleControls()
        case .apps where flow.apps.isEmpty:
            let note = Look.secondary(words.foundNoApps, width: Self.textWidth - 150)
            note.setContentHuggingPriority(.defaultLow, for: .horizontal)
            let line = NSStackView(views: [
                note, Look.button(words.chooseApp, .regular) { [weak self] in self?.chooseApp() },
            ])
            line.orientation = .horizontal
            line.distribution = .fill
            line.spacing = 14
            return [rows([Look.padded(line, top: 12, left: 14, bottom: 12, right: 14)])]
        case .apps:
            return [grid(flow.apps, appItems)]
        case .websites:
            return websitesControls()
        case .lastThings:
            var options: [NSView] = []
            if flow.notifications != .granted {
                let title = words.tellMeWhenARuleIsSnoozed
                // macOS shows its prompt only once, so after a refusal only System Settings can.
                options.append(
                    flow.notifications == .denied
                        ? option(
                            title, words.whatNotificationsTell(switchedOff: true),
                            button: Look.button(words.openNotificationSettings, .regular) {
                                NSWorkspace.shared.open(CoreHost.notificationSettings)
                            })
                        : option(
                            title, words.whatNotificationsTell(switchedOff: false), ticked: flow.wantsNotifications,
                            toggle: { [weak self] in self?.flow.wantsNotifications = $0 }))
            }
            if let opensAtLogin = flow.opensAtLogin {
                options.append(
                    option(words.openAtLogin, words.openAtLoginDetail, ticked: opensAtLogin) { [weak self] in
                        self?.flow.opensAtLogin = $0
                    })
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
        let granted = flow.accessibilityGranted
        var lines: [(String, String, Bool)] = []
        if flow.setup.strength != nil { lines.append((words.strength, words.warmth(flow.strength), true)) }
        if flow.setup.schedule != nil { lines.append((words.schedule, words.schedule(flow.schedule), true)) }
        let apps = flow.addedApps.count
        lines.append((words.apps, words.appRules(apps), apps > 0))
        switch flow.websites {
        case true? where !granted: lines.append((words.websites, words.needsAccessibility, false))
        case true?:
            let sites = flow.addedSites.count
            lines.append((words.websites, sites > 0 ? words.siteRules(sites) : words.onOrOff(true), true))
        default: lines.append((words.websites, words.onOrOff(false), false))
        }
        let notificationsOn = flow.notifications == .granted
        lines.append((words.notifications, words.onOrOff(notificationsOn), notificationsOn))
        if let opensAtLogin = flow.opensAtLogin {
            lines.append((words.openAtLogin, words.onOrOff(opensAtLogin), opensAtLogin))
        }
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

    private func websitesControls() -> [NSView] {
        switch flow.page {
        case .ask:
            return [rows(words.websiteFacts.map { fact(symbol: $0.symbol, $0.lead, $0.rest) })]
        case .accessibility:
            let granted = flow.accessibilityGranted
            let asked = flow.waitingForAccessibility || granted
            let marks: [Look.Mark] = [
                asked ? .ok : .current,
                granted ? .ok : asked ? .current : .todo,
                granted ? .ok : .todo,
            ]
            return [rows(zip(marks, words.accessibilitySteps).map { progress($0, $1) })]
        case .sites:
            return [grid(flow.sites, siteItems)]
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

    /// The suggestions' cells, three to a row in a group, divided by hairlines.
    private func grid(_ suggestions: [OnboardingFlow.Suggestion], _ items: [RuleIdentity: GridItem]) -> NSView {
        let cells = suggestions.compactMap { suggestion -> GridCell? in
            guard let item = items[suggestion.identity] else { return nil }
            return GridCell(item, ruled: suggestion.ruled, ticked: flow.isTicked(suggestion)) { [weak self] in
                guard let self else { return }
                flow.toggle(suggestion)
                draw()
            }
        }
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
        Apps.pick(on: window, prompt: words.choose, message: words.chooseAnAppThatNeedsTrueColour) {
            [weak self] bundleId in
            guard let self else { return }
            let identity = RuleIdentity.app(bundleId)
            guard let item = appItems[identity] ?? GridItem(app: bundleId, kind: words.chosenByYou) else { return }
            appItems[identity] = item
            flow.add(app: bundleId)
            draw()
        }
    }

    /// The slider previews the strength on the screen while it's dragged, and saves it on release,
    /// as the menu's does.
    private func strengthControls() -> [NSView] {
        let slider = StrengthSlider(
            value: Double(flow.strength), minValue: 0, maxValue: 100, target: nil, action: nil)
        slider.isContinuous = true
        slider.setAccessibilityLabel(words.strength)
        Look.attach(slider) { [weak self, weak slider] in
            guard let self, let slider else { return }
            flow.moveStrength(to: UInt8(slider.doubleValue.rounded()), previewing: slider.dragging)
            stage?.strength = flow.strength
            showPreview(slider.dragging)
        }
        slider.onRelease = { [weak self, weak slider] in
            guard let self, let slider else { return }
            flow.moveStrength(to: UInt8(slider.doubleValue.rounded()), previewing: false)
            showPreview(false)
        }
        let less = Look.secondary(words.lessWarm, width: 80)
        let more = Look.secondary(words.moreWarm, width: 80)
        let line = NSStackView(views: [less, slider, more])
        line.orientation = .horizontal
        line.spacing = 12
        let note = Look.secondary(words.strengthPreview(showing: false), width: Self.textWidth)
        previewNote = note
        let facts = NSStackView(views: words.nightShiftFacts.map { fact($0.title, $0.text) })
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
        let replacement = Look.label(
            words.strengthPreview(showing: previewing), size: 12, weight: .regular,
            color: previewing ? .nightcapMoon : .nightcapSecondary, width: Self.textWidth)
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
            return .custom(from: flow.customHours.from, to: flow.customHours.to)
        }
        let hours = NSStackView(views: [
            timeField(flow.customHours.from) { [weak self] in self?.flow.customHours.from = $0 },
            Look.secondary(words.customHoursTo, width: 20),
            timeField(flow.customHours.to) { [weak self] in self?.flow.customHours.to = $0 },
        ])
        hours.spacing = 6
        return [
            rows([
                radioRow(words.sunsetToSunriseChoice, detail: Look.secondary(words.followsYourLocation, width: 200)) {
                    .sunsetToSunrise
                },
                radioRow(words.customHoursChoice, detail: hours, custom),
                radioRow(words.byHandChoice, detail: Look.secondary(words.onlyWhenYouTurnItOn, width: 200)) { .off },
            ]),
            Look.secondary(words.eitherWay, width: Self.textWidth),
        ]
    }

    private func radioRow(_ title: String, detail: NSView, _ schedule: @escaping () -> Schedule) -> NSView {
        let button = NSButton(radioButtonWithTitle: title, target: nil, action: nil)
        button.font = .systemFont(ofSize: 13)
        let chosen = schedule()
        button.state = Self.sameKind(chosen, flow.schedule) ? .on : .off
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
        flow.pick(schedule)
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
            pick(.custom(from: flow.customHours.from, to: flow.customHours.to))
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

    /// Back and a link at the left, the dots in the middle, and the step's buttons at the right.
    private func footerView() -> NSView {
        let buttons = flow.footer
        var leading: [NSView] = []
        if flow.canGoBack {
            leading.append(
                Look.button(words.back, .regular) { [weak self] in
                    self?.flow.goBack()
                    self?.draw()
                })
        }
        if let link = buttons.link {
            leading.append(Look.link(words.title(of: link)) { [weak self] in self?.run(link) })
        }
        var trailing: [NSView] = []
        if let secondary = buttons.secondary {
            trailing.append(Look.button(words.title(of: secondary), .regular) { [weak self] in self?.run(secondary) })
        }
        let primary = Look.button(words.title(of: buttons.primary), .primary) { [weak self] in
            self?.run(buttons.primary)
        }
        primary.isEnabled = buttons.primary.enabled
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
        if flow.step != .ready {
            let dots = StepDots(
                groups: flow.dots.map { group in
                    group.map { dot in
                        StepDots.Dot(
                            state: dot.state, label: words.name(of: dot.step),
                            go: dot.go.map { index in { [weak self] in self?.go(to: index) } })
                    }
                })
            dots.translatesAutoresizingMaskIntoConstraints = false
            footer.addSubview(dots)
            NSLayoutConstraint.activate([
                dots.centerXAnchor.constraint(equalTo: footer.centerXAnchor),
                dots.centerYAnchor.constraint(equalTo: footer.centerYAnchor),
            ])
        }
        return footer
    }

}

/// Onboarding's stage: a soft gradient behind the step's illustration, under the title band
/// with the wordmark.
final class Stage: NSView {
    private let sun: Sun?
    private let now: () -> Date
    private let shortcut: KeyboardShortcuts.Shortcut?
    /// Where the illustration's centre sits: in the middle of what the title band leaves.
    private let middle: CGFloat
    private var scene: Scene?
    private var art: NSView?

    /// The strength the mocks of a screen show, which the strength step's follows as it moves.
    var strength: UInt8 = 50 {
        didSet { (art as? StrengthArt)?.strength = strength }
    }

    init(band: CGFloat, height: CGFloat, sun: Sun?, now: @escaping () -> Date, shortcut: KeyboardShortcuts.Shortcut?) {
        self.sun = sun
        self.now = now
        self.shortcut = shortcut
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
        let art = StageArt.view(for: scene, strength: strength, sun: sun, now: now(), shortcut: shortcut)
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

/// A suggestion's look in the grid: an installed app, or a website.
struct GridItem {
    let id: String
    let name: String
    let detail: String
    let icon: Icon

    /// An app's icon, or a site's own once it has arrived (ADR-0019).
    enum Icon {
        case app(NSImage)
        case site(NSImage?)
    }

    /// A website by its name, with its domain under it.
    init(site name: String, host: String, icon: NSImage?) {
        id = host
        self.name = name
        detail = host
        self.icon = .site(icon)
    }

    /// The app with this bundle ID, by its installed name and icon, or nil when it isn't
    /// installed.
    init?(app bundleId: String, kind: String) {
        guard let url = NSWorkspace.shared.urlForApplication(withBundleIdentifier: bundleId) else { return nil }
        id = bundleId
        name = Apps.name(at: url)
        detail = kind
        icon = .app(Apps.icon(at: url, side: 24))
    }
}

/// A suggestion in onboarding's grid: a checkbox, the icon and two lines. A click anywhere on it
/// ticks it, unless its rule exists already, which the second line then says where it comes from.
final class GridCell: NSView {
    var dividedOnLeft = false
    private let checkbox: NSButton

    init(_ item: GridItem, ruled: String?, ticked: Bool, toggle: @escaping () -> Void) {
        checkbox = NSButton(checkboxWithTitle: "", target: nil, action: nil)
        super.init(frame: .zero)
        checkbox.state = ticked ? .on : .off
        checkbox.isEnabled = ruled == nil
        checkbox.setAccessibilityLabel(item.name)
        Look.attach(checkbox, toggle)
        self.toggle = ruled == nil ? toggle : nil
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
        let detail = NSTextField(labelWithString: ruled ?? item.detail)
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
