import AppKit
import NightcapCore

/// Nightcap's menu-bar item. The core decides what it shows (`Core.menu()`); this only draws
/// it, rebuilding the menu each time it opens, and sends the chosen command back.
final class StatusMenu: NSObject, NSMenuDelegate {
    private let item = NSStatusBar.system.statusItem(withLength: NSStatusItem.squareLength)
    private let current: () -> Menu
    private let choose: (Command) -> Void
    private let askForAccessibility: () -> Void
    /// Nil unless Sparkle runs, which it does in self-managed installs only.
    private let checkForUpdates: (() -> Void)?

    /// Boxes a command for `NSMenuItem.representedObject`, which takes only objects.
    private final class Choice: NSObject {
        let command: Command
        init(_ command: Command) { self.command = command }
    }

    init(
        current: @escaping () -> Menu,
        choose: @escaping (Command) -> Void,
        askForAccessibility: @escaping () -> Void,
        checkForUpdates: (() -> Void)?
    ) {
        self.current = current
        self.choose = choose
        self.askForAccessibility = askForAccessibility
        self.checkForUpdates = checkForUpdates
        super.init()
        let menu = NSMenu()
        menu.delegate = self
        menu.autoenablesItems = false
        item.menu = menu
        refresh()
    }

    /// Redraws the icon; the menu itself is rebuilt when it opens.
    func refresh() {
        let icon = current().icon
        let (symbol, description) =
            switch icon {
            case .on: ("moon.fill", "Night Shift on")
            case .off: ("moon", "Night Shift off")
            case .forcedOff: ("moon.zzz", "Night Shift forced off")
            case .unavailable: ("moon", "Night Shift unavailable")
            }
        let image = NSImage(systemSymbolName: symbol, accessibilityDescription: description)
        image?.isTemplate = true
        item.button?.image = image
        item.button?.appearsDisabled = icon == .unavailable
    }

    func menuNeedsUpdate(_ menu: NSMenu) {
        let snapshot = current()
        menu.removeAllItems()
        let status = NSMenuItem(title: snapshot.status, action: nil, keyEquivalent: "")
        status.attributedTitle = NSAttributedString(
            string: snapshot.status, attributes: [.font: NSFont.menuFont(ofSize: 0).bold])
        status.subtitle = snapshot.reason
        status.isEnabled = false
        menu.addItem(status)
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
            case .askForAccessibility(let title):
                menu.addItem(NSMenuItem(title: title, action: #selector(allowAccessibility), keyEquivalent: ""))
            case .separator:
                menu.addItem(.separator())
            }
        }
        menu.addItem(.separator())
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

    @objc private func allowAccessibility() {
        askForAccessibility()
    }

    @objc private func checkNow() {
        checkForUpdates?()
    }

    @objc private func quit() {
        NSApp.terminate(nil)
    }
}

private extension NSFont {
    var bold: NSFont {
        NSFontManager.shared.convert(self, toHaveTrait: .boldFontMask)
    }
}
