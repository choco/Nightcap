// `NSItemBadge.indicator` is imported as shared mutable state, though it only makes a badge.
@preconcurrency import AppKit
import KeyboardShortcuts
import NightcapCore

/// Nightcap's settings, as the core describes them (`Core.settings()`). The shell only draws
/// them, rebuilding the shown tab whenever the snapshot changes while the window is open.
final class SettingsWindow: NSObject, NSWindowDelegate, NSToolbarDelegate {
    enum Tab: String, CaseIterable {
        case general, rules

        var title: String {
            switch self {
            case .general: "General"
            case .rules: "Rules"
            }
        }

        var symbol: String {
            switch self {
            case .general: "gearshape"
            case .rules: "list.bullet"
            }
        }

        var identifier: NSToolbarItem.Identifier { NSToolbarItem.Identifier(rawValue) }
    }

    private let current: () -> Settings
    private let icons: SiteIcons
    private let choose: (Command) -> Void
    private let perform: (Fix) -> Void
    private let checkForUpdates: (() -> Void)?
    private let export: (ExportFormat) -> String
    private let takeTour: () -> Void
    /// The format "Copy" last wrote, kept across redraws.
    private var exportFormat = ExportFormat.nix
    private let onClose: () -> Void
    private var window: NSWindow?
    private var tab = Tab.general
    private var shown: Settings?
    /// Whether switching tabs animates the window to its new height.
    private var animates = true

    /// The groups' width, which makes the mockup's 600-point window.
    private static let width: CGFloat = 552
    private static let checksByTab: [Tab: [CheckKind]] = [
        .general: [.nightShift, .accessibility, .notifications, .openAtLogin],
        .rules: [.configFile, .adHocRules],
    ]

    init(
        current: @escaping () -> Settings,
        icons: SiteIcons,
        choose: @escaping (Command) -> Void,
        perform: @escaping (Fix) -> Void,
        checkForUpdates: (() -> Void)?,
        export: @escaping (ExportFormat) -> String,
        takeTour: @escaping () -> Void,
        onClose: @escaping () -> Void
    ) {
        self.current = current
        self.icons = icons
        self.choose = choose
        self.perform = perform
        self.checkForUpdates = checkForUpdates
        self.export = export
        self.takeTour = takeTour
        self.onClose = onClose
        super.init()
    }

    /// The tab with the problem the menu points at: General's first, then Rules'.
    static func tab(for settings: Settings) -> Tab {
        let problems = settings.checks.filter(\.isProblem).map(\.kind)
        return problems.isEmpty || problems.contains(where: { checksByTab[.general]!.contains($0) })
            ? .general : .rules
    }

    /// Opens on `tab`, or on the tab last shown, as macOS settings windows do.
    func show(_ tab: Tab? = nil) {
        let window = self.window ?? makeWindow()
        let wasOpen = window.isVisible
        select(tab ?? self.tab)
        if !wasOpen { window.center() }
        NSApp.activate()
        window.makeKeyAndOrderFront(nil)
    }

    /// Redraws only when what the core shows changed, so a click in progress isn't lost.
    func refresh() {
        guard window?.isVisible == true else { return }
        let settings = current()
        guard settings != shown else { return }
        draw(settings)
    }

    /// Redraws the Rules tab, the only one with sites, with a site's icon that just arrived.
    func iconArrived() {
        guard window?.isVisible == true, tab == .rules, let shown else { return }
        draw(shown)
    }

    func close() {
        window?.close()
    }

    func windowWillClose(_ notification: Notification) {
        onClose()
    }

    #if DEBUG
    /// The shown tab's content as a PNG, for comparing against the mockup.
    func snapshot(_ tab: Tab) -> Data? {
        let window = self.window ?? makeWindow()
        window.alphaValue = 0
        animates = false
        show(tab)
        return window.contentView?.png()
    }
    #endif

    // MARK: Toolbar

    func toolbarAllowedItemIdentifiers(_ toolbar: NSToolbar) -> [NSToolbarItem.Identifier] {
        Tab.allCases.map(\.identifier)
    }

    func toolbarDefaultItemIdentifiers(_ toolbar: NSToolbar) -> [NSToolbarItem.Identifier] {
        Tab.allCases.map(\.identifier)
    }

    func toolbarSelectableItemIdentifiers(_ toolbar: NSToolbar) -> [NSToolbarItem.Identifier] {
        Tab.allCases.map(\.identifier)
    }

    func toolbar(
        _ toolbar: NSToolbar, itemForItemIdentifier identifier: NSToolbarItem.Identifier,
        willBeInsertedIntoToolbar flag: Bool
    ) -> NSToolbarItem? {
        guard let tab = Tab(rawValue: identifier.rawValue) else { return nil }
        let item = NSToolbarItem(itemIdentifier: identifier)
        item.label = tab.title
        item.image = NSImage(systemSymbolName: tab.symbol, accessibilityDescription: tab.title)
        item.target = self
        item.action = #selector(tabChosen(_:))
        return item
    }

    @objc private func tabChosen(_ item: NSToolbarItem) {
        guard let tab = Tab(rawValue: item.itemIdentifier.rawValue) else { return }
        select(tab)
    }

    private func makeWindow() -> NSWindow {
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: Self.width + 48, height: 300),
            styleMask: [.titled, .closable],
            backing: .buffered,
            defer: false)
        window.isReleasedWhenClosed = false
        window.delegate = self
        window.backgroundColor = .nightcapWindow
        window.toolbarStyle = .preference
        let toolbar = NSToolbar(identifier: "settings")
        toolbar.delegate = self
        toolbar.displayMode = .iconAndLabel
        toolbar.allowsUserCustomization = false
        window.toolbar = toolbar
        self.window = window
        return window
    }

    private func select(_ tab: Tab) {
        self.tab = tab
        window?.toolbar?.selectedItemIdentifier = tab.identifier
        window?.title = tab.title
        draw(current())
    }

    /// Builds the shown tab, sizes the window to fit it, keeping its top edge, then shows it.
    private func draw(_ settings: Settings) {
        guard let window else { return }
        shown = settings
        let content = tab == .general ? generalPane(settings) : rulesPane(settings)
        let size = content.fittingSize
        // The title bar and toolbar, which `frameRect(forContentRect:)` leaves out.
        let chrome = window.frame.height - window.contentLayoutRect.height
        let container = Backdrop()
        content.translatesAutoresizingMaskIntoConstraints = false
        container.addSubview(content)
        // Pinned to the top only, at its own height: while the window animates to a new
        // height, it neither stretches nor squeezes the content.
        NSLayoutConstraint.activate([
            content.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            content.trailingAnchor.constraint(equalTo: container.trailingAnchor),
            content.topAnchor.constraint(equalTo: container.topAnchor),
        ])
        // The new content goes in before the window takes its size: resizing first would
        // draw the old content squeezed into the new size.
        window.contentView = container
        window.setFrame(
            NSRect(
                x: window.frame.minX, y: window.frame.maxY - size.height - chrome,
                width: size.width, height: size.height + chrome),
            display: true, animate: animates && window.isVisible)
        markTabs(settings)
    }

    /// A tab with a problem in it gets an indicator badge, the way System Settings marks items
    /// that need attention. Before macOS 26 toolbar items take no badge, and the problem shows
    /// only in its pane.
    private func markTabs(_ settings: Settings) {
        guard #available(macOS 26, *) else { return }
        for item in window?.toolbar?.items ?? [] {
            guard let tab = Tab(rawValue: item.itemIdentifier.rawValue) else { continue }
            let kinds = Self.checksByTab[tab] ?? []
            let problem = settings.checks.contains { kinds.contains($0.kind) && $0.isProblem }
            item.badge = problem ? .indicator : nil
        }
    }

    // MARK: Panes

    private func generalPane(_ settings: Settings) -> NSView {
        let check = { (kind: CheckKind) in settings.checks.first { $0.kind == kind } }
        let websites = settings.websiteChecking
        let toggle = NSSwitch()
        toggle.state = websites.on ? .on : .off
        toggle.isEnabled = !websites.locked
        Look.attach(toggle) { [choose] in choose(websites.command) }
        var websitesDetail =
            "Also keep Night Shift off, or on, on the websites you choose, "
            + "in Safari, Chrome, Firefox, Edge, Arc, Brave, Dia, Zen and Orion. "
            + "Nightcap reads the front tab’s address with Accessibility and keeps only the site’s name, "
            + "and asks the site for its icon."
        if websites.locked { websitesDetail += " Set in your config." }
        // Accessibility goes unmentioned while website checking doesn't need it.
        let accessibility = check(.accessibility).flatMap { $0.state == .off ? nil : $0 }

        let version = Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "?"
        return pane([
            block(
                "Websites",
                [
                    cell(row(title: "Check websites", detail: websitesDetail, accessory: toggle), accessibility)
                ]),
            block(
                "Night Shift",
                [
                    cell(
                        row(
                            title: "Schedule and strength",
                            detail: "Kept by macOS. The menu’s slider sets the strength, and your config can set both.",
                            accessory: Look.button("Open Night Shift Settings…", .regular) {
                                let pane = "x-apple.systempreferences:com.apple.Displays-Settings.extension"
                                NSWorkspace.shared.open(URL(string: pane)!)
                            }),
                        check(.nightShift)),
                    cell(
                        row(
                            title: "Shortcut",
                            detail:
                                "Tap it to turn Night Shift on or off. Hold it to see true colour until you let go.",
                            accessory: KeyboardShortcuts.RecorderCocoa(for: .nightShift)),
                        nil),
                ]),
            block(
                "Notifications",
                [
                    cell(
                        row(
                            title: nil,
                            detail: "Nightcap tells you when a change made outside it overrides one of your rules.",
                            accessory: nil),
                        check(.notifications))
                ]),
            block(
                "Nightcap",
                openAtLogin(settings) + [
                    cell(
                        row(
                            title: "Tour",
                            detail: "A rule at work, the peek and the menu, in about a minute.",
                            accessory: Look.button("Take the Tour", .regular) { [takeTour] in takeTour() }),
                        nil),
                    cell(
                        row(
                            title: "Version \(version)",
                            detail: checkForUpdates == nil
                                ? "Updates come from your installer."
                                : "Checks for updates once a day and installs them when you quit.",
                            accessory: checkForUpdates.map { update in
                                Look.button("Check for Updates…", .regular, update)
                            }),
                        nil),
                    cell(
                        row(
                            title: "Support Nightcap",
                            detail: "It’s free and open source. If it helps you, you can sponsor it on GitHub.",
                            accessory: Look.button("Sponsor…", .regular) {
                                NSWorkspace.shared.open(URL(string: "https://github.com/sponsors/choco")!)
                            }),
                        nil),
                ]),
        ])
    }

    /// "Open at login", in self-managed installs: its switch shows on or off, so its check
    /// shows only when what the user chose stopped working.
    private func openAtLogin(_ settings: Settings) -> [NSView] {
        guard let openAtLogin = settings.openAtLogin else { return [] }
        let toggle = NSSwitch()
        toggle.state = openAtLogin.on ? .on : .off
        Look.attach(toggle) { [choose] in choose(openAtLogin.command) }
        let problem = settings.checks.first { $0.kind == .openAtLogin && $0.isProblem }
        return [cell(row(title: "Open at login", detail: nil, accessory: toggle), problem)]
    }

    private func rulesPane(_ settings: Settings) -> NSView {
        let check = { (kind: CheckKind) in settings.checks.first { $0.kind == kind } }
        let path = (settings.configPath as NSString).abbreviatingWithTildeInPath
        var files = [cell(row(title: "Config file", detail: path, accessory: nil), check(.configFile))]
        // The ad-hoc rules file is mentioned only when something's wrong with it.
        if let adHoc = check(.adHocRules), adHoc.isProblem {
            files.append(cell(row(title: "Ad-hoc rules", detail: nil, accessory: nil), adHoc))
        }
        let rules =
            settings.rules.isEmpty
            ? [Look.padded(Look.secondary("No rules yet.", width: Self.width - 28), top: 11, bottom: 11)]
            : settings.rules.map(ruleRow)
        var blocks = [
            group(files),
            note("Each keeps Night Shift off, or on, while its app or site is in front."),
            group(rules),
            rulesToolbar(),
            note("Ad-hoc rules stay in Nightcap until you copy them into your config."),
        ]
        // What the menu set for an app or site with no rule, which ends by itself.
        if !settings.forNow.isEmpty {
            blocks.append(block("For now", settings.forNow.map(forNowRow)))
        }
        return pane(blocks)
    }

    /// An app's or site's temporary on or off: its icon and name, until when, and ending it.
    private func forNowRow(_ layers: SettingsLayers) -> NSView {
        let identity: RuleIdentity
        switch layers.place {
        case .app(let bundleId): identity = .app(bundleId)
        case .domain(let domain): identity = .domain(domain)
        case .everywhere: preconditionFailure("For now lists only apps and sites")
        }
        let (name, _, icon) = describe(identity)
        let texts = NSStackView(views: [
            Look.label(name, size: 13, weight: .medium, color: .labelColor, width: 300),
            Look.secondary(layers.label, width: 300),
        ])
        texts.orientation = .vertical
        texts.alignment = .leading
        texts.spacing = 2
        texts.setContentHuggingPriority(.defaultLow, for: .horizontal)
        let end = Look.button(layers.action.title, .small) { [choose] in choose(layers.action.command) }
        let row = NSStackView(views: [icon, texts, end])
        row.orientation = .horizontal
        row.distribution = .fill
        row.alignment = .centerY
        row.spacing = 14
        return Look.padded(row, top: 11, bottom: 11)
    }

    /// A rule's icon, name, what it says and what it is, its label and what can be done about
    /// it: a config rule changes only in the config (ADR-0006).
    private func ruleRow(_ rule: SettingsRule) -> NSView {
        let (name, detail, icon) = describe(rule.identity)
        let texts = NSStackView(views: [
            Look.label(name, size: 13, weight: .medium, color: .labelColor, width: 260),
            Look.secondary("Night Shift \(rule.says == .on ? "on" : "off") · \(detail)", width: 260),
        ])
        texts.orientation = .vertical
        texts.alignment = .leading
        texts.spacing = 2
        texts.setContentHuggingPriority(.defaultLow, for: .horizontal)
        let action = rule.action.map { action in
            Look.button(action.title, .small) { [choose] in choose(action.command) }
        }
        let row = NSStackView(
            views: [icon, texts, Chip(rule.label, tinted: rule.snoozed)] + [action].compactMap { $0 })
        row.orientation = .horizontal
        row.distribution = .fill
        row.alignment = .centerY
        row.spacing = 14
        return Look.padded(row, top: 11, bottom: 11)
    }

    /// An app by the name and icon of its bundle, if it's installed, and a website by its domain
    /// and its own icon, once the menu has fetched it (ADR-0019).
    private func describe(_ identity: RuleIdentity) -> (String, String, NSView) {
        switch identity {
        case .app(let bundleId):
            guard let url = NSWorkspace.shared.urlForApplication(withBundleIdentifier: bundleId) else {
                return (bundleId, "Not installed on this Mac", Tile(symbol: "app"))
            }
            let icon = NSImageView(image: NSWorkspace.shared.icon(forFile: url.path))
            icon.imageScaling = .scaleProportionallyUpOrDown
            icon.widthAnchor.constraint(equalToConstant: 26).isActive = true
            icon.heightAnchor.constraint(equalToConstant: 26).isActive = true
            return (Apps.name(at: url), bundleId, icon)
        case .domain(let domain):
            let icon = icons.icon(for: domain).map { Tile(image: $0) } ?? Tile(symbol: "globe")
            return (domain, "Website, with its subdomains", icon)
        }
    }

    private func rulesToolbar() -> NSView {
        let addApp = Look.button("Add App…", .regular) { [weak self] in self?.addApp() }
        let addWebsite = Look.button("Add Website…", .regular) { [weak self] in self?.addWebsite() }
        let spacer = NSView()
        spacer.setContentHuggingPriority(.defaultLow, for: .horizontal)
        let label = NSTextField(labelWithString: "Copy for config as")
        label.font = .systemFont(ofSize: 12)
        label.textColor = .nightcapSecondary
        let format = NSPopUpButton(frame: .zero, pullsDown: false)
        format.addItems(withTitles: ["Nix", "TOML"])
        format.controlSize = .small
        format.font = .systemFont(ofSize: 12)
        format.selectItem(at: exportFormat == .nix ? 0 : 1)
        Look.attach(format) { [weak self, weak format] in
            self?.exportFormat = format?.indexOfSelectedItem == 1 ? .toml : .nix
        }
        let copy = Look.button("Copy", .regular) {}
        Look.attach(copy) { [weak self, weak copy] in
            guard let self else { return }
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString(export(exportFormat), forType: .string)
            copy?.title = "Copied"
            DispatchQueue.main.asyncAfter(deadline: .now() + 1.2) { copy?.title = "Copy" }
        }
        let toolbar = NSStackView(views: [addApp, addWebsite, spacer, label, format, copy])
        toolbar.orientation = .horizontal
        toolbar.distribution = .fill
        toolbar.alignment = .centerY
        toolbar.spacing = 8
        toolbar.setCustomSpacing(16, after: addWebsite)
        return toolbar
    }

    /// The Applications folder, for picking an app to add a rule for.
    private func addApp() {
        guard let window else { return }
        let says = Self.saysPopup()
        Apps.pick(
            on: window, prompt: "Add", message: "Choose an app for a rule.",
            accessory: Look.padded(says, top: 8, bottom: 8)
        ) { [weak self] bundleId in
            self?.choose(.addRule(identity: .app(bundleId), says: Self.says(says)))
        }
    }

    /// What a new rule says: Night Shift off, as most rules do, or on.
    private static func saysPopup() -> NSPopUpButton {
        let popup = NSPopUpButton(frame: .zero, pullsDown: false)
        popup.addItems(withTitles: ["Keep Night Shift off", "Keep Night Shift on"])
        return popup
    }

    private static func says(_ popup: NSPopUpButton) -> OnOff {
        popup.indexOfSelectedItem == 1 ? .on : .off
    }

    /// A sheet asking for a domain, which says what it will add as the user types and adds
    /// nothing that isn't one.
    private func addWebsite() {
        guard let window else { return }
        let alert = NSAlert()
        alert.messageText = "Add a Website"
        let hint = "The rule covers it and its subdomains."
        alert.informativeText = hint
        let field = NSTextField(frame: NSRect(x: 0, y: 0, width: 280, height: 24))
        field.placeholderString = "figma.com"
        let says = Self.saysPopup()
        let accessory = NSStackView(views: [field, says])
        accessory.orientation = .vertical
        accessory.alignment = .leading
        accessory.spacing = 8
        accessory.frame = NSRect(x: 0, y: 0, width: 280, height: 58)
        field.widthAnchor.constraint(equalToConstant: 280).isActive = true
        alert.accessoryView = accessory
        let add = alert.addButton(withTitle: "Add")
        alert.addButton(withTitle: "Cancel")
        add.isEnabled = false
        let typing = NotificationCenter.default.addObserver(
            forName: NSControl.textDidChangeNotification, object: field, queue: .main
        ) { [weak alert, weak add, weak field] _ in
            MainActor.assumeIsolated {
                let domain = field.flatMap { normalizeDomain(text: $0.stringValue) }
                add?.isEnabled = domain != nil
                alert?.informativeText = domain.map { "Adds \($0) and its subdomains." } ?? hint
            }
        }
        alert.window.initialFirstResponder = field
        alert.beginSheetModal(for: window) { [weak self] response in
            NotificationCenter.default.removeObserver(typing)
            guard response == .alertFirstButtonReturn,
                let domain = normalizeDomain(text: field.stringValue)
            else { return }
            self?.choose(.addRule(identity: .domain(domain), says: Self.says(says)))
        }
    }

    private func note(_ text: String) -> NSView {
        Look.padded(Look.secondary(text, width: Self.width - 8), top: 0, left: 4, bottom: 0, right: 4)
    }

    // MARK: Building blocks

    /// The tab's content: blocks stacked 16 apart inside an 18, 24, 22 margin.
    private func pane(_ blocks: [NSView]) -> NSView {
        let stack = NSStackView(views: blocks)
        stack.orientation = .vertical
        stack.alignment = .leading
        stack.spacing = 16
        stack.edgeInsets = NSEdgeInsets(top: 18, left: 24, bottom: 22, right: 24)
        for block in blocks {
            block.widthAnchor.constraint(equalToConstant: Self.width).isActive = true
        }
        return stack
    }

    /// A heading over a rounded group of cells, separated by hairlines.
    private func block(_ title: String, _ cells: [NSView]) -> NSView {
        let heading = NSTextField(labelWithString: title)
        heading.font = .systemFont(ofSize: 12, weight: .semibold)
        heading.textColor = .nightcapSecondary
        let indented = NSStackView(views: [heading])
        indented.edgeInsets = NSEdgeInsets(top: 0, left: 4, bottom: 0, right: 0)
        let group = group(cells)
        let block = NSStackView(views: [indented, group])
        block.orientation = .vertical
        block.alignment = .leading
        block.spacing = 6
        group.widthAnchor.constraint(equalTo: block.widthAnchor).isActive = true
        return block
    }

    /// A rounded group of cells, separated by hairlines.
    private func group(_ cells: [NSView]) -> NSView {
        var views: [NSView] = []
        for (index, cell) in cells.enumerated() {
            if index > 0 { views.append(Hairline()) }
            views.append(cell)
        }
        let rows = NSStackView(views: views)
        rows.orientation = .vertical
        rows.alignment = .leading
        rows.spacing = 0
        for view in views {
            view.widthAnchor.constraint(equalTo: rows.widthAnchor).isActive = true
        }
        return Group(rows)
    }

    /// A setting's row with the status of its check, if it has one, right under it.
    private func cell(_ row: NSView, _ check: SettingsCheck?) -> NSView {
        let cell = NSStackView(views: [row] + (check.map { [status($0)] } ?? []))
        cell.orientation = .vertical
        cell.alignment = .leading
        cell.spacing = 0
        for view in cell.views {
            view.widthAnchor.constraint(equalTo: cell.widthAnchor).isActive = true
        }
        return cell
    }

    /// A title over a line of detail, with a control at the right edge: 11, 14 padding, 14 apart.
    private func row(title: String?, detail: String?, accessory: NSView?) -> NSView {
        let accessoryWidth = accessory.map { $0.fittingSize.width + 14 } ?? 0
        let textWidth = Self.width - 28 - accessoryWidth
        var texts: [NSView] = []
        if let title {
            texts.append(Look.label(title, size: 13, weight: .medium, color: .labelColor, width: textWidth))
        }
        if let detail {
            texts.append(Look.secondary(detail, width: textWidth))
        }
        let text = NSStackView(views: texts)
        text.orientation = .vertical
        text.alignment = .leading
        text.spacing = 2
        text.setContentHuggingPriority(.defaultLow, for: .horizontal)
        let row = NSStackView(views: [text] + (accessory.map { [$0] } ?? []))
        row.orientation = .horizontal
        row.distribution = .fill
        row.alignment = .centerY
        row.spacing = 14
        return Look.padded(row, top: 11, bottom: 11)
    }

    /// A check's symbol, its line of text and what can be done about it, under its setting.
    /// One fix sits at the end of the line; two get a line of their own, so the text can stay
    /// on one.
    private func status(_ check: SettingsCheck) -> NSView {
        let fixes = [check.restore, check.accept].compactMap { $0 }.map { fix in
            Look.button(fix.title, .small) { [perform] in perform(fix.fix) }
        }
        let inline = fixes.count < 2 ? fixes : []
        let inlineWidth = inline.reduce(0) { $0 + $1.fittingSize.width + 8 }
        let text = Look.secondary(check.text, width: Self.width - 28 - 18 - 8 - inlineWidth)
        text.setContentHuggingPriority(.defaultLow, for: .horizontal)
        let line = NSStackView(views: [Look.mark(Look.Mark(check.state)), text] + inline)
        line.orientation = .horizontal
        line.distribution = .fill
        line.alignment = .centerY
        line.spacing = 8
        guard inline.isEmpty, !fixes.isEmpty else {
            return Look.padded(line, top: 0, bottom: 11)
        }
        let spacer = NSView()
        spacer.setContentHuggingPriority(.defaultLow, for: .horizontal)
        let buttons = NSStackView(views: [spacer] + fixes)
        buttons.orientation = .horizontal
        buttons.distribution = .fill
        buttons.spacing = 8
        let status = NSStackView(views: [line, buttons])
        status.orientation = .vertical
        status.alignment = .leading
        status.spacing = 8
        for view in [line, buttons] {
            view.widthAnchor.constraint(equalTo: status.widthAnchor).isActive = true
        }
        return Look.padded(status, top: 0, bottom: 11)
    }
}

extension SettingsCheck {
    /// A check that needs the user: drift, or broken.
    var isProblem: Bool { state == .drifted || state == .broken }
}
