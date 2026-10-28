// `NSItemBadge.indicator` is imported as shared mutable state, though it only makes a badge.
@preconcurrency import AppKit
import NightcapCore

/// The look Settings and onboarding share, from the mockups: their palette, text with a 1.4 line
/// height, rounded groups, capsule buttons and the status marks.
enum Look {
    /// What a status mark says. Checks use the first four; onboarding's steps use all six.
    enum Mark {
        case ok, off, drifted, broken, todo, current

        init(_ state: CheckState) {
            self =
                switch state {
                case .ok: .ok
                case .off: .off
                case .drifted: .drifted
                case .broken: .broken
                }
        }
    }

    enum ButtonStyle {
        case regular, small, primary
    }

    /// An 18-point mark: a white sign on a filled circle or triangle, a grey dash, a hollow
    /// circle for a step not reached yet, or the current step's ring and pulsing dot.
    static func mark(_ mark: Mark) -> NSView {
        let symbol: (name: String, colors: [NSColor], description: String)
        let words = Wording.system
        switch mark {
        case .ok: symbol = ("checkmark.circle.fill", [.white, .nightcapGreen], words.mark(.ok))
        case .off: symbol = ("minus.circle", [.nightcapGrey], words.mark(.off))
        case .drifted: symbol = ("exclamationmark.triangle.fill", [.white, .nightcapOrange], words.mark(.drifted))
        case .broken: symbol = ("xmark.circle.fill", [.white, .nightcapRed], words.mark(.broken))
        case .todo: return StepRing(current: false)
        case .current: return StepRing(current: true)
        }
        let configuration = NSImage.SymbolConfiguration(pointSize: 15, weight: .semibold)
            .applying(NSImage.SymbolConfiguration(paletteColors: symbol.colors))
        let image = NSImageView(
            image: NSImage(systemSymbolName: symbol.name, accessibilityDescription: symbol.description)!
                .withSymbolConfiguration(configuration)!)
        image.widthAnchor.constraint(equalToConstant: 18).isActive = true
        image.heightAnchor.constraint(equalToConstant: 18).isActive = true
        return image
    }

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

    /// `content` inside the given margins, held by constraints.
    static func padded(_ content: NSView, top: CGFloat, left: CGFloat = 14, bottom: CGFloat, right: CGFloat = 14)
        -> NSView
    {
        let box = NSView()
        content.translatesAutoresizingMaskIntoConstraints = false
        box.addSubview(content)
        NSLayoutConstraint.activate([
            content.leadingAnchor.constraint(equalTo: box.leadingAnchor, constant: left),
            content.trailingAnchor.constraint(equalTo: box.trailingAnchor, constant: -right),
            content.topAnchor.constraint(equalTo: box.topAnchor, constant: top),
            content.bottomAnchor.constraint(equalTo: box.bottomAnchor, constant: -bottom),
        ])
        return box
    }

    static func button(_ title: String, _ style: ButtonStyle, _ run: @escaping () -> Void) -> NSButton {
        let button = CapsuleButton(title: title, style: style)
        attach(button, run)
        return button
    }

    /// Accent-coloured text that acts, like onboarding's "Change".
    static func link(_ title: String, _ run: @escaping () -> Void) -> NSButton {
        let button = NSButton(title: title, target: nil, action: nil)
        button.isBordered = false
        button.attributedTitle = NSAttributedString(
            string: title, attributes: [.font: NSFont.systemFont(ofSize: 12), .foregroundColor: NSColor.linkColor])
        attach(button, run)
        return button
    }

    static func attach(_ control: NSControl, _ run: @escaping () -> Void) {
        let action = Action(run)
        control.target = action
        control.action = #selector(Action.run)
        objc_setAssociatedObject(control, &Action.key, action, .OBJC_ASSOCIATION_RETAIN)
    }
}

/// A capsule button. The regular ones have a hairline border and a soft shadow, which AppKit's
/// own push button loses on a white group; a primary one is filled with the accent. 13-point
/// text in 25 points, or 12 in 21 when small, borders included.
final class CapsuleButton: NSButton {
    private let style: Look.ButtonStyle
    private var small: Bool { style == .small }
    private var padding: CGFloat { small ? 10 : 13 }
    /// Acts on a click into a window that isn't key, as in the tour's panel, which never takes
    /// the focus.
    var actsOnFirstClick = false

    init(title: String, style: Look.ButtonStyle) {
        self.style = style
        super.init(frame: .zero)
        self.title = title
        font = .systemFont(ofSize: small ? 12 : 13)
        if style == .primary {
            attributedTitle = NSAttributedString(
                string: title, attributes: [.font: font!, .foregroundColor: NSColor.white])
        }
        isBordered = false
        bezelStyle = .push
        wantsLayer = true
        setButtonType(.momentaryPushIn)
    }

    required init?(coder: NSCoder) {
        fatalError("not built from a nib")
    }

    override func acceptsFirstMouse(for event: NSEvent?) -> Bool {
        actsOnFirstClick || super.acceptsFirstMouse(for: event)
    }

    override var intrinsicContentSize: NSSize {
        let width = attributedTitle.size().width
        return NSSize(width: (width + padding * 2).rounded(.up), height: small ? 21 : 25)
    }

    override var isHighlighted: Bool {
        didSet { needsDisplay = true }
    }

    override var isEnabled: Bool {
        didSet { needsDisplay = true }
    }

    override var wantsUpdateLayer: Bool { true }

    override func updateLayer() {
        guard let layer else { return }
        layer.cornerRadius = bounds.height / 2
        // Dimmed whole, as the mockups draw a disabled button.
        layer.opacity = isEnabled ? 1 : 0.55
        let primary = style == .primary
        layer.borderWidth = primary ? 0 : 0.5
        layer.shadowOffset = NSSize(width: 0, height: -0.5)
        layer.shadowRadius = 0.75
        layer.shadowOpacity = primary ? 0 : 1
        effectiveAppearance.performAsCurrentDrawingAppearance {
            let fill: NSColor =
                switch (primary, isHighlighted) {
                case (true, false): .controlAccentColor
                case (true, true): NSColor.controlAccentColor.withSystemEffect(.pressed)
                case (false, false): .nightcapControl
                case (false, true): .nightcapControlPressed
                }
            layer.backgroundColor = fill.cgColor
            layer.borderColor = NSColor.nightcapControlBorder.cgColor
            layer.shadowColor = NSColor.black.withAlphaComponent(0.14).cgColor
        }
    }
}

/// A step's ring: hollow and grey until it's reached, the brand's gold with a pulsing dot while
/// current.
final class StepRing: NSView {
    private let current: Bool
    private let ring = CAShapeLayer()
    private let dot = CALayer()

    init(current: Bool) {
        self.current = current
        super.init(frame: NSRect(x: 0, y: 0, width: 18, height: 18))
        wantsLayer = true
        widthAnchor.constraint(equalToConstant: 18).isActive = true
        heightAnchor.constraint(equalToConstant: 18).isActive = true
        ring.path = CGPath(ellipseIn: CGRect(x: 1.6, y: 1.6, width: 14.8, height: 14.8), transform: nil)
        ring.fillColor = nil
        ring.lineWidth = 1.5
        layer?.addSublayer(ring)
        if current {
            dot.frame = CGRect(x: 4.9, y: 4.9, width: 8.2, height: 8.2)
            dot.cornerRadius = 4.1
            layer?.addSublayer(dot)
            if !NSWorkspace.shared.accessibilityDisplayShouldReduceMotion {
                let pulse = CABasicAnimation(keyPath: "opacity")
                pulse.fromValue = 1
                pulse.toValue = 0.45
                pulse.duration = 0.8
                pulse.autoreverses = true
                pulse.repeatCount = .infinity
                pulse.timingFunction = CAMediaTimingFunction(name: .easeInEaseOut)
                dot.add(pulse, forKey: "pulse")
            }
        }
        setAccessibilityElement(true)
        setAccessibilityRole(.image)
        setAccessibilityLabel(Wording.system.step(current: current))
    }

    required init?(coder: NSCoder) {
        fatalError("not built from a nib")
    }

    override var wantsUpdateLayer: Bool { true }

    override func updateLayer() {
        effectiveAppearance.performAsCurrentDrawingAppearance {
            ring.strokeColor = (current ? NSColor.nightcapMoon : NSColor.nightcapGrey.withAlphaComponent(0.7)).cgColor
            dot.backgroundColor = NSColor.nightcapMoon.cgColor
        }
    }
}

/// A rule's label in a capsule, tinted orange while the rule is snoozed.
final class Chip: NSView {
    private let tinted: Bool

    init(_ text: String, tinted: Bool) {
        self.tinted = tinted
        super.init(frame: .zero)
        let label = NSTextField(labelWithString: text)
        label.font = .systemFont(ofSize: 11)
        label.textColor = tinted ? .labelColor : .nightcapSecondary
        label.translatesAutoresizingMaskIntoConstraints = false
        addSubview(label)
        NSLayoutConstraint.activate([
            label.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 7),
            label.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -7),
            label.topAnchor.constraint(equalTo: topAnchor, constant: 1),
            label.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -1),
        ])
        setContentHuggingPriority(.required, for: .horizontal)
        setContentCompressionResistancePriority(.required, for: .horizontal)
    }

    required init?(coder: NSCoder) {
        fatalError("not built from a nib")
    }

    override var wantsUpdateLayer: Bool { true }

    override func updateLayer() {
        guard let layer else { return }
        layer.cornerRadius = bounds.height / 2
        effectiveAppearance.performAsCurrentDrawingAppearance {
            layer.backgroundColor =
                (tinted ? NSColor.nightcapOrange.withAlphaComponent(0.18) : NSColor.nightcapTile).cgColor
        }
    }
}

/// A 26-point rounded tile for what has no app icon of its own: a site, by its own icon or the
/// globe, or an app that isn't installed.
final class Tile: NSView {
    convenience init(symbol: String) {
        self.init(
            image: NSImage(systemSymbolName: symbol, accessibilityDescription: nil)!
                .withSymbolConfiguration(NSImage.SymbolConfiguration(pointSize: 14, weight: .regular))!,
            tinted: true)
    }

    /// A symbol in the secondary colour, at its own size, or a picture, such as a site's icon,
    /// at 16 points.
    init(image: NSImage, tinted: Bool = false) {
        super.init(frame: .zero)
        let view = NSImageView(image: image)
        if tinted {
            view.contentTintColor = .nightcapSecondary
        } else {
            view.imageScaling = .scaleProportionallyUpOrDown
            view.widthAnchor.constraint(equalToConstant: 16).isActive = true
            view.heightAnchor.constraint(equalToConstant: 16).isActive = true
        }
        view.translatesAutoresizingMaskIntoConstraints = false
        addSubview(view)
        NSLayoutConstraint.activate([
            widthAnchor.constraint(equalToConstant: 26),
            heightAnchor.constraint(equalToConstant: 26),
            view.centerXAnchor.constraint(equalTo: centerXAnchor),
            view.centerYAnchor.constraint(equalTo: centerYAnchor),
        ])
    }

    required init?(coder: NSCoder) {
        fatalError("not built from a nib")
    }

    override var wantsUpdateLayer: Bool { true }

    override func updateLayer() {
        layer?.cornerRadius = 7
        effectiveAppearance.performAsCurrentDrawingAppearance {
            layer?.backgroundColor = NSColor.nightcapTile.cgColor
        }
    }
}

/// The window's own colour, which the mockups set apart from the groups.
final class Backdrop: NSView {
    override var wantsUpdateLayer: Bool { true }

    override func updateLayer() {
        effectiveAppearance.performAsCurrentDrawingAppearance {
            layer?.backgroundColor = NSColor.nightcapWindow.cgColor
        }
    }
}

/// A rounded group of rows, sized by its content, which it clips to its corners.
final class Group: NSView {
    init(_ content: NSView) {
        super.init(frame: .zero)
        content.translatesAutoresizingMaskIntoConstraints = false
        addSubview(content)
        NSLayoutConstraint.activate([
            content.leadingAnchor.constraint(equalTo: leadingAnchor),
            content.trailingAnchor.constraint(equalTo: trailingAnchor),
            content.topAnchor.constraint(equalTo: topAnchor),
            content.bottomAnchor.constraint(equalTo: bottomAnchor),
        ])
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

/// A row's background: clear, or the accent's light tint on onboarding's current step.
final class Tint: NSView {
    private let tinted: Bool

    init(_ content: NSView, tinted: Bool) {
        self.tinted = tinted
        super.init(frame: .zero)
        content.translatesAutoresizingMaskIntoConstraints = false
        addSubview(content)
        NSLayoutConstraint.activate([
            content.leadingAnchor.constraint(equalTo: leadingAnchor),
            content.trailingAnchor.constraint(equalTo: trailingAnchor),
            content.topAnchor.constraint(equalTo: topAnchor),
            content.bottomAnchor.constraint(equalTo: bottomAnchor),
        ])
    }

    required init?(coder: NSCoder) {
        fatalError("not built from a nib")
    }

    override var wantsUpdateLayer: Bool { true }

    override func updateLayer() {
        effectiveAppearance.performAsCurrentDrawingAppearance {
            layer?.backgroundColor = tinted ? NSColor.nightcapMoon.withAlphaComponent(0.08).cgColor : nil
        }
    }
}

/// A 0.5-point line between rows.
final class Hairline: NSView {
    override var intrinsicContentSize: NSSize { NSSize(width: NSView.noIntrinsicMetric, height: 0.5) }
    override var wantsUpdateLayer: Bool { true }
    override func updateLayer() {
        effectiveAppearance.performAsCurrentDrawingAppearance {
            layer?.backgroundColor = NSColor.nightcapSeparator.cgColor
        }
    }
}

/// The mockups' palette, light and dark.
extension NSColor {
    static func appearance(light: UInt32, dark: UInt32, lightAlpha: CGFloat = 1, darkAlpha: CGFloat = 1) -> NSColor {
        NSColor(name: nil) { appearance in
            let isDark = appearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua
            let hex = isDark ? dark : light
            return NSColor(
                srgbRed: CGFloat((hex >> 16) & 0xff) / 255,
                green: CGFloat((hex >> 8) & 0xff) / 255,
                blue: CGFloat(hex & 0xff) / 255,
                alpha: isDark ? darkAlpha : lightAlpha)
        }
    }

    static var nightcapWindow: NSColor { appearance(light: 0xf4f4f6, dark: 0x1e1e21) }
    static var nightcapGroup: NSColor { appearance(light: 0xffffff, dark: 0x2b2b2e) }
    static var nightcapSeparator: NSColor {
        appearance(light: 0x000000, dark: 0xffffff, lightAlpha: 0.1, darkAlpha: 0.09)
    }
    static var nightcapSecondary: NSColor { appearance(light: 0x5f5f64, dark: 0xa9a9af) }
    static var nightcapControl: NSColor { appearance(light: 0xffffff, dark: 0x3a3a3e) }
    static var nightcapControlPressed: NSColor { appearance(light: 0xe8e8ec, dark: 0x4a4a4f) }
    static var nightcapControlBorder: NSColor {
        appearance(light: 0x000000, dark: 0xffffff, lightAlpha: 0.16, darkAlpha: 0.12)
    }
    static var nightcapTile: NSColor {
        appearance(light: 0x000000, dark: 0xffffff, lightAlpha: 0.07, darkAlpha: 0.1)
    }
    /// The design system's gold for marks: its `moon-text` by day, since the moon itself is too
    /// pale to see on a light window, and the moon at night.
    static var nightcapMoon: NSColor { appearance(light: 0x8a5b00, dark: 0xf6d58a) }
    static var nightcapGreen: NSColor { appearance(light: 0x22b33a, dark: 0x30d158) }
    static var nightcapOrange: NSColor { appearance(light: 0xf08a00, dark: 0xff9f0a) }
    static var nightcapRed: NSColor { appearance(light: 0xe5372c, dark: 0xff453a) }
    static var nightcapGrey: NSColor { appearance(light: 0xa1a1a6, dark: 0x6e6e73) }
    /// Onboarding's stage, a soft fall from top to bottom behind its illustrations.
    static var nightcapStageTop: NSColor { appearance(light: 0xe9edf4, dark: 0x272d3b) }
    static var nightcapStageBottom: NSColor { appearance(light: 0xd6dde9, dark: 0x1b202b) }
}

/// A strength slider, 0 to 100, for the menu and onboarding. Its `mouseDown` tracks the whole
/// drag and returns only once the mouse goes up, so moves inside it are a drag, which previews,
/// and its return is the release, which saves.
final class StrengthSlider: NSSlider {
    private(set) var dragging = false
    var onRelease: () -> Void = {}

    override func mouseDown(with event: NSEvent) {
        dragging = true
        super.mouseDown(with: event)
        dragging = false
        onRelease()
    }
}

/// Onboarding's step markers: a dot per step in groups, the current one a gold pill. Steps
/// already reached can be clicked.
final class StepDots: NSView {
    struct Dot {
        let state: OnboardingFlow.Dot.State
        let label: String
        let go: (() -> Void)?
    }

    init(groups: [[Dot]]) {
        super.init(frame: .zero)
        var views: [NSView] = []
        for (index, group) in groups.enumerated() {
            if index > 0 {
                let gap = NSView()
                gap.widthAnchor.constraint(equalToConstant: 10).isActive = true
                views.append(gap)
            }
            views += group.map(DotView.init)
        }
        let stack = NSStackView(views: views)
        stack.orientation = .horizontal
        stack.spacing = 0
        stack.translatesAutoresizingMaskIntoConstraints = false
        addSubview(stack)
        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(equalTo: leadingAnchor),
            stack.trailingAnchor.constraint(equalTo: trailingAnchor),
            stack.topAnchor.constraint(equalTo: topAnchor),
            stack.bottomAnchor.constraint(equalTo: bottomAnchor),
        ])
    }

    required init?(coder: NSCoder) {
        fatalError("not built from a nib")
    }

    /// One dot, with room around it to click.
    private final class DotView: NSView {
        private let dot: Dot

        init(_ dot: Dot) {
            self.dot = dot
            super.init(frame: .zero)
            widthAnchor.constraint(equalToConstant: dot.state == .current ? 24 : 12).isActive = true
            heightAnchor.constraint(equalToConstant: 22).isActive = true
            setAccessibilityElement(true)
            setAccessibilityRole(dot.go == nil ? .staticText : .button)
            setAccessibilityLabel(dot.state == .current ? Wording.system.current(dot.label) : dot.label)
        }

        required init?(coder: NSCoder) {
            fatalError("not built from a nib")
        }

        override func draw(_ dirtyRect: NSRect) {
            let width: CGFloat = dot.state == .current ? 18 : 6
            let pill = NSRect(x: (bounds.width - width) / 2, y: (bounds.height - 6) / 2, width: width, height: 6)
            let colour: NSColor =
                switch dot.state {
                case .current: .nightcapMoon
                case .done: .nightcapSecondary
                case .todo: .nightcapControlBorder
                }
            colour.setFill()
            NSBezierPath(roundedRect: pill, xRadius: 3, yRadius: 3).fill()
        }

        override func resetCursorRects() {
            if dot.go != nil { addCursorRect(bounds, cursor: .pointingHand) }
        }

        override func mouseDown(with event: NSEvent) {
            dot.go?()
        }

        override func accessibilityPerformPress() -> Bool {
            dot.go?()
            return dot.go != nil
        }
    }
}

/// A control's target that runs a closure; the control keeps it alive.
final class Action: NSObject {
    nonisolated(unsafe) static var key = 0
    private let body: () -> Void

    init(_ body: @escaping () -> Void) {
        self.body = body
    }

    @objc func run() {
        body()
    }
}

extension NSFont {
    /// The wordmark's face, Caprasimo, which the app bundles beside its licence. Missing, it's a
    /// broken bundle, not something to work around.
    static func nightcapWordmark(size: CGFloat) -> NSFont {
        registerBundledFonts
        guard let font = NSFont(name: "Caprasimo-Regular", size: size) else {
            fatalError("Caprasimo isn't in the app bundle")
        }
        return font
    }

    private static let registerBundledFonts: Void = {
        guard let url = Bundle.main.url(forResource: "Caprasimo-Regular", withExtension: "ttf") else {
            fatalError("Caprasimo-Regular.ttf isn't in the app bundle")
        }
        var error: Unmanaged<CFError>?
        if !CTFontManagerRegisterFontsForURL(url as CFURL, .process, &error) {
            fatalError("couldn't register Caprasimo: \(error!.takeRetainedValue())")
        }
    }()
}
