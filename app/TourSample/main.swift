import AppKit

/// The tour's sample (ADR-0022): an app of its own with a window of colours that Night Shift
/// visibly warms, which the user gives a rule from Nightcap's menu. Nightcap opens it for the
/// tour's rule step and quits it after.
final class Sample: NSObject, NSApplicationDelegate {
    private var window: NSWindow?

    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.applicationIconImage = Self.icon
        let size = NSSize(width: 440, height: 300)
        let window = NSWindow(
            contentRect: NSRect(origin: .zero, size: size), styleMask: [.titled, .miniaturizable],
            backing: .buffered, defer: false)
        window.title = "Tour Sample"
        window.contentView = Poster()
        if let screen = NSScreen.main?.visibleFrame {
            window.setFrameOrigin(NSPoint(x: screen.maxX - size.width - 120, y: screen.minY + 120))
        }
        window.makeKeyAndOrderFront(nil)
        self.window = window
        NSApp.activate()
    }

    /// A clicked Dock icon brings the window back.
    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
        window?.deminiaturize(nil)
        window?.makeKeyAndOrderFront(nil)
        return false
    }

    /// The poster on a rounded square, for the Dock.
    private static let icon = NSImage(size: NSSize(width: 512, height: 512), flipped: true) { rect in
        let square = NSBezierPath(roundedRect: rect.insetBy(dx: 50, dy: 50), xRadius: 90, yRadius: 90)
        square.addClip()
        Poster.paint(in: rect.insetBy(dx: 50, dy: 50), scale: 412 / 440)
        return true
    }
}

/// A sky, two overlapping discs and a column of swatches: colours a warm tint visibly shifts.
private final class Poster: NSView {
    override var isFlipped: Bool { true }

    override func draw(_ dirtyRect: NSRect) {
        Self.paint(in: bounds, scale: 1)
    }

    static func paint(in rect: NSRect, scale: CGFloat) {
        let hex = { (rgb: UInt32) in
            NSColor(
                srgbRed: CGFloat(rgb >> 16 & 0xff) / 255, green: CGFloat(rgb >> 8 & 0xff) / 255,
                blue: CGFloat(rgb & 0xff) / 255, alpha: 1)
        }
        let at = { (x: CGFloat, y: CGFloat, width: CGFloat, height: CGFloat) in
            NSRect(x: rect.minX + x * scale, y: rect.minY + y * scale, width: width * scale, height: height * scale)
        }
        hex(0xf7f3e8).setFill()
        rect.fill()
        NSGradient(starting: hex(0x5aa9e6), ending: hex(0xbfe3f5))!
            .draw(in: at(0, 0, rect.width / scale, 130), angle: 90)
        hex(0x2f6fe4).setFill()
        NSBezierPath(ovalIn: at(40, 50, 120, 120)).fill()
        NSGraphicsContext.current?.saveGraphicsState()
        NSGraphicsContext.current?.compositingOperation = .multiply
        hex(0xe84f8a).setFill()
        NSBezierPath(ovalIn: at(110, 110, 120, 120)).fill()
        NSGraphicsContext.current?.restoreGraphicsState()
        for (index, rgb) in [UInt32(0xffffff), 0x2f6fe4, 0xe84f8a, 0x22b33a].enumerated() {
            let swatch = NSBezierPath(
                roundedRect: at(rect.width / scale - 64, 30 + CGFloat(index) * 42, 34, 34),
                xRadius: 7 * scale, yRadius: 7 * scale)
            hex(rgb).setFill()
            swatch.fill()
            NSColor.black.withAlphaComponent(0.15).setStroke()
            swatch.lineWidth = 0.5
            swatch.stroke()
        }
    }
}

let app = NSApplication.shared
let delegate = Sample()
app.delegate = delegate
app.setActivationPolicy(.regular)
let appMenu = NSMenu()
appMenu.addItem(NSMenuItem(title: "Quit Tour Sample", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q"))
let appItem = NSMenuItem()
appItem.submenu = appMenu
app.mainMenu = NSMenu()
app.mainMenu?.addItem(appItem)
withExtendedLifetime(delegate) { app.run() }
