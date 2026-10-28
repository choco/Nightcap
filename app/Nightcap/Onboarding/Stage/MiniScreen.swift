import AppKit

/// A small Mac screen: wallpaper, a menu bar naming the app in front, and windows on top.
final class MiniScreen: NSView {
    private let size: NSSize

    /// With `otherApp`, the menu bar names it in the middle of each loop, as when that app comes
    /// to the front; `forcesOff` gives the moon its "z" then. A `daytime` screen shows the moon
    /// off and an afternoon time.
    init(_ size: NSSize, app: String, otherApp: String?, forcesOff: Bool, daytime: Bool = false) {
        self.size = size
        super.init(frame: NSRect(origin: .zero, size: size))
        wantsLayer = true
        layer?.cornerRadius = 8
        layer?.masksToBounds = true
        let backdrop = Drawn(size) { bounds in
            NSGradient(
                starting: NSColor(srgbRed: 0.62, green: 0.72, blue: 0.88, alpha: 1),
                ending: NSColor(srgbRed: 0.78, green: 0.71, blue: 0.86, alpha: 1))?.draw(in: bounds, angle: -20)
        }
        addSubview(backdrop)
        let bar = Drawn(NSSize(width: size.width, height: 15)) { bounds in
            NSColor(white: 0.97, alpha: 0.88).setFill()
            bounds.fill()
        }
        addSubview(bar)
        let ink = NSColor(white: 0.11, alpha: 1)
        let name = { (app: String) in
            Drawn(NSSize(width: 60, height: 15)) { _ in
                Paint.text(app, .systemFont(ofSize: 8, weight: .bold), ink, at: NSPoint(x: 0, y: 3))
            }
        }
        let first = name(app)
        first.frame.origin = NSPoint(x: 8, y: 0)
        addSubview(first)
        if let otherApp {
            let second = name(otherApp)
            second.frame.origin = first.frame.origin
            addSubview(second)
            Loop.middle(second)
            Loop.ends(first)
        }
        let words = Wording.system
        let clock = daytime ? words.afternoonClock : words.eveningClock
        let clockFont = NSFont.systemFont(ofSize: 8)
        // The moon, then the clock, as wide as the user's clock makes it, with room for 21:42.
        let width = max(40, 13 + Paint.width(clock, clockFont).rounded(.up) + 4)
        let status = { (icon: Menu.Moon) in
            Drawn(NSSize(width: width, height: 15)) { _ in
                Paint.moon(icon, ink, in: NSRect(x: 0, y: 3, width: 9, height: 9))
                Paint.text(clock, clockFont, ink, at: NSPoint(x: 13, y: 3))
            }
        }
        let on = status(daytime ? .off : .on)
        on.frame.origin = NSPoint(x: size.width - 4 - width, y: 0)
        addSubview(on)
        if forcesOff {
            let forced = status(.forcedOff)
            forced.frame.origin = on.frame.origin
            addSubview(forced)
            Loop.middle(forced)
            Loop.ends(on)
        }
    }

    required init?(coder: NSCoder) {
        fatalError("not built from a nib")
    }

    override var isFlipped: Bool { true }
    override var intrinsicContentSize: NSSize { size }

    func add(_ view: NSView, at origin: NSPoint) {
        view.frame.origin = origin
        addSubview(view)
    }

    /// Night Shift's tint over the whole screen, lifting while the loop's other app is in front.
    func warm(_ strength: UInt8, loops: Bool = true) {
        let tint = Drawn(size) { bounds in
            Paint.warm(Paint.alpha(for: strength)).setFill()
            bounds.fill()
        }
        add(tint, at: .zero)
        if loops { Loop.ends(tint, lifting: true) }
    }
}
