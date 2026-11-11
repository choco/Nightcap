import AppKit

/// The menu bar's menus, shown only while Nightcap is in the Dock.
enum MainMenu {
    static func make() -> NSMenu {
        let words = Wording.system
        let appMenu = NSMenu()
        appMenu.addItem(
            withTitle: words.aboutNightcap,
            action: #selector(NSApplication.orderFrontStandardAboutPanel(_:)),
            keyEquivalent: "")
        appMenu.addItem(.separator())
        appMenu.addItem(withTitle: words.settings, action: #selector(AppDelegate.openSettings(_:)), keyEquivalent: ",")
        appMenu.addItem(.separator())
        appMenu.addItem(withTitle: words.hideNightcap, action: #selector(NSApplication.hide(_:)), keyEquivalent: "h")
        appMenu.addItem(.separator())
        appMenu.addItem(
            withTitle: words.quitNightcap, action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        let appItem = NSMenuItem()
        appItem.submenu = appMenu
        let menu = NSMenu()
        menu.addItem(appItem)
        return menu
    }
}
