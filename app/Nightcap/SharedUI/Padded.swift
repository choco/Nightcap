import AppKit

/// `content` inside the given margins, held by constraints.
final class Padded: NSView {
    init(_ content: NSView, top: CGFloat, left: CGFloat = 14, bottom: CGFloat, right: CGFloat = 14) {
        super.init(frame: .zero)
        addPinned(content, insets: NSEdgeInsets(top: top, left: left, bottom: bottom, right: right))
    }

    required init?(coder: NSCoder) {
        fatalError("not built from a nib")
    }
}
