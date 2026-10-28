import AppKit

/// A suggestion in onboarding's grid: a checkbox, the icon and two lines. A click anywhere on it
/// ticks it, unless its rule exists already, which the second line then says where it comes from.
final class GridCell: NSView {
    private let checkbox: NSButton

    init(_ item: GridItem, ruled: String?, ticked: Bool, toggle: @escaping () -> Void) {
        checkbox = NSButton(checkboxWithTitle: "", target: nil, action: nil)
        super.init(frame: .zero)
        checkbox.state = ticked ? .on : .off
        checkbox.isEnabled = ruled == nil
        checkbox.setAccessibilityLabel(item.name)
        checkbox.onAction(toggle)
        self.toggle = ruled == nil ? toggle : nil
        let icon: NSView
        switch item.icon {
        case .app(let image):
            let view = NSImageView(image: image)
            view.imageScaling = .scaleProportionallyUpOrDown
            view.widthAnchor.constraint(equalToConstant: 24).isActive = true
            view.heightAnchor.constraint(equalToConstant: 24).isActive = true
            icon = view
        case .site(let image):
            icon = image.map { Tile(image: $0) } ?? Tile(symbol: "globe")
        }
        let name = NSTextField(labelWithString: item.name)
        name.font = .systemFont(ofSize: 13)
        name.lineBreakMode = .byTruncatingTail
        let detail = NSTextField(labelWithString: ruled ?? item.detail)
        detail.font = .systemFont(ofSize: 11)
        detail.textColor = .nightcapSecondary
        detail.lineBreakMode = .byTruncatingTail
        for label in [name, detail] {
            label.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        }
        let words = NSStackView(.vertical, [name, detail], alignment: .leading, spacing: 0)
        let line = NSStackView(.horizontal, [checkbox, icon, words])
        line.translatesAutoresizingMaskIntoConstraints = false
        addSubview(line)
        NSLayoutConstraint.activate([
            line.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 10),
            line.trailingAnchor.constraint(lessThanOrEqualTo: trailingAnchor, constant: -10),
            line.topAnchor.constraint(equalTo: topAnchor, constant: 6),
            line.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -6),
        ])
    }

    required init?(coder: NSCoder) {
        fatalError("not built from a nib")
    }

    private var toggle: (() -> Void)?

    override func mouseDown(with event: NSEvent) {
        toggle?()
    }
}
