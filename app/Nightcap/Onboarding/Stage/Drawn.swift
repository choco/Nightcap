import AppKit

/// A flipped view drawn by a closure, at a fixed size.
final class Drawn: NSView {
    private let paint: (NSRect) -> Void

    init(_ size: NSSize, _ paint: @escaping (NSRect) -> Void) {
        self.paint = paint
        super.init(frame: NSRect(origin: .zero, size: size))
    }

    required init?(coder: NSCoder) {
        fatalError("not built from a nib")
    }

    override var isFlipped: Bool { true }
    override var intrinsicContentSize: NSSize { frame.size }

    override func draw(_ dirtyRect: NSRect) {
        paint(bounds)
    }
}
