import AppKit

/// Drawing for the scenes, in flipped views.
enum Paint {
    static var menu: NSColor { .appearance(light: 0xf0f0f2, dark: 0x2a2a2d, lightAlpha: 0.97, darkAlpha: 0.97) }
    static var bar: NSColor { .appearance(light: 0xf6f6f8, dark: 0x1c1c1f, lightAlpha: 0.95, darkAlpha: 0.9) }
    static var chrome: NSColor { .appearance(light: 0xececef, dark: 0x2a2a2d) }
    static var highlight: NSColor { .appearance(light: 0x000000, dark: 0xffffff, lightAlpha: 0.1, darkAlpha: 0.14) }

    /// Night Shift's warmth, the brand's `tint`.
    static func warm(_ alpha: CGFloat) -> NSColor {
        NSColor(srgbRed: 1, green: 138 / 255, blue: 46 / 255, alpha: alpha)
    }

    static func alpha(for strength: UInt8) -> CGFloat {
        0.08 + 0.34 * CGFloat(strength) / 100
    }

    static func rounded(_ rect: NSRect, _ radius: CGFloat, _ fill: NSColor, stroke: NSColor? = nil) {
        let path = NSBezierPath(roundedRect: rect, xRadius: radius, yRadius: radius)
        fill.setFill()
        path.fill()
        if let stroke {
            stroke.setStroke()
            path.lineWidth = 0.5
            path.stroke()
        }
    }

    /// What follows is drawn with the soft shadow of something floating.
    static func lifted(_ body: () -> Void) {
        NSGraphicsContext.saveGraphicsState()
        let shadow = NSShadow()
        shadow.shadowOffset = NSSize(width: 0, height: -4)
        shadow.shadowBlurRadius = 11
        shadow.shadowColor = NSColor.black.withAlphaComponent(0.16)
        shadow.set()
        body()
        NSGraphicsContext.restoreGraphicsState()
    }

    static func width(_ text: String, _ font: NSFont) -> CGFloat {
        (text as NSString).size(withAttributes: [.font: font]).width
    }

    static func text(_ text: String, _ font: NSFont, _ colour: NSColor, at point: NSPoint) {
        (text as NSString).draw(at: point, withAttributes: [.font: font, .foregroundColor: colour])
    }

    static func text(_ text: String, _ font: NSFont, _ colour: NSColor, centredAt point: NSPoint) {
        let size = (text as NSString).size(withAttributes: [.font: font])
        self.text(text, font, colour, at: NSPoint(x: point.x - size.width / 2, y: point.y - size.height / 2))
    }

    static func glyph(_ symbol: String, _ size: CGFloat, _ colour: NSColor, centredAt point: NSPoint) {
        guard
            let image = NSImage(systemSymbolName: symbol, accessibilityDescription: nil)?
                .withSymbolConfiguration(
                    .init(pointSize: size, weight: .regular).applying(.init(paletteColors: [colour])))
        else { return }
        image.draw(
            in: NSRect(
                x: point.x - image.size.width / 2, y: point.y - image.size.height / 2,
                width: image.size.width, height: image.size.height),
            from: .zero, operation: .sourceOver, fraction: 1, respectFlipped: true, hints: nil)
    }

    /// One of the menu-bar moons, in `colour`.
    static func moon(_ icon: Menu.Moon, _ colour: NSColor, in rect: NSRect) {
        guard let image = MenuArt.moon(for: icon) else { return }
        template(image, colour, in: rect)
    }

    /// A template image, such as the menu's suns, in `colour`.
    static func template(_ image: NSImage, _ colour: NSColor, in rect: NSRect) {
        let tinted = NSImage(size: rect.size, flipped: false) { bounds in
            image.draw(in: bounds)
            colour.set()
            bounds.fill(using: .sourceAtop)
            return true
        }
        tinted.draw(in: rect, from: .zero, operation: .sourceOver, fraction: 1, respectFlipped: true, hints: nil)
    }

    static func trafficLights(at origin: NSPoint, size: CGFloat) {
        for (index, colour) in [NSColor.systemRed, .systemYellow, .systemGreen].enumerated() {
            colour.withAlphaComponent(0.9).setFill()
            NSBezierPath(
                ovalIn: NSRect(
                    x: origin.x + CGFloat(index) * (size + 4), y: origin.y, width: size, height: size)
            ).fill()
        }
    }

    static func toggle(_ rect: NSRect, on: Bool, faded: Bool = false) {
        let alpha: CGFloat = faded ? 0.5 : 1
        rounded(rect, rect.height / 2, on ? NSColor.controlAccentColor.withAlphaComponent(alpha) : .nightcapTile)
        let knob = rect.height - 4
        let x = on ? rect.maxX - knob - 2 : rect.minX + 2
        rounded(
            NSRect(x: x, y: rect.minY + 2, width: knob, height: knob), knob / 2, NSColor.white.withAlphaComponent(alpha)
        )
    }

    /// A light window with lines of text, as a mini screen's windows have; screens stay light, as
    /// a picture of one would.
    static func window(_ rect: NSRect, _ fill: NSColor = .white) {
        lifted { rounded(rect, 5, fill) }
    }

    private static let line = NSColor(white: 0, alpha: 0.08)

    static func page(_ bounds: NSRect) {
        window(bounds)
        rounded(
            NSRect(x: 10, y: 10, width: bounds.width * 0.46, height: 5), 2.5, NSColor.systemBlue.withAlphaComponent(0.8)
        )
        rounded(NSRect(x: 10, y: 22, width: bounds.width * 0.78, height: 5), 2.5, line)
        picture(NSRect(x: 10, y: 36, width: 84, height: 56), dark: false)
        rounded(NSRect(x: 104, y: 40, width: 72, height: 5), 2.5, line)
        rounded(NSRect(x: 104, y: 52, width: 58, height: 5), 2.5, line)
    }

    static func mail(_ bounds: NSRect) {
        window(bounds)
        for (y, share) in [(12.0, 0.5), (26, 0.8), (38, 0.72), (54, 0.6)] {
            rounded(NSRect(x: 10, y: y, width: bounds.width * share, height: 5), 2.5, line)
        }
    }

    /// A design in Figma: two overlapping discs and three swatches.
    static func figma(_ bounds: NSRect) {
        window(bounds, NSColor(srgbRed: 0.97, green: 0.95, blue: 0.91, alpha: 1))
        let disc = min(bounds.height * 0.5, 54)
        NSColor.systemBlue.setFill()
        NSBezierPath(ovalIn: NSRect(x: 20, y: 12, width: disc, height: disc)).fill()
        NSGraphicsContext.saveGraphicsState()
        NSGraphicsContext.current?.compositingOperation = .multiply
        NSColor.systemPink.setFill()
        NSBezierPath(ovalIn: NSRect(x: 20 + disc * 0.6, y: 12 + disc * 0.5, width: disc, height: disc)).fill()
        NSGraphicsContext.restoreGraphicsState()
        guard bounds.width > 180 else { return }
        for (index, colour) in [NSColor.systemBlue, .systemPink, .systemGreen].enumerated() {
            rounded(NSRect(x: bounds.width - 62, y: 16 + CGFloat(index) * 24, width: 18, height: 18), 5, colour)
        }
    }

    static func picture(_ rect: NSRect, dark: Bool) {
        let start = dark ? NSColor(srgbRed: 0.12, green: 0.17, blue: 0.29, alpha: 1) : .systemBlue
        let path = NSBezierPath(roundedRect: rect, xRadius: 4, yRadius: 4)
        NSGradient(starting: start, ending: .systemPink)?.draw(in: path, angle: -40)
    }

    static func browser(_ bounds: NSRect, tab: String, address: String, video: Bool) {
        window(bounds, NSColor(white: 0.96, alpha: 1))
        NSGraphicsContext.saveGraphicsState()
        NSBezierPath(roundedRect: bounds, xRadius: 5, yRadius: 5).addClip()
        NSColor(white: 0.9, alpha: 1).setFill()
        NSRect(x: 0, y: 0, width: bounds.width, height: 20).fill()
        rounded(NSRect(x: 8, y: 4, width: 92, height: 18), 4, NSColor(white: 0.96, alpha: 1))
        let ink = NSColor(white: 0.11, alpha: 1)
        text(tab, .systemFont(ofSize: 8), ink, at: NSPoint(x: 14, y: 7))
        text(
            address, .monospacedSystemFont(ofSize: 8, weight: .regular), NSColor(white: 0.37, alpha: 1),
            at: NSPoint(x: 8, y: 23))
        NSColor.white.setFill()
        NSRect(x: 0, y: 38, width: bounds.width, height: bounds.height - 38).fill()
        if video {
            picture(NSRect(x: 12, y: 47, width: 140, height: 64), dark: true)
            rounded(NSRect(x: 166, y: 52, width: 110, height: 6), 3, line)
            rounded(NSRect(x: 166, y: 66, width: 80, height: 6), 3, line)
        } else {
            for (y, share) in [(50.0, 0.4), (64, 0.7), (78, 0.6)] {
                rounded(NSRect(x: 10, y: y, width: bounds.width * share, height: 5), 2.5, line)
            }
        }
        NSGraphicsContext.restoreGraphicsState()
    }
}
