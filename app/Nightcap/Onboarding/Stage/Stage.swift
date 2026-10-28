import AppKit
import KeyboardShortcuts

/// Onboarding's stage: a soft gradient behind the step's illustration, under the title band
/// with the wordmark.
final class Stage: NSView {
    private let sun: Sun?
    private let now: () -> Date
    private let shortcut: KeyboardShortcuts.Shortcut?
    /// Where the illustration's centre sits: in the middle of what the title band leaves.
    private let middle: CGFloat
    private var scene: Scene?
    private var art: NSView?

    /// The strength the mocks of a screen show, which the strength step's follows as it moves.
    var strength: UInt8 = 50 {
        didSet { (art as? StrengthArt)?.strength = strength }
    }

    init(band: CGFloat, height: CGFloat, sun: Sun?, now: @escaping () -> Date, shortcut: KeyboardShortcuts.Shortcut?) {
        self.sun = sun
        self.now = now
        self.shortcut = shortcut
        middle = band + (height - band) / 2
        super.init(frame: .zero)
        let wordmark = Self.wordmark()
        let line = Hairline()
        for view in [wordmark, line] {
            view.translatesAutoresizingMaskIntoConstraints = false
            addSubview(view)
        }
        NSLayoutConstraint.activate([
            wordmark.centerXAnchor.constraint(equalTo: centerXAnchor),
            wordmark.centerYAnchor.constraint(equalTo: topAnchor, constant: band / 2),
            line.leadingAnchor.constraint(equalTo: leadingAnchor),
            line.trailingAnchor.constraint(equalTo: trailingAnchor),
            line.bottomAnchor.constraint(equalTo: bottomAnchor),
        ])
    }

    required init?(coder: NSCoder) {
        fatalError("not built from a nib")
    }

    /// Replaces the illustration when the scene changes; the same one keeps its loops running.
    func show(_ scene: Scene) {
        guard scene != self.scene else { return }
        self.scene = scene
        art?.removeFromSuperview()
        let art = StageArt.view(for: scene, strength: strength, sun: sun, now: now(), shortcut: shortcut)
        art.translatesAutoresizingMaskIntoConstraints = false
        addSubview(art)
        NSLayoutConstraint.activate([
            art.centerXAnchor.constraint(equalTo: centerXAnchor),
            art.centerYAnchor.constraint(equalTo: topAnchor, constant: middle),
            art.widthAnchor.constraint(equalToConstant: art.frame.width),
            art.heightAnchor.constraint(equalToConstant: art.frame.height),
        ])
        self.art = art
    }

    /// A soft fall from top to bottom behind the illustrations.
    private static var top: NSColor { .appearance(light: 0xe9edf4, dark: 0x272d3b) }
    private static var bottom: NSColor { .appearance(light: 0xd6dde9, dark: 0x1b202b) }

    /// Drawn rather than a gradient layer, which snapshots wouldn't show.
    override func draw(_ dirtyRect: NSRect) {
        NSGradient(starting: Self.top, ending: Self.bottom)?.draw(in: bounds, angle: -90)
    }

    /// The app icon beside "Nightcap" in Caprasimo at 26 points, one of the brand's two moments
    /// in the app (design/README.md). The icon's tile fills 80% of its image, so a 28-point
    /// image shows a 22-point tile.
    private static func wordmark() -> NSView {
        let icon = NSImageView(image: NSApp.applicationIconImage)
        icon.imageScaling = .scaleProportionallyUpOrDown
        icon.widthAnchor.constraint(equalToConstant: 28).isActive = true
        icon.heightAnchor.constraint(equalToConstant: 28).isActive = true
        let name = NSTextField(labelWithString: "Nightcap")
        name.font = .nightcapWordmark(size: 26)
        name.textColor = .labelColor
        return NSStackView(.horizontal, [icon, name], spacing: 5)
    }
}
