import AppKit

/// Text as the mockups set it, with a 1.4 line height.
extension NSTextField {
    static func secondary(_ text: String, width: CGFloat) -> NSTextField {
        label(text, size: 12, weight: .regular, color: .nightcapSecondary, width: width)
    }

    /// Wrapping text with a line height of 1.4 times the font size.
    static func label(
        _ text: String, size: CGFloat, weight: NSFont.Weight, color: NSColor, width: CGFloat
    ) -> NSTextField {
        label(text, font: .systemFont(ofSize: size, weight: weight), color: color, width: width)
    }

    static func label(_ text: String, font: NSFont, color: NSColor, width: CGFloat) -> NSTextField {
        let lineHeight = (font.pointSize * 1.4).rounded()
        let paragraph = NSMutableParagraphStyle()
        paragraph.minimumLineHeight = lineHeight
        paragraph.maximumLineHeight = lineHeight
        let label = NSTextField(wrappingLabelWithString: "")
        // A selectable label redraws a click through the field editor, in the field's own font
        // and colour without the line height, and keeps that look afterwards.
        label.isSelectable = false
        label.font = font
        label.textColor = color
        label.attributedStringValue = NSAttributedString(
            string: text,
            attributes: [
                .font: font,
                .foregroundColor: color,
                .paragraphStyle: paragraph,
                // Centers the glyphs in the taller line, as CSS does.
                .baselineOffset: (lineHeight - font.ascender + font.descender) / 4,
            ])
        label.preferredMaxLayoutWidth = width
        return label
    }
}
