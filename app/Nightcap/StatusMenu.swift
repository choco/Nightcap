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
    /// Custom…'s window, while it shows.
    private var picking: TimePicker?
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
        let title: String
        let place: Place
        let state: OnOff
        let start: Date
        init(title: String, place: Place, state: OnOff, start: Date) {
            self.title = title
            self.place = place
            self.state = state
            self.start = start
        }
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
    /// Where items' titles start, in the slider's view, which puts its small sun there as the
    /// mockup does; and the widest a subtitle's line gets.
    private static let titleInset: CGFloat = 46
    private static let subtitleWidth: CGFloat = 160
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

    /// The mockup's orange triangle, beside the problem line. Drawn rather than an SF Symbol,
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
        if let problemLine = snapshot.problemLine {
            let line = NSMenuItem(title: problemLine.title, action: #selector(problemLineChosen), keyEquivalent: "")
            line.subtitle = problemLine.subtitle.map(Self.wrapped)
            line.image = Self.warning
            Self.showImage(line)
            menu.addItem(line)
            menu.addItem(.separator())
        }
        menu.addItem(nightShiftItem(snapshot))
        for entry in snapshot.entries {
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
        let settings = NSMenuItem(title: "Settings…", action: #selector(settingsChosen), keyEquivalent: ",")
        settings.image = Self.symbol("gear")
        Self.showImage(settings)
        menu.addItem(settings)
        let quit = NSMenuItem(title: "Quit Nightcap", action: #selector(quit), keyEquivalent: "q")
        quit.image = Self.symbol("power")
        Self.showImage(quit)
        menu.addItem(quit)
        menu.items.filter { $0.action != nil }.forEach { $0.target = self }
    }

    /// "Night Shift", checked while it's on everywhere, with the menu-bar moon's look: what
    /// decides the screen, and the shortcut that does the same.
    private func nightShiftItem(_ menu: Menu) -> NSMenuItem {
        let state = menu.nightShift
        let item = NSMenuItem(title: "Night Shift", action: #selector(chose(_:)), keyEquivalent: "")
        item.image = Self.image(for: menu.icon).map(Self.menuSized)
        Self.showImage(item)
        item.state = state.on ? .on : .off
        item.subtitle = state.reason.map(Self.wrapped)
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

    /// A subtitle broken into lines at words, at most `subtitleWidth` wide: a menu is as wide as
    /// its longest line, and NSMenuItem never wraps one itself. Two lines break where they come
    /// out most even, so neither ends on a lone word.
    static func wrapped(_ text: String) -> String {
        // The size macOS 27 sets subtitles in.
        let font = NSFont.menuFont(ofSize: 12)
        let width = { (line: String) in (line as NSString).size(withAttributes: [.font: font]).width }
        guard width(text) > subtitleWidth else { return text }
        let words = text.split(separator: " ").map(String.init)
        let halves = (1..<words.count).map { split in
            (words[..<split].joined(separator: " "), words[split...].joined(separator: " "))
        }
        if let even = halves.filter({ width($0.0) <= subtitleWidth && width($0.1) <= subtitleWidth })
            .min(by: { max(width($0.0), width($0.1)) < max(width($1.0), width($1.1)) })
        {
            return "\(even.0)\n\(even.1)"
        }
        var lines: [String] = []
        var line = ""
        for word in text.split(separator: " ") {
            let longer = line.isEmpty ? String(word) : "\(line) \(word)"
            if !line.isEmpty, width(longer) > subtitleWidth {
                lines.append(line)
                line = String(word)
            } else {
                line = longer
            }
        }
        lines.append(line)
        return lines.joined(separator: "\n")
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

    /// The mockup's suns at the slider's ends: a small one with four rays for less warmth, a
    /// large one with eight for more. Templates, so they take the menu's text colour.
    static func sun(large: Bool) -> NSImage {
        let side: CGFloat = large ? 16 : 11
        let image = NSImage(size: NSSize(width: side, height: side), flipped: true) { _ in
            let unit = side / 24
            let point = { (x: CGFloat, y: CGFloat) in NSPoint(x: x * unit, y: y * unit) }
            let rays: [(CGFloat, CGFloat, CGFloat, CGFloat)] = large
                ? [(12, 2.5, 12, 5), (12, 19, 12, 21.5), (2.5, 12, 5, 12), (19, 12, 21.5, 12),
                   (5.3, 5.3, 7.1, 7.1), (16.9, 16.9, 18.7, 18.7), (5.3, 18.7, 7.1, 16.9), (16.9, 7.1, 18.7, 5.3)]
                : [(12, 3, 12, 5), (12, 19, 12, 21), (3, 12, 5, 12), (19, 12, 21, 12)]
            let path = NSBezierPath(ovalIn: NSRect(
                x: (12 - (large ? 4.2 : 4)) * unit, y: (12 - (large ? 4.2 : 4)) * unit,
                width: (large ? 8.4 : 8) * unit, height: (large ? 8.4 : 8) * unit))
            for (x0, y0, x1, y1) in rays {
                path.move(to: point(x0, y0))
                path.line(to: point(x1, y1))
            }
            path.lineWidth = (large ? 2 : 2.4) * unit
            path.lineCapStyle = .round
            NSColor.black.setStroke()
            path.stroke()
            return true
        }
        image.isTemplate = true
        return image
    }

    /// A copy at an item image's 16 points.
    private static func menuSized(_ image: NSImage) -> NSImage {
        let copy = image.copy() as! NSImage
        copy.size = NSSize(width: 16, height: 16)
        return copy
    }

    /// The image beside a place row: the timer for everywhere, under the switch, the app's own
    /// icon, or a globe for a site.
    private static func image(for image: RowImage) -> NSImage? {
        switch image {
        case .everywhere: return symbol("timer")
        case .site: return symbol("globe")
        case .app(let bundleId):
            let running = NSRunningApplication.runningApplications(withBundleIdentifier: bundleId).first?.icon
            let installed = NSWorkspace.shared.urlForApplication(withBundleIdentifier: bundleId)
                .map { NSWorkspace.shared.icon(forFile: $0.path) }
            return (running ?? installed).map(menuSized)
        }
    }

    /// A place row: a submenu that turns its place the other way, or a greyed item with only what
    /// its place says, while it has nothing to offer (ADR-0024).
    private func rowItem(_ row: PlaceRow) -> NSMenuItem {
        let item = NSMenuItem(title: row.title, action: nil, keyEquivalent: "")
        // A site's row names the site on the first line under its title, unwrapped.
        let lines = [row.site, row.subtitle.map(Self.wrapped)].compactMap { $0 }
        item.subtitle = lines.isEmpty ? nil : lines.joined(separator: "\n")
        item.image = Self.image(for: row.image)
        Self.showImage(item)
        item.indentationLevel = row.indented ? 1 : 0
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
            case .pickTime(let title, let place, let state, let start):
                let custom = NSMenuItem(title: "Custom…", action: #selector(pickTime(_:)), keyEquivalent: "")
                custom.target = self
                custom.representedObject = Pick(title: title, place: place, state: state, start: start)
                submenu.addItem(custom)
            case .separator:
                submenu.addItem(.separator())
            }
        }
        item.submenu = submenu
        return item
    }

    @objc private func pickTime(_ sender: NSMenuItem) {
        guard let pick = sender.representedObject as? Pick else { return }
        let picker = TimePicker(
            title: pick.title, place: pick.place, state: pick.state, start: pick.start,
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
        slider.setAccessibilityLabel("Strength")
        slider.translatesAutoresizingMaskIntoConstraints = false
        let view = NSView(frame: NSRect(x: 0, y: 0, width: 240, height: 26))
        view.autoresizingMask = [.width]
        let suns = [Self.sun(large: false), Self.sun(large: true)].map { image in
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

    private func menuItem(_ item: MenuItem) -> NSMenuItem {
        let menuItem = NSMenuItem(title: item.title, action: #selector(chose(_:)), keyEquivalent: "")
        menuItem.target = self
        menuItem.subtitle = item.subtitle.map(Self.wrapped)
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
            "Its menu turns Night Shift on or off, everywhere or just in the app in front, for as long as you choose, and changes its strength: click OK, then the moon. New to Nightcap? The tour shows a rule at work, in about a minute.",
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
