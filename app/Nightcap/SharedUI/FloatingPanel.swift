import AppKit

/// A small window of Nightcap's own, as the tour's and Custom…'s: floating on every Space, never
/// taking the focus from the app in front, with its glass active throughout. Its content goes in
/// `glass`.
final class FloatingPanel: NSPanel {
    let glass = NSVisualEffectView()

    init() {
        super.init(
            contentRect: NSRect(x: 0, y: 0, width: 330, height: 100),
            styleMask: [.borderless, .nonactivatingPanel], backing: .buffered, defer: false)
        isFloatingPanel = true
        level = .floating
        collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]
        hidesOnDeactivate = false
        isMovableByWindowBackground = true
        isReleasedWhenClosed = false
        isOpaque = false
        backgroundColor = .clear
        hasShadow = true
        glass.material = .popover
        glass.blendingMode = .behindWindow
        glass.state = .active
        glass.maskImage = Self.rounded(radius: 14)
        contentView = glass
    }

    /// Borderless, it would otherwise never be key, and its buttons couldn't take Return.
    override var canBecomeKey: Bool { true }

    /// A stretchable rounded rectangle, which clips the glass and shapes the shadow.
    private static func rounded(radius: CGFloat) -> NSImage {
        let edge = radius * 2 + 1
        let image = NSImage(size: NSSize(width: edge, height: edge), flipped: false) { rect in
            NSColor.black.setFill()
            NSBezierPath(roundedRect: rect, xRadius: radius, yRadius: radius).fill()
            return true
        }
        image.capInsets = NSEdgeInsets(top: radius, left: radius, bottom: radius, right: radius)
        image.resizingMode = .stretch
        return image
    }
}
