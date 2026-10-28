import AppKit

/// The window's own colour, which the mockups set apart from the groups.
final class Backdrop: NSView {
    override var wantsUpdateLayer: Bool { true }

    override func updateLayer() {
        effectiveAppearance.performAsCurrentDrawingAppearance {
            layer?.backgroundColor = NSColor.nightcapWindow.cgColor
        }
    }
}
