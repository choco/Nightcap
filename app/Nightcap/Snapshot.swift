#if DEBUG
import AppKit
import NightcapCore

/// Dev builds only: with `NIGHTCAP_SNAPSHOT` set to a folder, Nightcap renders the settings
/// window's tabs, light and dark, from the mockup's scenario into that folder, then quits.
enum Snapshot {
    static func runIfAsked() {
        guard let folder = ProcessInfo.processInfo.environment["NIGHTCAP_SNAPSHOT"] else { return }
        let settings = Settings(
            websiteChecking: SettingsSwitch(on: true, locked: false, command: .websites(.off)),
            openAtLogin: SettingsSwitch(on: true, locked: false, command: .openAtLogin(.off)),
            checks: [
                SettingsCheck(
                    kind: .nightShift, state: .ok,
                    text: "Available, following its schedule: sunset to sunrise.", restore: nil, accept: nil),
                SettingsCheck(
                    kind: .accessibility, state: .drifted,
                    text: "Accessibility isn’t allowed, so website checking can’t read the front tab.",
                    restore: FixButton(title: "Allow…", fix: .askForAccessibility),
                    accept: FixButton(title: "Stop Checking Websites", fix: .stopCheckingWebsites)),
                SettingsCheck(
                    kind: .notifications, state: .drifted,
                    text: "Notifications were allowed, but they’re switched off in System Settings.",
                    restore: FixButton(title: "Allow Notifications…", fix: .allowNotifications),
                    accept: FixButton(title: "Keep Notifications Off", fix: .keepNotificationsOff)),
                SettingsCheck(
                    kind: .configFile, state: .broken,
                    text: "The config file is invalid: line 4: unknown field `domian`.",
                    restore: FixButton(title: "Reload", fix: .reload), accept: nil),
                SettingsCheck(kind: .adHocRules, state: .ok, text: "2 ad-hoc rules.", restore: nil, accept: nil),
            ])
        let window = SettingsWindow(
            current: { settings }, choose: { _ in }, perform: { _ in }, checkForUpdates: {}, onClose: {})
        for (name, appearance) in [("light", NSAppearance.Name.aqua), ("dark", .darkAqua)] {
            NSApp.appearance = NSAppearance(named: appearance)
            for tab in SettingsWindow.Tab.allCases {
                let url = URL(fileURLWithPath: folder).appendingPathComponent("app-\(tab.rawValue)-\(name).png")
                guard let png = window.snapshot(tab) else { fatalError("couldn't render \(tab)") }
                try! png.write(to: url)
            }
        }
        for icon in [MenuIcon.on, .off, .forcedOff] {
            for attention in [false, true] {
                let name = "icon-\(icon)-\(attention ? "attention" : "plain").png"
                try! menuBarIcon(icon, attention: attention)
                    .write(to: URL(fileURLWithPath: folder).appendingPathComponent(name))
            }
        }
        exit(0)
    }

    /// The menu-bar icon at 8x, black on white as a template draws in a light menu bar, with the
    /// dot where the status item lays it over the moon.
    private static func menuBarIcon(_ icon: MenuIcon, attention: Bool) -> Data {
        let image = StatusMenu.image(for: icon)!
        let scale: CGFloat = 8
        let size = NSSize(width: image.size.width * scale + 64, height: image.size.height * scale + 64)
        let bitmap = NSBitmapImageRep(
            bitmapDataPlanes: nil, pixelsWide: Int(size.width), pixelsHigh: Int(size.height),
            bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
            colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
        NSGraphicsContext.saveGraphicsState()
        NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: bitmap)
        NSColor.white.setFill()
        NSRect(origin: .zero, size: size).fill()
        image.draw(in: NSRect(x: 32, y: 32, width: size.width - 64, height: size.height - 64))
        if attention {
            let diameter = StatusMenu.dotSize * scale
            let centre = NSPoint(
                x: size.width / 2 + StatusMenu.dotOffset.x * scale,
                y: size.height / 2 + StatusMenu.dotOffset.y * scale)
            NSColor.systemOrange.setFill()
            NSBezierPath(ovalIn: NSRect(
                x: centre.x - diameter / 2, y: centre.y - diameter / 2, width: diameter, height: diameter)).fill()
        }
        let warning = StatusMenu.warning!
        warning.draw(in: NSRect(x: 0, y: 0, width: warning.size.width * 3, height: warning.size.height * 3))
        NSGraphicsContext.restoreGraphicsState()
        return bitmap.representation(using: .png, properties: [:])!
    }
}
#endif
