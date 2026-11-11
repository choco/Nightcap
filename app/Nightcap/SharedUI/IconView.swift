import AppKit

extension NSImageView {
    /// `image` scaled into a square `side` points wide, as an app's icon is shown.
    convenience init(icon image: NSImage, side: CGFloat) {
        self.init(image: image)
        imageScaling = .scaleProportionallyUpOrDown
        widthAnchor.constraint(equalToConstant: side).isActive = true
        heightAnchor.constraint(equalToConstant: side).isActive = true
    }
}
