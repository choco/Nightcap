import AppKit

/// `content` inside the given margins, held by constraints.
final class Padded: NSView {
    init(_ content: NSView, top: CGFloat, left: CGFloat = 14, bottom: CGFloat, right: CGFloat = 14) {
        super.init(frame: .zero)
        content.translatesAutoresizingMaskIntoConstraints = false
        addSubview(content)
        NSLayoutConstraint.activate([
            content.leadingAnchor.constraint(equalTo: leadingAnchor, constant: left),
            content.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -right),
            content.topAnchor.constraint(equalTo: topAnchor, constant: top),
            content.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -bottom),
        ])
    }

    required init?(coder: NSCoder) {
        fatalError("not built from a nib")
    }
}
