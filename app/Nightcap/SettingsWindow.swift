// `NSItemBadge.indicator` is imported as shared mutable state, though it only makes a badge.
@preconcurrency import AppKit
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
    private let choose: (Command) -> Void
    private let perform: (Fix) -> Void
    private let checkForUpdates: (() -> Void)?
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
        choose: @escaping (Command) -> Void,
        perform: @escaping (Fix) -> Void,
        checkForUpdates: (() -> Void)?,
        onClose: @escaping () -> Void
    ) {
        self.current = current
        self.choose = choose
        self.perform = perform
        self.checkForUpdates = checkForUpdates
        self.onClose = onClose
        super.init()
    }

    /// The tab with the problem the menu points at: General's first, then Rules'.
    static func tab(for settings: Settings) -> Tab {
        let problems = settings.checks.filter { $0.state == .drifted || $0.state == .broken }.map(\.kind)
        return problems.isEmpty || problems.contains(where: { checksByTab[.general]!.contains($0) })
            ? .general : .rules
    }

    func show(_ tab: Tab) {
        let window = self.window ?? makeWindow()
        let wasOpen = window.isVisible
        select(tab)
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
        guard let view = window.contentView else { return nil }
        view.layoutSubtreeIfNeeded()
        view.displayIfNeeded()
        guard let bitmap = view.bitmapImageRepForCachingDisplay(in: view.bounds) else { return nil }
        view.cacheDisplay(in: view.bounds, to: bitmap)
        return bitmap.representation(using: .png, properties: [:])
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
        window.backgroundColor = .settingsWindow
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
        window.setFrame(
            NSRect(
                x: window.frame.minX, y: window.frame.maxY - size.height - chrome,
                width: size.width, height: size.height + chrome),
            display: true, animate: animates && window.isVisible)
        let container = Backdrop()
        content.translatesAutoresizingMaskIntoConstraints = false
        container.addSubview(content)
        // Pinned to the top: while the window animates to a new height, the extra room stays
        // below the content instead of stretching it.
        NSLayoutConstraint.activate([
            content.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            content.trailingAnchor.constraint(equalTo: container.trailingAnchor),
            content.topAnchor.constraint(equalTo: container.topAnchor),
            content.bottomAnchor.constraint(lessThanOrEqualTo: container.bottomAnchor),
        ])
        window.contentView = container
        markTabs(settings)
    }

    /// A tab with a problem in it gets an indicator badge, the way System Settings marks items
    /// that need attention.
    private func markTabs(_ settings: Settings) {
        for item in window?.toolbar?.items ?? [] {
            guard let tab = Tab(rawValue: item.itemIdentifier.rawValue) else { continue }
            let kinds = Self.checksByTab[tab] ?? []
            let problem = settings.checks.contains {
                kinds.contains($0.kind) && ($0.state == .drifted || $0.state == .broken)
            }
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
        attach(toggle) { [choose] in choose(websites.command) }
        var websitesDetail = "Also force Night Shift off on the websites you choose, in Safari and Chrome. Nightcap reads the front tab’s address with Accessibility and keeps only the site’s name."
        if websites.locked { websitesDetail += " Set in your config." }
        // Accessibility goes unmentioned while website checking doesn't need it.
        let accessibility = check(.accessibility).flatMap { $0.state == .off ? nil : $0 }

        let version = Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "?"
        return pane([
            block("Websites", [
                cell(row(title: "Check websites", detail: websitesDetail, accessory: toggle), accessibility),
            ]),
            block("Night Shift", [
                cell(
                    row(
                        title: "Schedule and strength",
                        detail: "Set in System Settings. Nightcap follows them.",
                        accessory: button("Open Night Shift Settings…", size: .regular) {
                            let pane = "x-apple.systempreferences:com.apple.Displays-Settings.extension"
                            NSWorkspace.shared.open(URL(string: pane)!)
                        }),
                    check(.nightShift)),
            ]),
            block("Notifications", [
                cell(
                    row(
                        title: nil,
                        detail: "Nightcap tells you when a change made outside it overrides one of your rules.",
                        accessory: nil),
                    check(.notifications)),
            ]),
            block("Nightcap", openAtLogin(settings) + [
                cell(
                    row(
                        title: "Version \(version)",
                        detail: checkForUpdates == nil
                            ? "Updates come from your installer."
                            : "Checks for updates once a day and installs them when you quit.",
                        accessory: checkForUpdates.map { update in
                            button("Check for Updates…", size: .regular, update)
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
        attach(toggle) { [choose] in choose(openAtLogin.command) }
        let problem = settings.checks.first {
            $0.kind == .openAtLogin && ($0.state == .drifted || $0.state == .broken)
        }
        return [cell(row(title: "Open at login", detail: nil, accessory: toggle), problem)]
    }

    private func rulesPane(_ settings: Settings) -> NSView {
        let check = { (kind: CheckKind) in settings.checks.first { $0.kind == kind } }
        return pane([
            block("Files", [
                cell(row(title: "Config file", detail: nil, accessory: nil), check(.configFile)),
                cell(row(title: "Ad-hoc rules", detail: nil, accessory: nil), check(.adHocRules)),
            ]),
        ])
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
        heading.textColor = .settingsSecondary
        let indented = NSStackView(views: [heading])
        indented.edgeInsets = NSEdgeInsets(top: 0, left: 4, bottom: 0, right: 0)
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
        let group = Group(rows)
        let block = NSStackView(views: [indented, group])
        block.orientation = .vertical
        block.alignment = .leading
        block.spacing = 6
        group.widthAnchor.constraint(equalTo: block.widthAnchor).isActive = true
        return block
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
            texts.append(label(title, size: 13, weight: .medium, color: .labelColor, width: textWidth))
        }
        if let detail {
            texts.append(secondary(detail, width: textWidth))
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
        return padded(row, top: 11, bottom: 11)
    }

    /// A check's symbol, its line of text and what can be done about it, under its setting.
    /// One fix sits at the end of the line; two get a line of their own, so the text can stay
    /// on one.
    private func status(_ check: SettingsCheck) -> NSView {
        let fixes = [check.restore, check.accept].compactMap { $0 }.map { fix in
            button(fix.title, size: .small) { [perform] in perform(fix.fix) }
        }
        let inline = fixes.count < 2 ? fixes : []
        let inlineWidth = inline.reduce(0) { $0 + $1.fittingSize.width + 8 }
        let text = secondary(check.text, width: Self.width - 28 - 18 - 8 - inlineWidth)
        text.setContentHuggingPriority(.defaultLow, for: .horizontal)
        let line = NSStackView(views: [badge(check.state), text] + inline)
        line.orientation = .horizontal
        line.distribution = .fill
        line.alignment = .centerY
        line.spacing = 8
        guard inline.isEmpty, !fixes.isEmpty else {
            return padded(line, top: 0, bottom: 11)
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
        return padded(status, top: 0, bottom: 11)
    }

    /// `content` inside 14-point sides and the given top and bottom, held by constraints.
    private func padded(_ content: NSView, top: CGFloat, bottom: CGFloat) -> NSView {
        let box = NSView()
        content.translatesAutoresizingMaskIntoConstraints = false
        box.addSubview(content)
        NSLayoutConstraint.activate([
            content.leadingAnchor.constraint(equalTo: box.leadingAnchor, constant: 14),
            content.trailingAnchor.constraint(equalTo: box.trailingAnchor, constant: -14),
            content.topAnchor.constraint(equalTo: box.topAnchor, constant: top),
            content.bottomAnchor.constraint(equalTo: box.bottomAnchor, constant: -bottom),
        ])
        return box
    }

    /// An 18-point SF Symbol: a white mark on a filled circle or triangle, or a grey dash.
    private func badge(_ state: CheckState) -> NSImageView {
        let (symbol, colors, description): (String, [NSColor], String) =
            switch state {
            case .ok: ("checkmark.circle.fill", [.white, .settingsGreen], "OK")
            case .off: ("minus.circle", [.settingsGrey], "Off")
            case .drifted: ("exclamationmark.triangle.fill", [.white, .settingsOrange], "Needs attention")
            case .broken: ("xmark.circle.fill", [.white, .settingsRed], "Problem")
            }
        let configuration = NSImage.SymbolConfiguration(pointSize: 15, weight: .semibold)
            .applying(NSImage.SymbolConfiguration(paletteColors: colors))
        let image = NSImageView(
            image: NSImage(systemSymbolName: symbol, accessibilityDescription: description)!
                .withSymbolConfiguration(configuration)!)
        image.widthAnchor.constraint(equalToConstant: 18).isActive = true
        image.heightAnchor.constraint(equalToConstant: 18).isActive = true
        return image
    }

    private func secondary(_ text: String, width: CGFloat) -> NSTextField {
        label(text, size: 12, weight: .regular, color: .settingsSecondary, width: width)
    }

    /// Wrapping text with the mockup's line height, 1.4 times the font size.
    private func label(
        _ text: String, size: CGFloat, weight: NSFont.Weight, color: NSColor, width: CGFloat
    ) -> NSTextField {
        let font = NSFont.systemFont(ofSize: size, weight: weight)
        let lineHeight = (size * 1.4).rounded()
        let paragraph = NSMutableParagraphStyle()
        paragraph.minimumLineHeight = lineHeight
        paragraph.maximumLineHeight = lineHeight
        let label = NSTextField(wrappingLabelWithString: "")
        label.attributedStringValue = NSAttributedString(
            string: text,
            attributes: [
                .font: font,
                .foregroundColor: color,
                .paragraphStyle: paragraph,
                // Centers the glyphs in the taller line, as CSS does.
                .baselineOffset: (lineHeight - font.ascender + font.descender) / 4,
            ])
        label.preferredMaxLayoutWidth = width
        return label
    }

    private func button(_ title: String, size: NSControl.ControlSize, _ run: @escaping () -> Void) -> NSButton {
        let button = CapsuleButton(title: title, small: size == .small)
        attach(button, run)
        return button
    }

    private func attach(_ control: NSControl, _ run: @escaping () -> Void) {
        let action = Action(run)
        control.target = action
        control.action = #selector(Action.run)
        objc_setAssociatedObject(control, &Action.key, action, .OBJC_ASSOCIATION_RETAIN)
    }
}

/// The mockup's button: a capsule with a hairline border and a soft shadow, which AppKit's own
/// push button loses on a white group. 13-point text in 24 points, or 12 in 20 when small.
private final class CapsuleButton: NSButton {
    private let small: Bool
    private var padding: CGFloat { small ? 10 : 13 }

    init(title: String, small: Bool) {
        self.small = small
        super.init(frame: .zero)
        self.title = title
        font = .systemFont(ofSize: small ? 12 : 13)
        isBordered = false
        bezelStyle = .push
        wantsLayer = true
        setButtonType(.momentaryChange)
    }

    required init?(coder: NSCoder) {
        fatalError("not built from a nib")
    }

    override var intrinsicContentSize: NSSize {
        let width = attributedTitle.size().width
        return NSSize(width: (width + padding * 2).rounded(.up), height: small ? 20 : 24)
    }

    override var isHighlighted: Bool {
        didSet { needsDisplay = true }
    }

    override var wantsUpdateLayer: Bool { true }

    override func updateLayer() {
        guard let layer else { return }
        layer.cornerRadius = bounds.height / 2
        layer.borderWidth = 0.5
        layer.shadowOffset = NSSize(width: 0, height: -0.5)
        layer.shadowRadius = 0.75
        layer.shadowOpacity = 1
        effectiveAppearance.performAsCurrentDrawingAppearance {
            let fill = isHighlighted ? NSColor.settingsControlPressed : NSColor.settingsControl
            layer.backgroundColor = fill.cgColor
            layer.borderColor = NSColor.settingsControlBorder.cgColor
            layer.shadowColor = NSColor.black.withAlphaComponent(0.14).cgColor
        }
    }
}

/// The window's own colour behind the tabs, which the mockup sets apart from the groups.
private final class Backdrop: NSView {
    override var wantsUpdateLayer: Bool { true }

    override func updateLayer() {
        effectiveAppearance.performAsCurrentDrawingAppearance {
            layer?.backgroundColor = NSColor.settingsWindow.cgColor
        }
    }
}

/// A rounded group of rows, sized by its content.
private final class Group: NSView {
    init(_ content: NSView) {
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

    override var wantsUpdateLayer: Bool { true }

    override func updateLayer() {
        guard let layer else { return }
        layer.cornerRadius = 12
        layer.borderWidth = 0.5
        effectiveAppearance.performAsCurrentDrawingAppearance {
            layer.borderColor = NSColor.settingsSeparator.cgColor
            layer.backgroundColor = NSColor.settingsGroup.cgColor
        }
    }
}

/// The mockup's 0.5-point line between rows.
private final class Hairline: NSView {
    override var intrinsicContentSize: NSSize { NSSize(width: NSView.noIntrinsicMetric, height: 0.5) }
    override var wantsUpdateLayer: Bool { true }
    override func updateLayer() {
        effectiveAppearance.performAsCurrentDrawingAppearance {
            layer?.backgroundColor = NSColor.settingsSeparator.cgColor
        }
    }
}

/// The mockup's palette, light and dark.
private extension NSColor {
    static func appearance(light: UInt32, dark: UInt32, lightAlpha: CGFloat = 1, darkAlpha: CGFloat = 1) -> NSColor {
        NSColor(name: nil) { appearance in
            let isDark = appearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua
            let hex = isDark ? dark : light
            return NSColor(
                srgbRed: CGFloat((hex >> 16) & 0xff) / 255,
                green: CGFloat((hex >> 8) & 0xff) / 255,
                blue: CGFloat(hex & 0xff) / 255,
                alpha: isDark ? darkAlpha : lightAlpha)
        }
    }

    static var settingsWindow: NSColor { appearance(light: 0xf4f4f6, dark: 0x1e1e21) }
    static var settingsGroup: NSColor { appearance(light: 0xffffff, dark: 0x2b2b2e) }
    static var settingsSeparator: NSColor {
        appearance(light: 0x000000, dark: 0xffffff, lightAlpha: 0.1, darkAlpha: 0.09)
    }
    static var settingsSecondary: NSColor { appearance(light: 0x5f5f64, dark: 0xa9a9af) }
    static var settingsControl: NSColor { appearance(light: 0xffffff, dark: 0x3a3a3e) }
    static var settingsControlPressed: NSColor { appearance(light: 0xe8e8ec, dark: 0x4a4a4f) }
    static var settingsControlBorder: NSColor {
        appearance(light: 0x000000, dark: 0xffffff, lightAlpha: 0.16, darkAlpha: 0.12)
    }
    static var settingsGreen: NSColor { appearance(light: 0x22b33a, dark: 0x30d158) }
    static var settingsOrange: NSColor { appearance(light: 0xf08a00, dark: 0xff9f0a) }
    static var settingsRed: NSColor { appearance(light: 0xe5372c, dark: 0xff453a) }
    static var settingsGrey: NSColor { appearance(light: 0xa1a1a6, dark: 0x6e6e73) }
}

/// A control's target that runs a closure; the control keeps it alive.
private final class Action: NSObject {
    nonisolated(unsafe) static var key = 0
    private let body: () -> Void

    init(_ body: @escaping () -> Void) {
        self.body = body
    }

    @objc func run() {
        body()
    }
}
