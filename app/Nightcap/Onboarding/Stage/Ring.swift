import AppKit

/// A slow pulsing ring around a control the scene points at.
final class Ring: NSView {
    init(around rect: NSRect, radius: CGFloat) {
        super.init(frame: rect)
        wantsLayer = true
        layer?.cornerRadius = radius
        layer?.borderWidth = 2
        layer?.borderColor = NSColor.controlAccentColor.withAlphaComponent(0.55).cgColor
        Loop.pulse(self)
    }

    required init?(coder: NSCoder) {
        fatalError("not built from a nib")
    }
}
