// `NSItemBadge.indicator` is imported as shared mutable state, though it only makes a badge.
@preconcurrency import AppKit
import KeyboardShortcuts
import NightcapCore

/// Nightcap's settings, as `Settings` decides them from the core's snapshot. The window only
/// draws them, rebuilding the shown tab whenever they change while it's open.
final class SettingsWindow: NSObject, NSWindowDelegate, NSToolbarDelegate {
    typealias Tab = Settings.Tab

    /// The core's latest snapshot, which the window takes as it opens or switches tabs.
    private let current: () -> Snapshot
    private let words = Wording.system
    private let icons: SiteIcons
    private let choose: (Command) -> Void
    private let perform: (AppFix) -> Void
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
    #if DEBUG
    /// The shortcut a snapshot's recorder shows, held by the recorder alone, so the one the dev
    /// build has recorded stays as it is.
    private var snapshotShortcut: KeyboardShortcuts.Shortcut?
    #endif

    /// The groups' width, which makes the mockup's 600-point window.
    private static let width: CGFloat = 552

    init(
        current: @escaping () -> Snapshot,
        icons: SiteIcons,
        choose: @escaping (Command) -> Void,
        perform: @escaping (AppFix) -> Void,
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

    /// Opens on `tab`, or on the tab last shown, as macOS settings windows do.
    func show(_ tab: Tab? = nil) {
        let window = self.window ?? makeWindow()
        let wasOpen = window.isVisible
        select(tab ?? self.tab)
        if !wasOpen { window.center() }
        NSApp.activate()
        window.makeKeyAndOrderFront(nil)
    }

    /// Redraws from `snapshot` only when what it shows changed, so a click in progress isn't lost.
    func refresh(_ snapshot: Snapshot) {
        guard window?.isVisible == true else { return }
        let settings = Settings(snapshot, words: words)
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
    /// The shown tab's content as a PNG, for comparing against the mockup, with the scenario's
    /// `shortcut` in the shortcut's recorder.
    func snapshot(_ tab: Tab, shortcut: KeyboardShortcuts.Shortcut) -> Data? {
        let window = self.window ?? makeWindow()
        window.alphaValue = 0
        animates = false
        snapshotShortcut = shortcut
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
        item.label = words.title(of: tab)
        item.image = NSImage(systemSymbolName: tab.symbol, accessibilityDescription: item.label)
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
        window?.title = words.title(of: tab)
        draw(Settings(current(), words: words))
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
            item.badge = settings.hasProblem(in: tab) ? .indicator : nil
        }
    }

    // MARK: Panes

    private func generalPane(_ settings: Settings) -> NSView {
        let websites = settings.websiteChecking
        let toggle = NSSwitch()
        toggle.state = websites.on ? .on : .off
        toggle.isEnabled = !websites.locked
        toggle.onAction { [choose] in choose(websites.command) }
        return pane([
            block(
                words.websites,
                [
                    cell(
                        row(
                            title: words.checkWebsites, detail: words.checkWebsitesDetail(setInConfig: websites.locked),
                            accessory: toggle),
                        settings.check(.accessibility))
                ]),
            block(
                words.nightShift,
                [
                    cell(
                        row(
                            title: words.scheduleAndStrength,
                            detail: words.scheduleAndStrengthDetail,
                            accessory: CapsuleButton(words.openNightShiftSettings, .regular) {
                                let pane = "x-apple.systempreferences:com.apple.Displays-Settings.extension"
                                NSWorkspace.shared.open(URL(string: pane)!)
                            }),
                        settings.check(.nightShift)),
                    cell(row(title: words.shortcut, detail: words.shortcutDetail, accessory: shortcutRecorder()), nil),
                ]),
            block(
                words.notifications,
                [
                    cell(
                        row(title: nil, detail: words.notificationsDetail, accessory: nil),
                        settings.check(.notifications))
                ]),
            block(
                words.nightcap,
                openAtLogin(settings) + [
                    cell(
                        row(
                            title: words.tour,
                            detail: words.tourDetail,
                            accessory: CapsuleButton(words.takeTheTour, .regular) { [takeTour] in takeTour() }),
                        nil),
                    cell(
                        row(
                            title: words.version(settings.version),
                            detail: words.updates(byItself: checkForUpdates != nil),
                            accessory: checkForUpdates.map { update in
                                CapsuleButton(words.checkForUpdates, .regular, update)
                            }),
                        nil),
                    cell(
                        row(
                            title: words.supportNightcap,
                            detail: words.supportNightcapDetail,
                            accessory: CapsuleButton(words.sponsor, .regular) {
                                NSWorkspace.shared.open(URL(string: "https://github.com/sponsors/choco")!)
                            }),
                        nil),
                ]),
        ])
    }

    /// The shortcut's recorder, bound to the shortcut KeyboardShortcuts keeps in user defaults; a
    /// snapshot's shows its scenario's instead.
    private func shortcutRecorder() -> NSView {
        #if DEBUG
        if let snapshotShortcut { return KeyboardShortcuts.RecorderCocoa(shortcut: snapshotShortcut) }
        #endif
        return KeyboardShortcuts.RecorderCocoa(for: .nightShift)
    }

    /// "Open at login", in self-managed installs, with its check while it's a problem.
    private func openAtLogin(_ settings: Settings) -> [NSView] {
        guard let openAtLogin = settings.openAtLogin else { return [] }
        let toggle = NSSwitch()
        toggle.state = openAtLogin.on ? .on : .off
        toggle.onAction { [choose] in choose(openAtLogin.command) }
        return [cell(row(title: words.openAtLogin, detail: nil, accessory: toggle), settings.check(.openAtLogin))]
    }

    private func rulesPane(_ settings: Settings) -> NSView {
        let path = (settings.configPath as NSString).abbreviatingWithTildeInPath
        var files = [cell(row(title: words.configFile, detail: path, accessory: nil), settings.check(.configFile))]
        if let adHoc = settings.check(.adHocRules) {
            files.append(cell(row(title: words.adHocRules, detail: nil, accessory: nil), adHoc))
        }
        let rules =
            settings.rules.isEmpty
            ? [Padded(NSTextField.secondary(words.noRulesYet, width: Self.width - 28), top: 11, bottom: 11)]
            : settings.rules.map(ruleRow)
        var blocks: [NSView] = [
            Group(rows: files),
            note(words.eachRule),
            Group(rows: rules),
            rulesToolbar(),
            note(words.adHocRulesStay),
        ]
        // What the menu set for an app or site with no rule, which ends by itself.
        if !settings.forNow.isEmpty {
            blocks.append(block(words.forNow, settings.forNow.map(forNowRow)))
        }
        return pane(blocks)
    }

    /// An app's or site's temporary rules: its icon and name, what they say until when, and
    /// ending them.
    private func forNowRow(_ temporary: Settings.ForNowRow) -> NSView {
        let (name, _, icon) = describe(temporary.identity)
        let texts = NSStackView(
            .vertical,
            [
                NSTextField.label(name, size: 13, weight: .medium, color: .labelColor, width: 300),
                NSTextField.secondary(temporary.label, width: 300),
            ], alignment: .leading, spacing: 2)
        texts.setContentHuggingPriority(.defaultLow, for: .horizontal)
        let end = CapsuleButton(temporary.button.title, .small) { [choose] in choose(temporary.button.command) }
        let row = NSStackView(.horizontal, [icon, texts, end], distribution: .fill, spacing: 14)
        return Padded(row, top: 11, bottom: 11)
    }

    /// A rule's icon, name, what it says and what it is, its label and what can be done about
    /// it: a config rule changes only in the config (ADR-0006).
    private func ruleRow(_ rule: Settings.RuleRow) -> NSView {
        let (name, detail, icon) = describe(rule.identity)
        let texts = NSStackView(
            .vertical,
            [
                NSTextField.label(name, size: 13, weight: .medium, color: .labelColor, width: 260),
                NSTextField.secondary(words.rule(rule.says, detail), width: 260),
            ], alignment: .leading, spacing: 2)
        texts.setContentHuggingPriority(.defaultLow, for: .horizontal)
        let button = rule.button.map { button in
            CapsuleButton(button.title, .small) { [choose] in choose(button.command) }
        }
        let row = NSStackView(
            .horizontal, [icon, texts, Chip(rule.label, tinted: rule.snoozed)] + [button].compactMap { $0 },
            distribution: .fill, spacing: 14)
        return Padded(row, top: 11, bottom: 11)
    }

    /// An app by the name and icon of its bundle, if it's installed, and a website by its domain
    /// and its own icon, once the menu has fetched it (ADR-0019).
    private func describe(_ identity: RuleIdentity) -> (String, String, NSView) {
        switch identity {
        case .app(let bundleId):
            guard let url = NSWorkspace.shared.urlForApplication(withBundleIdentifier: bundleId) else {
                return (bundleId, words.notInstalled, Tile(symbol: "app"))
            }
            let icon = NSImageView(image: Apps.icon(at: url, side: 26))
            icon.imageScaling = .scaleProportionallyUpOrDown
            icon.widthAnchor.constraint(equalToConstant: 26).isActive = true
            icon.heightAnchor.constraint(equalToConstant: 26).isActive = true
            return (Apps.name(at: url), bundleId, icon)
        case .domain(let domain):
            let icon = icons.icon(for: domain).map { Tile(image: $0) } ?? Tile(symbol: "globe")
            return (domain, words.websiteWithSubdomains, icon)
        }
    }

    private func rulesToolbar() -> NSView {
        let addApp = CapsuleButton(words.addApp, .regular) { [weak self] in self?.addApp() }
        let addWebsite = CapsuleButton(words.addWebsite, .regular) { [weak self] in self?.addWebsite() }
        let spacer = NSView()
        spacer.setContentHuggingPriority(.defaultLow, for: .horizontal)
        let label = NSTextField(labelWithString: words.copyForConfigAs)
        label.font = .systemFont(ofSize: 12)
        label.textColor = .nightcapSecondary
        let format = NSPopUpButton(frame: .zero, pullsDown: false)
        format.addItems(withTitles: [words.name(of: .nix), words.name(of: .toml)])
        format.controlSize = .small
        format.font = .systemFont(ofSize: 12)
        format.selectItem(at: exportFormat == .nix ? 0 : 1)
        format.onAction { [weak self, weak format] in
            self?.exportFormat = format?.indexOfSelectedItem == 1 ? .toml : .nix
        }
        let copy = CapsuleButton(words.copy, .regular) {}
        copy.onAction { [weak self, weak copy] in
            guard let self else { return }
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString(export(exportFormat), forType: .string)
            copy?.title = words.copied
            DispatchQueue.main.asyncAfter(deadline: .now() + 1.2) { [words] in copy?.title = words.copy }
        }
        let toolbar = NSStackView(.horizontal, [addApp, addWebsite, spacer, label, format, copy], distribution: .fill)
        toolbar.setCustomSpacing(16, after: addWebsite)
        return toolbar
    }

    /// The Applications folder, for picking an app to add a rule for.
    private func addApp() {
        guard let window else { return }
        let says = saysPopup()
        Apps.pick(
            on: window, prompt: words.add, message: words.chooseAnAppForARule,
            accessory: Padded(says, top: 8, bottom: 8)
        ) { [weak self] bundleId in
            self?.choose(.addRule(identity: .app(bundleId), says: Self.says(says)))
        }
    }

    /// What a new rule says: Night Shift off, as most rules do, or on.
    private func saysPopup() -> NSPopUpButton {
        let popup = NSPopUpButton(frame: .zero, pullsDown: false)
        popup.addItems(withTitles: [words.keepNightShift(.off), words.keepNightShift(.on)])
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
        alert.messageText = words.addAWebsite
        let hint = words.ruleCoversSubdomains
        alert.informativeText = hint
        let field = NSTextField(frame: NSRect(x: 0, y: 0, width: 280, height: 24))
        field.placeholderString = words.domainExample
        let says = saysPopup()
        let accessory = NSStackView(.vertical, [field, says], alignment: .leading)
        accessory.frame = NSRect(x: 0, y: 0, width: 280, height: 58)
        field.widthAnchor.constraint(equalToConstant: 280).isActive = true
        alert.accessoryView = accessory
        let add = alert.addButton(withTitle: words.add)
        alert.addButton(withTitle: words.cancel)
        add.isEnabled = false
        let typing = NotificationCenter.default.addObserver(
            forName: NSControl.textDidChangeNotification, object: field, queue: .main
        ) { [weak alert, weak add, weak field, words] _ in
            MainActor.assumeIsolated {
                let domain = field.flatMap { normalizeDomain(text: $0.stringValue) }
                add?.isEnabled = domain != nil
                alert?.informativeText = domain.map(words.adds) ?? hint
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
        Padded(NSTextField.secondary(text, width: Self.width - 8), top: 0, left: 4, bottom: 0, right: 4)
    }

    // MARK: Building blocks

    /// The tab's content: blocks stacked 16 apart inside an 18, 24, 22 margin.
    private func pane(_ blocks: [NSView]) -> NSView {
        let stack = NSStackView(
            .vertical, blocks, alignment: .leading, spacing: 16,
            edgeInsets: NSEdgeInsets(top: 18, left: 24, bottom: 22, right: 24))
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
        let indented = NSStackView(
            .horizontal, [heading], edgeInsets: NSEdgeInsets(top: 0, left: 4, bottom: 0, right: 0))
        let group = Group(rows: cells)
        let block = NSStackView(.vertical, [indented, group], alignment: .leading, spacing: 6)
        group.widthAnchor.constraint(equalTo: block.widthAnchor).isActive = true
        return block
    }

    /// A setting's row with the status of its check, if it has one, right under it.
    private func cell(_ row: NSView, _ check: Settings.CheckStatus?) -> NSView {
        let cell = NSStackView(
            .vertical, [row] + (check.map { [status($0)] } ?? []), alignment: .leading, spacing: 0)
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
            texts.append(NSTextField.label(title, size: 13, weight: .medium, color: .labelColor, width: textWidth))
        }
        if let detail {
            texts.append(NSTextField.secondary(detail, width: textWidth))
        }
        let text = NSStackView(.vertical, texts, alignment: .leading, spacing: 2)
        text.setContentHuggingPriority(.defaultLow, for: .horizontal)
        let row = NSStackView(
            .horizontal, [text] + (accessory.map { [$0] } ?? []), distribution: .fill, spacing: 14)
        return Padded(row, top: 11, bottom: 11)
    }

    /// A check's symbol, its line of text and what can be done about it, under its setting.
    /// One fix sits at the end of the line; two get a line of their own, so the text can stay
    /// on one.
    private func status(_ check: Settings.CheckStatus) -> NSView {
        let fixes = [check.restore, check.accept].compactMap { $0 }.map { button in
            CapsuleButton(button.title, .small) { [choose, perform] in
                switch button.fix {
                case .command(let command): choose(command)
                case .app(let fix): perform(fix)
                }
            }
        }
        let inline = fixes.count < 2 ? fixes : []
        let inlineWidth = inline.reduce(0) { $0 + $1.fittingSize.width + 8 }
        let text = NSTextField.secondary(check.text, width: Self.width - 28 - 18 - 8 - inlineWidth)
        text.setContentHuggingPriority(.defaultLow, for: .horizontal)
        let line = NSStackView(.horizontal, [StateMark(check.state).view(), text] + inline, distribution: .fill)
        guard inline.isEmpty, !fixes.isEmpty else {
            return Padded(line, top: 0, bottom: 11)
        }
        let spacer = NSView()
        spacer.setContentHuggingPriority(.defaultLow, for: .horizontal)
        let buttons = NSStackView(.horizontal, [spacer] + fixes, distribution: .fill)
        let status = NSStackView(.vertical, [line, buttons], alignment: .leading)
        for view in [line, buttons] {
            view.widthAnchor.constraint(equalTo: status.widthAnchor).isActive = true
        }
        return Padded(status, top: 0, bottom: 11)
    }
}

extension Settings.Tab {
    fileprivate var symbol: String {
        switch self {
        case .general: "gearshape"
        case .rules: "list.bullet"
        }
    }

    fileprivate var identifier: NSToolbarItem.Identifier { NSToolbarItem.Identifier(rawValue) }
}
