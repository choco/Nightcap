import AppKit

extension NSStackView {
    /// `views` in a row or a column, with AppKit's own spacing, distribution and insets for those
    /// left out. Left out, `alignment` is AppKit's for the orientation: centred across it.
    convenience init(
        _ orientation: NSUserInterfaceLayoutOrientation,
        _ views: [NSView],
        alignment: NSLayoutConstraint.Attribute? = nil,
        distribution: Distribution = .gravityAreas,
        spacing: CGFloat = 8,
        edgeInsets: NSEdgeInsets = NSEdgeInsets()
    ) {
        self.init(views: views)
        // Setting the orientation resets the alignment, so it goes first.
        self.orientation = orientation
        if let alignment { self.alignment = alignment }
        self.distribution = distribution
        self.spacing = spacing
        self.edgeInsets = edgeInsets
    }
}

extension NSView {
    /// Adds `content`, held to the view's four edges, `insets` in from them.
    func addPinned(_ content: NSView, insets: NSEdgeInsets = NSEdgeInsets()) {
        content.translatesAutoresizingMaskIntoConstraints = false
        addSubview(content)
        NSLayoutConstraint.activate([
            content.leadingAnchor.constraint(equalTo: leadingAnchor, constant: insets.left),
            content.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -insets.right),
            content.topAnchor.constraint(equalTo: topAnchor, constant: insets.top),
            content.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -insets.bottom),
        ])
    }
}
