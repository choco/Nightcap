import AppKit
import KeyboardShortcuts
import NightcapCore

/// Nightcap's menu-bar item. The core decides what it shows (`Core.menu()`); this only draws
/// it, rebuilding the menu each time it opens, and sends the chosen command back.
final class StatusMenu: NSObject, NSMenuDelegate {
    private let item = NSStatusBar.system.statusItem(withLength: NSStatusItem.squareLength)
    private let current: () -> Menu
    private let choose: (Command) -> Void
    /// Opens Settings: on the problem's tab for the problem line, on the last tab otherwise.
    private let openSettings: (_ onProblem: Bool) -> Void
    private let onOpen: () -> Void
    private let onClose: () -> Void
    /// What the open menu shows, so `refresh()` redraws it only when that changed.
    private var shown: Menu?
    /// The open menu's strength slider, moved in place when only the strength changed.
    private weak var slider: NSSlider?
    /// "Nightcap is up here", while it shows.
    private var pointing: NSPopover?
    /// The mockup's orange dot, over the template moon so the moon still follows the menu
    /// bar's appearance while the dot keeps its colour.
    private let dot = NSView()

    /// Boxes a command for `NSMenuItem.representedObject`, which takes only objects.
    private final class Choice: NSObject {
        let command: Command
        init(_ command: Command) { self.command = command }
    }

    init(
        current: @escaping () -> Menu,
        choose: @escaping (Command) -> Void,
        openSettings: @escaping (_ onProblem: Bool) -> Void,
        onOpen: @escaping () -> Void,
        onClose: @escaping () -> Void
    ) {
        self.current = current
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
            dot.layer?.cornerRadius = Self.dotSize / 2
            dot.translatesAutoresizingMaskIntoConstraints = false
            button.addSubview(dot)
            NSLayoutConstraint.activate([
                dot.widthAnchor.constraint(equalToConstant: Self.dotSize),
                dot.heightAnchor.constraint(equalToConstant: Self.dotSize),
                dot.centerXAnchor.constraint(equalTo: button.centerXAnchor, constant: Self.dotOffset.x),
                dot.centerYAnchor.constraint(equalTo: button.centerYAnchor, constant: -Self.dotOffset.y),
            ])
        }
        refresh()
    }

    static let dotSize: CGFloat = 5.5
    /// From the icon's centre to its top right corner, as in the mockup: clear of the forced
    /// moon's "z", and inside the status item's square.
    static let dotOffset = CGPoint(x: 9.2, y: 7.5)

    /// Redraws the icon, and the menu too while it's open and what it shows changed. A new
    /// strength alone moves the slider in place, since rebuilding the menu would end a drag.
    func refresh() {
        let snapshot = current()
        let image = Self.image(for: snapshot.icon)
        if snapshot.problemLine != nil, let description = image?.accessibilityDescription {
            image?.accessibilityDescription = "\(description), needs attention"
        }
        item.button?.image = image
        item.button?.appearsDisabled = snapshot.icon == .unavailable
        dot.isHidden = snapshot.problemLine == nil
        guard let shown, snapshot != shown, let menu = item.menu else { return }
        if let slider, Self.apartFromStrength(snapshot) == Self.apartFromStrength(shown),
           let strength = Self.strength(in: snapshot)
        {
            self.shown = snapshot
            if NSEvent.pressedMouseButtons == 0 { slider.doubleValue = Double(strength) }
        } else {
            menuNeedsUpdate(menu)
        }
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

    /// Points at the moon once onboarding is done: the only way into Nightcap from then on. It
    /// stays until OK: on macOS 27, a status item's click never arrives while a popover hangs
    /// from it. A popover, since on macOS 27 the system draws the status item, and only AppKit
    /// knows where it is. It offers the tour, which then starts knowing where the moon is.
    /// `onShown` runs once it's up and focused.
    func pointOut(takeTour: @escaping () -> Void, onShown: @escaping () -> Void) {
        // A moment for the new status item to take its place in the menu bar.
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.3) { [weak self] in
            guard let self, let button = item.button else { return }
            let popover = NSPopover()
            popover.behavior = .applicationDefined
            let content = PointOut(
                onOK: { [weak self] in self?.stopPointing() },
                takeTour: { [weak self] in
                    self?.stopPointing()
                    takeTour()
                })
            popover.contentViewController = content
            popover.contentSize = content.size
            // Focused, as a popover the user opened would be; without it the popover's glass washes
            // out. Nightcap is still the active app from the click on Done, so nothing else loses
            // the focus to it.
            NSApp.activate()
            popover.show(relativeTo: button.bounds, of: button, preferredEdge: .minY)
            popover.contentViewController?.view.window?.makeKey()
            pointing = popover
            onShown()
        }
    }

    private func stopPointing() {
        pointing?.close()
        pointing = nil
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

    /// The mockup's moons, template images from the asset catalog.
    static func image(for icon: MenuIcon) -> NSImage? {
        let (name, description) =
            switch icon {
            case .on: ("MenuBarOn", "Night Shift on")
            case .off: ("MenuBarOff", "Night Shift off")
            case .forcedOff: ("MenuBarForcedOff", "Night Shift forced off")
            case .unavailable: ("MenuBarOff", "Night Shift unavailable")
            }
        let image = NSImage(named: name)?.copy() as? NSImage
        image?.accessibilityDescription = description
        return image
    }

    /// The mockup's orange triangle, for the checkmark column. Drawn rather than an SF Symbol,
    /// which menus draw as a monochrome template whatever its colours. The "!" is cut out, so
    /// it still shows when a highlighted item turns the image white.
    static var warning: NSImage? {
        let side: CGFloat = 14
        let image = NSImage(size: NSSize(width: side, height: side), flipped: true) { _ in
            // The mockup's 20-point geometry, scaled to the image.
            let unit = side / 20
            let point = { (x: CGFloat, y: CGFloat) in NSPoint(x: x * unit, y: y * unit) }
            let triangle = NSBezierPath()
            triangle.move(to: point(10, 1.8))
            triangle.line(to: point(19, 17.6))
            triangle.line(to: point(1, 17.6))
            triangle.close()
            triangle.lineJoinStyle = .round
            triangle.lineWidth = 1.5 * unit
            NSColor.systemOrange.setFill()
            NSColor.systemOrange.setStroke()
            triangle.fill()
            triangle.stroke()
            let bar = NSBezierPath()
            bar.move(to: point(10, 7.2))
            bar.line(to: point(10, 11.8))
            bar.lineWidth = 2 * unit
            bar.lineCapStyle = .round
            NSGraphicsContext.current?.compositingOperation = .clear
            bar.stroke()
            NSBezierPath(ovalIn: NSRect(
                x: (10 - 1.1) * unit, y: (14.6 - 1.1) * unit, width: 2.2 * unit, height: 2.2 * unit)).fill()
            return true
        }
        image.accessibilityDescription = "Needs attention"
        return image
    }

    func menuNeedsUpdate(_ menu: NSMenu) {
        let snapshot = current()
        shown = snapshot
        menu.removeAllItems()
        let status = NSMenuItem(title: snapshot.status, action: nil, keyEquivalent: "")
        status.attributedTitle = NSAttributedString(
            string: snapshot.status,
            attributes: [.font: NSFont.boldSystemFont(ofSize: NSFont.menuFont(ofSize: 0).pointSize)])
        status.subtitle = snapshot.reason
        status.isEnabled = false
        menu.addItem(status)
        if let problemLine = snapshot.problemLine {
            let line = NSMenuItem(title: problemLine.title, action: #selector(problemLineChosen), keyEquivalent: "")
            line.subtitle = problemLine.subtitle
            line.onStateImage = Self.warning
            line.state = .on
            menu.addItem(line)
        }
        menu.addItem(.separator())
        for entry in snapshot.entries {
            switch entry {
            case .strength(let value):
                menu.addItem(strengthItem(value))
            case .item(let item):
                menu.addItem(menuItem(item))
            case .submenu(let title, let items):
                let parent = NSMenuItem(title: title, action: nil, keyEquivalent: "")
                let submenu = NSMenu(title: title)
                submenu.autoenablesItems = false
                items.forEach { submenu.addItem(menuItem($0)) }
                parent.submenu = submenu
                menu.addItem(parent)
            case .header(let title):
                menu.addItem(.sectionHeader(title: title))
            case .separator:
                menu.addItem(.separator())
            }
        }
        menu.addItem(.separator())
        menu.addItem(NSMenuItem(title: "Settings…", action: #selector(settingsChosen), keyEquivalent: ","))
        menu.addItem(NSMenuItem(title: "Quit Nightcap", action: #selector(quit), keyEquivalent: "q"))
        menu.items.filter { $0.action != nil }.forEach { $0.target = self }
    }

    /// The strength's slider, inset like the items' titles and as wide as the menu.
    private func strengthItem(_ value: UInt8) -> NSMenuItem {
        let slider = StrengthSlider(
            value: Double(value), minValue: 0, maxValue: 100, target: self, action: #selector(strengthMoved(_:)))
        slider.isContinuous = true
        slider.onRelease = { [weak self, weak slider] in
            guard let self, let slider else { return }
            choose(.strength(UInt8(slider.doubleValue.rounded())))
        }
        slider.controlSize = .small
        slider.setAccessibilityLabel("Strength")
        slider.translatesAutoresizingMaskIntoConstraints = false
        let view = NSView(frame: NSRect(x: 0, y: 0, width: 240, height: 26))
        view.autoresizingMask = [.width]
        view.addSubview(slider)
        NSLayoutConstraint.activate([
            slider.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: 14),
            slider.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -14),
            slider.centerYAnchor.constraint(equalTo: view.centerYAnchor),
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

    private func menuItem(_ item: MenuItem) -> NSMenuItem {
        let menuItem = NSMenuItem(title: item.title, action: #selector(chose(_:)), keyEquivalent: "")
        menuItem.target = self
        menuItem.subtitle = item.subtitle
        menuItem.indentationLevel = item.indented ? 1 : 0
        menuItem.state = item.checked ? .on : .off
        menuItem.isEnabled = item.enabled
        menuItem.representedObject = Choice(item.command)
        if item.command == .toggle, let shortcut = KeyboardShortcuts.getShortcut(for: .nightShift) {
            // Shown only: while a menu is open, KeyboardShortcuts takes the keys before the menu.
            menuItem.keyEquivalent = shortcut.nsMenuItemKeyEquivalent ?? ""
            menuItem.keyEquivalentModifierMask = shortcut.modifiers
            menuItem.allowsAutomaticKeyEquivalentLocalization = false
        }
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
/// height its words need.
private final class PointOut: NSViewController {
    private static let width: CGFloat = 300
    private let onOK: () -> Void
    private let takeTour: () -> Void
    private(set) var size = NSSize(width: width, height: 0)

    init(onOK: @escaping () -> Void, takeTour: @escaping () -> Void) {
        self.onOK = onOK
        self.takeTour = takeTour
        super.init(nibName: nil, bundle: nil)
        let title = Look.label("Nightcap is up here", size: 13, weight: .semibold, color: .labelColor, width: Self.width - 32)
        // The system's label colours, which stay readable on the popover's glass.
        let text = Look.label(
            "Its menu turns Night Shift on or off, changes its strength, and forces it off in the app in front: click OK, then the moon. New to Nightcap? The tour shows a rule at work, in about a minute.",
            size: 12, weight: .regular, color: .secondaryLabelColor, width: Self.width - 32)
        let ok = Look.button("OK", .primary) { [weak self] in self?.onOK() }
        let tour = Look.button("Take the Tour", .regular) { [weak self] in self?.takeTour() }
        let buttons = NSStackView(views: [NSView(), tour, ok])
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
