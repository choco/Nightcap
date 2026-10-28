import AppKit

/// A 26-point rounded tile for what has no app icon of its own: a site, by its own icon or the
/// globe, or an app that isn't installed.
final class Tile: NSView {
    convenience init(symbol: String) {
        self.init(
            image: NSImage(systemSymbolName: symbol, accessibilityDescription: nil)!
                .withSymbolConfiguration(NSImage.SymbolConfiguration(pointSize: 14, weight: .regular))!,
            tinted: true)
    }

    /// A symbol in the secondary colour, at its own size, or a picture, such as a site's icon,
    /// at 16 points.
    init(image: NSImage, tinted: Bool = false) {
        super.init(frame: .zero)
        let view = NSImageView(image: image)
        if tinted {
            view.contentTintColor = .nightcapSecondary
        } else {
            view.imageScaling = .scaleProportionallyUpOrDown
            view.widthAnchor.constraint(equalToConstant: 16).isActive = true
            view.heightAnchor.constraint(equalToConstant: 16).isActive = true
        }
        view.translatesAutoresizingMaskIntoConstraints = false
        addSubview(view)
        NSLayoutConstraint.activate([
            widthAnchor.constraint(equalToConstant: 26),
            heightAnchor.constraint(equalToConstant: 26),
            view.centerXAnchor.constraint(equalTo: centerXAnchor),
            view.centerYAnchor.constraint(equalTo: centerYAnchor),
        ])
    }

    required init?(coder: NSCoder) {
        fatalError("not built from a nib")
    }

    override var wantsUpdateLayer: Bool { true }

    override func updateLayer() {
        layer?.cornerRadius = 7
        effectiveAppearance.performAsCurrentDrawingAppearance {
            layer?.backgroundColor = NSColor.nightcapTile.cgColor
        }
    }
}
