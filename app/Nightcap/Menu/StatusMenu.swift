import AppKit
import KeyboardShortcuts
import NightcapCore

/// Nightcap's menu-bar item. `Menu` decides what it shows from the core's snapshot; this only
/// draws it, rebuilding the menu each time it opens, and sends the chosen command to the core.
final class StatusMenu: NSObject, NSMenuDelegate {
    private let item = NSStatusBar.system.statusItem(withLength: NSStatusItem.squareLength)
    /// The core's latest snapshot, which the menu takes as it appears and opens.
    private let current: () -> Snapshot
    private let icons: SiteIcons
    private let words = Wording.system
    private let choose: (Command) -> Void
    /// Opens Settings: on the problem's tab for the problem line, on the last tab otherwise.
    private let openSettings: (_ onProblem: Bool) -> Void
    private let onOpen: () -> Void
    private let onClose: () -> Void
    /// What the open menu shows, so `refresh(_:)` redraws it only when that changed.
    private var shown: Menu?
    /// The open menu's strength slider, moved in place when only the strength changed.
    private weak var slider: NSSlider?
    /// "Nightcap is up here", or the panel saying the moon is hidden, while it shows.
    private var pointing: NSPopover?
    private var hiddenPanel: (panel: FloatingPanel, content: PointOut)?
    /// Custom…'s window, while it shows.
    private var picking: TimePicker?
    /// The open menu's site row, which takes the site's icon if it arrives while it shows.
    private var siteRow: (site: String, item: NSMenuItem)?
    /// The mockup's orange dot, over the template moon so the moon still follows the menu
    /// bar's appearance while the dot keeps its colour.
    private let dot = NSView()

    /// Boxes a command for `NSMenuItem.representedObject`, which takes only objects.
    private final class Choice: NSObject {
        let command: Command
        init(_ command: Command) { self.command = command }
    }

    /// Boxes what Custom… asks for, likewise.
    private final class Pick: NSObject {
        let pick: Menu.PickTime
        init(_ pick: Menu.PickTime) { self.pick = pick }
    }

    init(
        current: @escaping () -> Snapshot,
        icons: SiteIcons,
        choose: @escaping (Command) -> Void,
        openSettings: @escaping (_ onProblem: Bool) -> Void,
        onOpen: @escaping () -> Void,
        onClose: @escaping () -> Void
    ) {
        self.current = current
        self.icons = icons
        self.choose = choose
        self.openSettings = openSettings
        self.onOpen = onOpen
        self.onClose = onClose
        super.init()
        let menu = NSMenu()
        menu.delegate = self
        menu.autoenablesItems = false
        item.menu = menu
        if let button = item.button {
            dot.wantsLayer = true
            dot.layer?.backgroundColor = NSColor.systemOrange.cgColor
            dot.layer?.cornerRadius = MenuArt.dotSize / 2
            dot.translatesAutoresizingMaskIntoConstraints = false
            button.addSubview(dot)
            NSLayoutConstraint.activate([
                dot.widthAnchor.constraint(equalToConstant: MenuArt.dotSize),
                dot.heightAnchor.constraint(equalToConstant: MenuArt.dotSize),
                dot.centerXAnchor.constraint(equalTo: button.centerXAnchor, constant: MenuArt.dotOffset.x),
                dot.centerYAnchor.constraint(equalTo: button.centerYAnchor, constant: -MenuArt.dotOffset.y),
            ])
        }
        refresh(current())
    }

    /// Where items' titles start, in the slider's view, which puts its small sun there as the
    /// mockup does.
    private static let titleInset: CGFloat = 46

    /// Redraws the icon from `snapshot`, and the menu too while it's open and what it shows
    /// changed. A new strength alone moves the slider in place, since rebuilding the menu would
    /// end a drag.
    func refresh(_ snapshot: Snapshot) {
        let decided = decide(snapshot)
        let image = MenuArt.moon(for: decided.moon)
        if decided.problemLine != nil { image?.accessibilityDescription = words.needingAttention(decided.moon) }
        item.button?.image = image
        item.button?.appearsDisabled = decided.moon == .unavailable
        dot.isHidden = decided.problemLine == nil
        guard let shown, decided != shown, let menu = item.menu else { return }
        if let slider, Self.apartFromStrength(decided) == Self.apartFromStrength(shown),
            let strength = Self.strength(in: decided)
        {
            self.shown = decided
            if NSEvent.pressedMouseButtons == 0 { slider.doubleValue = Double(strength) }
        } else {
            rebuild(menu, showing: decided)
        }
    }

    /// The menu `snapshot` decides as it's drawn, with the front app by its name.
    private func decide(_ snapshot: Snapshot) -> Menu {
        Menu(snapshot, now: Date(), appName: Apps.runningName(of:), words: words)
    }

    private static func strength(in menu: Menu) -> UInt8? {
        for entry in menu.entries {
            if case .strength(let value) = entry { return value }
        }
        return nil
    }

    private static func apartFromStrength(_ menu: Menu) -> Menu {
        var menu = menu
        menu.entries.removeAll { if case .strength = $0 { true } else { false } }
        return menu
    }

    /// Where macOS has the moon. A new status item's button waits at the screen's origin, with no
    /// height, until macOS places it, a quarter of a second to over two seconds in testing. It
    /// passes below the screen for a moment, then slides into the menu bar; when the menu bar has
    /// no room for it, it stays below the screen.
    private enum Placement { case pending, inMenuBar, belowScreen }

    private static let placementWait: TimeInterval = 5
    private static let belowScreenSettle: TimeInterval = 0.5

    private var placement: Placement {
        guard let window = item.button?.window, window.frame.height > 0 else { return .pending }
        guard let screen = window.screen else { return .belowScreen }
        return window.frame.maxY >= screen.frame.maxY - 1 ? .inMenuBar : .pending
    }

    /// Points at the moon once onboarding is done: the only way into Nightcap from then on. It
    /// stays until OK: on macOS 27, a status item's click never arrives while a popover hangs
    /// from it. A popover, since on macOS 27 the system draws the status item, and only AppKit
    /// knows where it is. It offers the tour, which then starts knowing where the moon is. A moon
    /// macOS hides, or never places, gets a panel saying so instead, under the menu bar at the
    /// right. `onShown` runs once either is up and focused.
    func pointOut(takeTour: @escaping () -> Void, onShown: @escaping () -> Void) {
        afterPlacement(deadline: Date().addingTimeInterval(Self.placementWait)) { [weak self] inMenuBar in
            guard let self, let button = item.button else { return }
            let content = PointOut(
                hidden: !inMenuBar,
                onOK: { [weak self] in self?.stopPointing() },
                takeTour: { [weak self] in
                    self?.stopPointing()
                    takeTour()
                })
            // Focused, as a popover the user opened would be; without it the popover's glass washes
            // out. Nightcap is still the active app from the click on Done, so nothing else loses
            // the focus to it.
            NSApp.activate()
            if inMenuBar {
                let popover = NSPopover()
                popover.behavior = .applicationDefined
                popover.contentViewController = content
                popover.contentSize = content.size
                popover.show(relativeTo: button.bounds, of: button, preferredEdge: .minY)
                popover.contentViewController?.view.window?.makeKey()
                pointing = popover
            } else {
                let panel = FloatingPanel()
                content.view.translatesAutoresizingMaskIntoConstraints = false
                panel.glass.addSubview(content.view)
                NSLayoutConstraint.activate([
                    content.view.leadingAnchor.constraint(equalTo: panel.glass.leadingAnchor),
                    content.view.topAnchor.constraint(equalTo: panel.glass.topAnchor),
                ])
                panel.setContentSize(content.size)
                if let screen = NSScreen.main?.visibleFrame {
                    panel.setFrameOrigin(
                        NSPoint(
                            x: screen.maxX - panel.frame.width - 12, y: screen.maxY - panel.frame.height - 8))
                }
                panel.makeKeyAndOrderFront(nil)
                hiddenPanel = (panel, content)
            }
            onShown()
        }
    }

    /// Calls `then` once the moon is in the menu bar, or once it has stayed below the screen, or
    /// at `deadline`, with whether it's in the menu bar.
    private func afterPlacement(deadline: Date, belowSince: Date? = nil, then: @escaping (_ inMenuBar: Bool) -> Void) {
        let now = Date()
        let placement = self.placement
        let belowSince = placement == .belowScreen ? belowSince ?? now : nil
        if placement == .inMenuBar { return then(true) }
        if let belowSince, now.timeIntervalSince(belowSince) >= Self.belowScreenSettle { return then(false) }
        if now >= deadline { return then(false) }
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.05) { [weak self] in
            self?.afterPlacement(deadline: deadline, belowSince: belowSince, then: then)
        }
    }

    private func stopPointing() {
        pointing?.close()
        pointing = nil
        hiddenPanel?.panel.close()
        hiddenPanel = nil
    }

    /// Puts a site's icon that just arrived in the open menu's site row.
    func iconArrived(_ site: String) {
        guard let siteRow, siteRow.site == site, let icon = icons.icon(for: site) else { return }
        siteRow.item.image = Self.menuSized(icon)
    }

    /// `onOpen` can learn something the menu should show, which then redraws it in place.
    func menuWillOpen(_ menu: NSMenu) {
        stopPointing()
        onOpen()
    }

    func menuDidClose(_ menu: NSMenu) {
        shown = nil
        onClose()
    }

    func menuNeedsUpdate(_ menu: NSMenu) {
        rebuild(menu, showing: decide(current()))
    }

    /// Fills `menu` afresh with what `decided` shows.
    private func rebuild(_ menu: NSMenu, showing decided: Menu) {
        shown = decided
        siteRow = nil
        menu.removeAllItems()
        if let problemLine = decided.problemLine {
            let line = NSMenuItem(title: problemLine.title, action: #selector(problemLineChosen), keyEquivalent: "")
            line.subtitle = MenuArt.wrapped(problemLine.subtitle)
            line.image = MenuArt.warning
            Self.showImage(line)
            menu.addItem(line)
            menu.addItem(.separator())
        }
        menu.addItem(nightShiftItem(decided))
        for entry in decided.entries {
            switch entry {
            case .strength(let value):
                menu.addItem(strengthItem(value))
            case .row(let row):
                menu.addItem(rowItem(row))
            case .separator:
                menu.addItem(.separator())
            }
        }
        menu.addItem(.separator())
        let settings = NSMenuItem(title: words.settings, action: #selector(settingsChosen), keyEquivalent: ",")
        settings.image = Self.symbol("gear")
        Self.showImage(settings)
        menu.addItem(settings)
        let quit = NSMenuItem(title: words.quitNightcap, action: #selector(quit), keyEquivalent: "q")
        quit.image = Self.symbol("power")
        Self.showImage(quit)
        menu.addItem(quit)
        menu.items.filter { $0.action != nil }.forEach { $0.target = self }
    }

    /// "Night Shift", checked while it's on everywhere, with the menu-bar moon's look: what
    /// decides the screen, and the shortcut that does the same.
    private func nightShiftItem(_ menu: Menu) -> NSMenuItem {
        let state = menu.nightShift
        let item = NSMenuItem(title: words.nightShift, action: #selector(chose(_:)), keyEquivalent: "")
        item.image = MenuArt.moon(for: menu.moon).map(Self.menuSized)
        Self.showImage(item)
        item.state = state.on ? .on : .off
        item.subtitle = state.line.map(MenuArt.wrapped)
        if let command = state.command {
            item.representedObject = Choice(command)
        } else {
            item.isEnabled = false
        }
        if let shortcut = KeyboardShortcuts.getShortcut(for: .nightShift) {
            // Shown only: while a menu is open, KeyboardShortcuts takes the keys before the menu.
            item.keyEquivalent = shortcut.nsMenuItemKeyEquivalent ?? ""
            item.keyEquivalentModifierMask = shortcut.modifiers
            item.allowsAutomaticKeyEquivalentLocalization = false
        }
        return item
    }

    /// macOS 27 hides menu items' images unless the system is set to show them, and an item
    /// whose image is hidden sets its title further left. Every item's shows, so the titles line
    /// up and the moon and the apps' icons say what the words don't.
    private static func showImage(_ item: NSMenuItem) {
        if #available(macOS 27, *) { item.preferredImageVisibility = .visible }
    }

    /// An SF Symbol at the menu's text size, as macOS's own menus show them.
    private static func symbol(_ name: String) -> NSImage? {
        NSImage(systemSymbolName: name, accessibilityDescription: nil)?
            .withSymbolConfiguration(.init(pointSize: NSFont.menuFont(ofSize: 0).pointSize, weight: .regular))
    }

    /// A copy at an item image's 16 points.
    private static func menuSized(_ image: NSImage) -> NSImage {
        let copy = image.copy() as! NSImage
        copy.size = NSSize(width: 16, height: 16)
        return copy
    }

    /// The image beside a place row: its symbol, or the app's own icon, running or installed.
    private static func image(for place: Place) -> NSImage? {
        guard case .app(let bundleId) = place else { return MenuArt.symbol(for: place).flatMap(symbol) }
        let running = NSRunningApplication.runningApplications(withBundleIdentifier: bundleId).first?.icon
        let installed = NSWorkspace.shared.urlForApplication(withBundleIdentifier: bundleId)
            .map { NSWorkspace.shared.icon(forFile: $0.path) }
        return (running ?? installed).map(menuSized)
    }

    /// A place row: a submenu that turns its place the other way, or a greyed item with only what
    /// its place says, while it has nothing to offer (ADR-0018).
    private func rowItem(_ row: Menu.PlaceRow) -> NSMenuItem {
        let item = NSMenuItem(title: row.title, action: nil, keyEquivalent: "")
        // A site's row names the site on the first line under its title, unwrapped.
        let lines = [row.site, row.subtitle.map(MenuArt.wrapped)].compactMap { $0 }
        item.subtitle = lines.isEmpty ? nil : lines.joined(separator: "\n")
        item.image = row.site.flatMap(icons.icon(for:)).map(Self.menuSized) ?? Self.image(for: row.place)
        if let site = row.site { siteRow = (site, item) }
        Self.showImage(item)
        // A site's row sits under its browser's.
        if case .domain = row.place { item.indentationLevel = 1 }
        guard !row.items.isEmpty else {
            item.isEnabled = false
            return item
        }
        let submenu = NSMenu(title: row.title)
        submenu.autoenablesItems = false
        for entry in row.items {
            switch entry {
            case .choice(let choice):
                submenu.addItem(menuItem(choice))
            case .pickTime(let pick):
                let custom = NSMenuItem(title: pick.item, action: #selector(pickTime(_:)), keyEquivalent: "")
                custom.target = self
                custom.representedObject = Pick(pick)
                submenu.addItem(custom)
            case .separator:
                submenu.addItem(.separator())
            }
        }
        item.submenu = submenu
        return item
    }

    @objc private func pickTime(_ sender: NSMenuItem) {
        guard let pick = (sender.representedObject as? Pick)?.pick else { return }
        let picker = TimePicker(
            title: pick.title, place: pick.place, state: pick.state, start: pick.start, words: words,
            choose: choose,
            onClose: { [weak self] in self?.picking = nil })
        picking = picker
        // After the menu has closed, which would otherwise take the focus back.
        DispatchQueue.main.async { picker.show() }
    }

    /// The strength's slider, between a small sun where the items' titles start and a large one
    /// at the menu's edge.
    private func strengthItem(_ value: UInt8) -> NSMenuItem {
        let slider = StrengthSlider(
            value: Double(value), minValue: 0, maxValue: 100, target: self, action: #selector(strengthMoved(_:)))
        slider.isContinuous = true
        slider.onRelease = { [weak self, weak slider] in
            guard let self, let slider else { return }
            choose(.strength(UInt8(slider.doubleValue.rounded())))
        }
        slider.controlSize = .small
        slider.setAccessibilityLabel(words.strength)
        slider.translatesAutoresizingMaskIntoConstraints = false
        let view = NSView(frame: NSRect(x: 0, y: 0, width: 240, height: 26))
        view.autoresizingMask = [.width]
        let suns = [MenuArt.sun(large: false), MenuArt.sun(large: true)].map { image in
            let sun = NSImageView(image: image)
            sun.contentTintColor = .secondaryLabelColor
            sun.translatesAutoresizingMaskIntoConstraints = false
            view.addSubview(sun)
            return sun
        }
        view.addSubview(slider)
        NSLayoutConstraint.activate([
            suns[0].leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: Self.titleInset),
            suns[0].centerYAnchor.constraint(equalTo: view.centerYAnchor),
            slider.leadingAnchor.constraint(equalTo: suns[0].trailingAnchor, constant: 6),
            slider.trailingAnchor.constraint(equalTo: suns[1].leadingAnchor, constant: -6),
            slider.centerYAnchor.constraint(equalTo: view.centerYAnchor),
            suns[1].trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -14),
            suns[1].centerYAnchor.constraint(equalTo: view.centerYAnchor),
        ])
        self.slider = slider
        let item = NSMenuItem()
        item.view = view
        return item
    }

    /// A drag previews the strength, and the release saves it. Moving it any other way, such as
    /// with VoiceOver, saves it at once.
    @objc private func strengthMoved(_ sender: StrengthSlider) {
        let value = UInt8(sender.doubleValue.rounded())
        choose(sender.dragging ? .previewStrength(value) : .strength(value))
    }

    private func menuItem(_ item: Menu.Choice) -> NSMenuItem {
        let menuItem = NSMenuItem(title: item.title, action: #selector(chose(_:)), keyEquivalent: "")
        menuItem.target = self
        menuItem.subtitle = item.subtitle.map(MenuArt.wrapped)
        menuItem.isEnabled = item.enabled
        menuItem.representedObject = Choice(item.command)
        return menuItem
    }

    @objc private func chose(_ sender: NSMenuItem) {
        guard let choice = sender.representedObject as? Choice else { return }
        choose(choice.command)
    }

    @objc private func settingsChosen() {
        openSettings(false)
    }

    @objc private func problemLineChosen() {
        openSettings(true)
    }

    @objc private func quit() {
        NSApp.terminate(nil)
    }
}

/// "Nightcap is up here", under the moon once onboarding is done, at a fixed width with the
/// height its words need. With the moon hidden, it says so instead, without the tour, which
/// works through the moon.
final class PointOut: NSViewController {
    private static let width: CGFloat = 300
    private let onOK: () -> Void
    private let takeTour: () -> Void
    private(set) var size = NSSize(width: width, height: 0)

    init(hidden: Bool, onOK: @escaping () -> Void, takeTour: @escaping () -> Void) {
        self.onOK = onOK
        self.takeTour = takeTour
        super.init(nibName: nil, bundle: nil)
        let words = Wording.system
        let said = hidden ? words.noRoomForTheMoon : words.upHere
        let title = Look.label(said.title, size: 13, weight: .semibold, color: .labelColor, width: Self.width - 32)
        // The system's label colours, which stay readable on the popover's glass.
        let text = Look.label(
            said.body, size: 12, weight: .regular, color: .secondaryLabelColor, width: Self.width - 32)
        let ok = Look.button(words.ok, .primary) { [weak self] in self?.onOK() }
        let tour = Look.button(words.takeTheTour, .regular) { [weak self] in self?.takeTour() }
        let buttons = NSStackView(views: hidden ? [NSView(), ok] : [NSView(), tour, ok])
        buttons.orientation = .horizontal
        let stack = NSStackView(views: [title, text, buttons])
        stack.orientation = .vertical
        stack.alignment = .leading
        stack.spacing = 6
        stack.setCustomSpacing(12, after: text)
        stack.edgeInsets = NSEdgeInsets(top: 14, left: 16, bottom: 14, right: 16)
        stack.widthAnchor.constraint(equalToConstant: Self.width).isActive = true
        buttons.widthAnchor.constraint(equalTo: stack.widthAnchor, constant: -32).isActive = true
        size = NSSize(width: Self.width, height: stack.fittingSize.height)
        view = stack
    }

    required init?(coder: NSCoder) {
        fatalError("not built from a nib")
    }
}
