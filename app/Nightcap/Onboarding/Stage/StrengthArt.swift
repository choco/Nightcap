import AppKit

/// The strength step's two screens, daytime in true colour and the evening warmed as the slider
/// says.
final class StrengthArt: NSView {
    var strength: UInt8 {
        didSet {
            evening.needsDisplay = true
            caption.needsDisplay = true
        }
    }
    private let evening: Drawn
    private let caption: Drawn

    init(strength: UInt8) {
        self.strength = strength
        let screen = NSSize(width: 236, height: 140)
        var warmth: () -> UInt8 = { strength }
        evening = Drawn(screen) { bounds in
            Paint.warm(Paint.alpha(for: warmth())).setFill()
            NSBezierPath(roundedRect: bounds, xRadius: 8, yRadius: 8).fill()
        }
        caption = Drawn(NSSize(width: screen.width, height: 16)) { bounds in
            Paint.text(
                Wording.system.evening(warmth()), .systemFont(ofSize: 11), .nightcapSecondary,
                centredAt: NSPoint(x: bounds.midX, y: 8))
        }
        super.init(frame: NSRect(x: 0, y: 0, width: screen.width * 2 + 78, height: 162))
        warmth = { [weak self] in self?.strength ?? strength }
        let day = MiniScreen(screen, app: "Mail", otherApp: nil, forcesOff: false, daytime: true)
        day.add(Drawn(NSSize(width: 196, height: 102)) { Paint.page($0) }, at: NSPoint(x: 20, y: 26))
        let night = MiniScreen(screen, app: "Mail", otherApp: nil, forcesOff: false)
        night.add(Drawn(NSSize(width: 196, height: 102)) { Paint.page($0) }, at: NSPoint(x: 20, y: 26))
        night.add(evening, at: .zero)
        day.frame.origin = .zero
        night.frame.origin = NSPoint(x: screen.width + 78, y: 0)
        addSubview(day)
        addSubview(night)
        let arrow = Drawn(NSSize(width: 22, height: 22)) { bounds in
            Paint.glyph("arrow.right", 16, .nightcapSecondary, centredAt: NSPoint(x: bounds.midX, y: bounds.midY))
        }
        arrow.frame.origin = NSPoint(x: screen.width + 28, y: 59)
        addSubview(arrow)
        let dayCaption = Drawn(NSSize(width: screen.width, height: 16)) { bounds in
            Paint.text(
                Wording.system.daytime, .systemFont(ofSize: 11), .nightcapSecondary,
                centredAt: NSPoint(x: bounds.midX, y: 8))
        }
        dayCaption.frame.origin = NSPoint(x: 0, y: 146)
        caption.frame.origin = NSPoint(x: night.frame.minX, y: 146)
        addSubview(dayCaption)
        addSubview(caption)
    }

    required init?(coder: NSCoder) {
        fatalError("not built from a nib")
    }

    override var isFlipped: Bool { true }
    override var intrinsicContentSize: NSSize { frame.size }
}
