import AppKit

/// The mockups' palette, light and dark.
extension NSColor {
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

    static var nightcapWindow: NSColor { appearance(light: 0xf4f4f6, dark: 0x1e1e21) }
    static var nightcapGroup: NSColor { appearance(light: 0xffffff, dark: 0x2b2b2e) }
    static var nightcapSeparator: NSColor {
        appearance(light: 0x000000, dark: 0xffffff, lightAlpha: 0.1, darkAlpha: 0.09)
    }
    static var nightcapSecondary: NSColor { appearance(light: 0x5f5f64, dark: 0xa9a9af) }
    static var nightcapControl: NSColor { appearance(light: 0xffffff, dark: 0x3a3a3e) }
    static var nightcapControlPressed: NSColor { appearance(light: 0xe8e8ec, dark: 0x4a4a4f) }
    static var nightcapControlBorder: NSColor {
        appearance(light: 0x000000, dark: 0xffffff, lightAlpha: 0.16, darkAlpha: 0.12)
    }
    static var nightcapTile: NSColor {
        appearance(light: 0x000000, dark: 0xffffff, lightAlpha: 0.07, darkAlpha: 0.1)
    }
    /// The design system's gold for marks: its `moon-text` by day, since the moon itself is too
    /// pale to see on a light window, and the moon at night.
    static var nightcapMoon: NSColor { appearance(light: 0x8a5b00, dark: 0xf6d58a) }
    static var nightcapGreen: NSColor { appearance(light: 0x22b33a, dark: 0x30d158) }
    static var nightcapOrange: NSColor { appearance(light: 0xf08a00, dark: 0xff9f0a) }
    static var nightcapRed: NSColor { appearance(light: 0xe5372c, dark: 0xff453a) }
    static var nightcapGrey: NSColor { appearance(light: 0xa1a1a6, dark: 0x6e6e73) }
}
