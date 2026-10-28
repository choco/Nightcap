import AppKit

/// A rounded group of rows, each as wide as the group, divided by hairlines. It's sized by its
/// rows, which it clips to its corners.
final class Group: NSView {
    init(rows: [NSView]) {
        var views: [NSView] = []
        for (index, row) in rows.enumerated() {
            if index > 0 { views.append(Hairline()) }
            views.append(row)
        }
        let stack = NSStackView(.vertical, views, alignment: .leading, spacing: 0)
        for view in views {
            view.widthAnchor.constraint(equalTo: stack.widthAnchor).isActive = true
        }
        super.init(frame: .zero)
        addPinned(stack)
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
