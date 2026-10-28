import AppKit

extension NSButton {
    /// Accent-coloured text that runs `run` when clicked, like onboarding's "Choose Another App…".
    static func link(_ title: String, _ run: @escaping () -> Void) -> NSButton {
        let button = NSButton(title: title, target: nil, action: nil)
        button.isBordered = false
        button.attributedTitle = NSAttributedString(
            string: title, attributes: [.font: NSFont.systemFont(ofSize: 12), .foregroundColor: NSColor.linkColor])
        button.onAction(run)
        return button
    }
}
