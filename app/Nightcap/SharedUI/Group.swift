import AppKit

/// A rounded group of rows, sized by its content, which it clips to its corners.
final class Group: NSView {
    init(_ content: NSView) {
        super.init(frame: .zero)
        content.translatesAutoresizingMaskIntoConstraints = false
        addSubview(content)
        NSLayoutConstraint.activate([
            content.leadingAnchor.constraint(equalTo: leadingAnchor),
            content.trailingAnchor.constraint(equalTo: trailingAnchor),
            content.topAnchor.constraint(equalTo: topAnchor),
            content.bottomAnchor.constraint(equalTo: bottomAnchor),
        ])
    }

    required init?(coder: NSCoder) {
        fatalError("not built from a nib")
    }

    override var wantsUpdateLayer: Bool { true }

    override func updateLayer() {
        guard let layer else { return }
        layer.cornerRadius = 12
        layer.borderWidth = 0.5
        layer.masksToBounds = true
        effectiveAppearance.performAsCurrentDrawingAppearance {
            layer.borderColor = NSColor.nightcapSeparator.cgColor
            layer.backgroundColor = NSColor.nightcapGroup.cgColor
        }
    }
}
