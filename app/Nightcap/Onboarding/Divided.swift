import AppKit

/// A view with a hairline down its left edge, for cells side by side in a group.
final class Divided: NSView {
    private let left: Bool

    init(_ content: NSView, left: Bool) {
        self.left = left
        super.init(frame: .zero)
        addPinned(content)
    }

    required init?(coder: NSCoder) {
        fatalError("not built from a nib")
    }

    override func draw(_ dirtyRect: NSRect) {
        guard left else { return }
        NSColor.nightcapSeparator.setFill()
        NSRect(x: 0, y: 0, width: 0.5, height: bounds.height).fill()
    }
}
