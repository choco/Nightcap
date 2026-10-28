import AppKit

/// A rule's label in a capsule, tinted orange while the rule is snoozed.
final class Chip: NSView {
    private let tinted: Bool

    init(_ text: String, tinted: Bool) {
        self.tinted = tinted
        super.init(frame: .zero)
        let label = NSTextField(labelWithString: text)
        label.font = .systemFont(ofSize: 11)
        label.textColor = tinted ? .labelColor : .nightcapSecondary
        addPinned(label, insets: NSEdgeInsets(top: 1, left: 7, bottom: 1, right: 7))
        setContentHuggingPriority(.required, for: .horizontal)
        setContentCompressionResistancePriority(.required, for: .horizontal)
    }

    required init?(coder: NSCoder) {
        fatalError("not built from a nib")
    }

    override var wantsUpdateLayer: Bool { true }

    override func updateLayer() {
        guard let layer else { return }
        layer.cornerRadius = bounds.height / 2
        effectiveAppearance.performAsCurrentDrawingAppearance {
            layer.backgroundColor =
                (tinted ? NSColor.nightcapOrange.withAlphaComponent(0.18) : NSColor.nightcapTile).cgColor
        }
    }
}
