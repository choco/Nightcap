import AppKit

/// A capsule button that runs `run` when clicked. The regular ones have a hairline border and a
/// soft shadow, which AppKit's own push button loses on a white group; a primary one is filled
/// with the accent. 13-point text in 25 points, or 12 in 21 when small, borders included.
final class CapsuleButton: NSButton {
    enum Style {
        case regular, small, primary
    }

    private let style: Style
    private var small: Bool { style == .small }
    private var padding: CGFloat { small ? 10 : 13 }
    /// Acts on a click into a window that isn't key, as in the tour's panel, which never takes
    /// the focus.
    var actsOnFirstClick = false

    init(_ title: String, _ style: Style, _ run: @escaping () -> Void) {
        self.style = style
        super.init(frame: .zero)
        self.title = title
        font = .systemFont(ofSize: small ? 12 : 13)
        if style == .primary {
            attributedTitle = NSAttributedString(
                string: title, attributes: [.font: font!, .foregroundColor: NSColor.white])
        }
        isBordered = false
        bezelStyle = .push
        wantsLayer = true
        setButtonType(.momentaryPushIn)
        onAction(run)
    }

    required init?(coder: NSCoder) {
        fatalError("not built from a nib")
    }

    override func acceptsFirstMouse(for event: NSEvent?) -> Bool {
        actsOnFirstClick || super.acceptsFirstMouse(for: event)
    }

    override var intrinsicContentSize: NSSize {
        let width = attributedTitle.size().width
        return NSSize(width: (width + padding * 2).rounded(.up), height: small ? 21 : 25)
    }

    override var isHighlighted: Bool {
        didSet { needsDisplay = true }
    }

    override var isEnabled: Bool {
        didSet { needsDisplay = true }
    }

    override var wantsUpdateLayer: Bool { true }

    override func updateLayer() {
        guard let layer else { return }
        layer.cornerRadius = bounds.height / 2
        // Dimmed whole, as the mockups draw a disabled button.
        layer.opacity = isEnabled ? 1 : 0.55
        let primary = style == .primary
        layer.borderWidth = primary ? 0 : 0.5
        layer.shadowOffset = NSSize(width: 0, height: -0.5)
        layer.shadowRadius = 0.75
        layer.shadowOpacity = primary ? 0 : 1
        effectiveAppearance.performAsCurrentDrawingAppearance {
            let fill: NSColor =
                switch (primary, isHighlighted) {
                case (true, false): .controlAccentColor
                case (true, true): NSColor.controlAccentColor.withSystemEffect(.pressed)
                case (false, false): .nightcapControl
                case (false, true): .nightcapControlPressed
                }
            layer.backgroundColor = fill.cgColor
            layer.borderColor = NSColor.nightcapControlBorder.cgColor
            layer.shadowColor = NSColor.black.withAlphaComponent(0.14).cgColor
        }
    }
}
