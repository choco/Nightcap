import AppKit
import NightcapCore

/// The menu's art, which the status item and its menu draw and onboarding's stage copies: the
/// moons, the problem's dot and triangle, the strength's suns, and how a subtitle breaks.
enum MenuArt {
    /// The mockup's moons, template images from the asset catalog.
    static func moon(for icon: MenuIcon) -> NSImage? {
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

    /// The mockup's orange dot over the moon while a problem exists.
    static let dotSize: CGFloat = 5.5
    /// From the icon's centre to its top right corner, as in the mockup: clear of the forced
    /// moon's "z", and inside the status item's square.
    static let dotOffset = CGPoint(x: 9.2, y: 7.5)

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
            NSBezierPath(
                ovalIn: NSRect(
                    x: (10 - 1.1) * unit, y: (14.6 - 1.1) * unit, width: 2.2 * unit, height: 2.2 * unit)
            ).fill()
            return true
        }
        image.accessibilityDescription = "Needs attention"
        return image
    }

    /// The mockup's suns at the slider's ends: a small one with four rays for less warmth, a
    /// large one with eight for more. Templates, so they take the menu's text colour.
    static func sun(large: Bool) -> NSImage {
        let side: CGFloat = large ? 16 : 11
        let image = NSImage(size: NSSize(width: side, height: side), flipped: true) { _ in
            let unit = side / 24
            let point = { (x: CGFloat, y: CGFloat) in NSPoint(x: x * unit, y: y * unit) }
            let rays: [(CGFloat, CGFloat, CGFloat, CGFloat)] =
                large
                ? [
                    (12, 2.5, 12, 5), (12, 19, 12, 21.5), (2.5, 12, 5, 12), (19, 12, 21.5, 12),
                    (5.3, 5.3, 7.1, 7.1), (16.9, 16.9, 18.7, 18.7), (5.3, 18.7, 7.1, 16.9), (16.9, 7.1, 18.7, 5.3),
                ]
                : [(12, 3, 12, 5), (12, 19, 12, 21), (3, 12, 5, 12), (19, 12, 21, 12)]
            let path = NSBezierPath(
                ovalIn: NSRect(
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

    /// The widest a subtitle's line gets.
    private static let subtitleWidth: CGFloat = 160

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
}
