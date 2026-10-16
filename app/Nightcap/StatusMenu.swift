import AppKit
import NightcapCore

/// Nightcap's menu-bar item. The core decides what it shows (`Core.menu()`); this only draws
/// it, rebuilding the menu each time it opens, and sends the chosen command back.
final class StatusMenu: NSObject, NSMenuDelegate {
    private let item = NSStatusBar.system.statusItem(withLength: NSStatusItem.squareLength)
    private let current: () -> Menu
    private let choose: (Command) -> Void
    private let openSettings: () -> Void
    private let onOpen: () -> Void
    /// Nil unless Sparkle runs, which it does in self-managed installs only.
    private let checkForUpdates: (() -> Void)?
    /// What the open menu shows, so `refresh()` redraws it only when that changed.
    private var shown: Menu?
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
        openSettings: @escaping () -> Void,
        onOpen: @escaping () -> Void,
        checkForUpdates: (() -> Void)?
    ) {
        self.current = current
        self.choose = choose
        self.openSettings = openSettings
        self.onOpen = onOpen
        self.checkForUpdates = checkForUpdates
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

    /// Redraws the icon, and the menu too while it's open and what it shows changed.
    func refresh() {
        let snapshot = current()
        let image = Self.image(for: snapshot.icon)
        if snapshot.attention != nil, let description = image?.accessibilityDescription {
            image?.accessibilityDescription = "\(description), needs attention"
        }
        item.button?.image = image
        item.button?.appearsDisabled = snapshot.icon == .unavailable
        dot.isHidden = snapshot.attention == nil
        if shown != nil, snapshot != shown, let menu = item.menu {
            menuNeedsUpdate(menu)
        }
    }

    /// `onOpen` can learn something the menu should show, which then redraws it in place.
    func menuWillOpen(_ menu: NSMenu) {
        onOpen()
    }

    func menuDidClose(_ menu: NSMenu) {
        shown = nil
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
        if let attention = snapshot.attention {
            let line = NSMenuItem(title: attention.title, action: #selector(settingsChosen), keyEquivalent: "")
            line.subtitle = attention.subtitle
            line.onStateImage = Self.warning
            line.state = .on
            menu.addItem(line)
        }
        menu.addItem(.separator())
        for entry in snapshot.entries {
            switch entry {
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
        if checkForUpdates != nil {
            menu.addItem(NSMenuItem(title: "Check for Updates…", action: #selector(checkNow), keyEquivalent: ""))
        }
        menu.addItem(NSMenuItem(title: "Quit Nightcap", action: #selector(quit), keyEquivalent: "q"))
        menu.items.filter { $0.action != nil }.forEach { $0.target = self }
    }

    private func menuItem(_ item: MenuItem) -> NSMenuItem {
        let menuItem = NSMenuItem(title: item.title, action: #selector(chose(_:)), keyEquivalent: "")
        menuItem.target = self
        menuItem.subtitle = item.subtitle
        menuItem.indentationLevel = item.indented ? 1 : 0
        menuItem.state = item.checked ? .on : .off
        menuItem.isEnabled = item.enabled
        menuItem.representedObject = Choice(item.command)
        return menuItem
    }

    @objc private func chose(_ sender: NSMenuItem) {
        guard let choice = sender.representedObject as? Choice else { return }
        choose(choice.command)
    }

    @objc private func settingsChosen() {
        openSettings()
    }

    @objc private func checkNow() {
        checkForUpdates?()
    }

    @objc private func quit() {
        NSApp.terminate(nil)
    }
}
