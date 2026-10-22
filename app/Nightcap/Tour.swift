import AppKit
import KeyboardShortcuts
import NightcapCore
import os

/// The tour (ADR-0016): a floating panel at the top of the screen that walks through the moon, a
/// rule the user makes for Tour Sample and sees act, snoozing that rule, the same rule for a site
/// where website checking can, and the peek. Every step can be skipped, and the tour ended.
/// While it runs, the core has Night Shift on for it if it was off, and holds the rules added
/// during it in memory.
final class Tour {
    enum Step {
        case moon, rule, snooze, site, peek, end
    }

    /// The browser the site step opens the project's site in: the default one, while website
    /// checking can watch it.
    struct Browser {
        let bundleId: String
        let name: String
    }

    private enum Try {
        case tap, hold
    }

    /// The sample app's ID, which its rule names.
    static let sampleApp = Bundle.main.bundleIdentifier! + ".tour-sample"
    private static let site = URL(string: "https://nightcap.choco.me")!
    private static let width: CGFloat = 330
    private static let padding: CGFloat = 16
    /// How far the panel moves aside while the menu is open, about the menu's width.
    private static let menuWidth: CGFloat = 320

    private let log = Logger(subsystem: Bundle.main.bundleIdentifier!, category: "tour")
    /// Night Shift is off, so the core has it on for the tour once it begins.
    private let standsIn: Bool
    private let shortcut: KeyboardShortcuts.Shortcut?
    private let browser: Browser?
    private let steps: [Step]
    private var index = 0
    /// The core hears of the tour once the user continues from its first step.
    private var begun = false
    private var front: String?
    /// What the sample's rule says, once it has one.
    private var rule: OnOff?
    private var returned = false
    private var snoozed = false
    private var ruleBack = false
    private var siteRuled = false
    private var siteLeft = false
    /// Whether a force holds Night Shift off, as the moon shows.
    private var forced = false
    private var keysDown = false
    /// Whether Night Shift is on everywhere, which a tap of the shortcut switches.
    private var switchedOn = true
    private var lastTry: Try?
    private var tried = false
    private let panel = FloatingPanel()
    private weak var keyCaps: KeyCaps?
    private weak var hint: NSTextField?
    /// Reopens the sample if the user quits it during the rule step.
    private var terminations: NSObjectProtocol?
    private let press: () -> Void
    private let release: () -> Void
    private let begin: () -> Void
    private let onEnd: (_ begun: Bool) -> Void

    /// `press` and `release` work the shortcut as its keys would, for the keycaps; `begin` runs
    /// as the user continues from the first step, and `onEnd` once the tour is over. Without
    /// `browser`, the tour has no site step.
    init(
        standsIn: Bool,
        shortcut: KeyboardShortcuts.Shortcut?,
        browser: Browser?,
        press: @escaping () -> Void,
        release: @escaping () -> Void,
        begin: @escaping () -> Void,
        onEnd: @escaping (_ begun: Bool) -> Void
    ) {
        self.standsIn = standsIn
        self.shortcut = shortcut
        self.browser = browser
        self.press = press
        self.release = release
        self.begin = begin
        self.onEnd = onEnd
        // Without a shortcut there's nothing to peek with.
        steps = [.moon, .rule, .snooze] + (browser == nil ? [] : [.site]) + (shortcut == nil ? [] : [.peek]) + [.end]
    }

    private var step: Step { steps[index] }

    /// The rule the rule step asks for, which the snooze step snoozes.
    private var ruled: Bool { rule == .off }

    /// Tour Sample stays open through the steps that use it.
    private var usesSample: Bool { step == .rule || step == .snooze }

    /// At the top right, under the menu bar, where the moon is: on macOS 27 an app isn't told
    /// where its status item is, so nothing points at it.
    func start() {
        draw()
        if let screen = NSScreen.main?.visibleFrame {
            panel.setFrameOrigin(NSPoint(
                x: screen.maxX - panel.frame.width - 12, y: screen.maxY - panel.frame.height - 8))
        }
        panel.makeKeyAndOrderFront(nil)
    }

    /// Brings the panel forward, as taking the tour again asks.
    func reveal() {
        panel.orderFrontRegardless()
    }

    func end() {
        panel.close()
        closeSample()
        onEnd(begun)
    }

    // MARK: What the tour follows

    /// The front app as reported to the core.
    func frontmostChanged(_ bundleId: String?) {
        let left = front != bundleId
        let was = front
        front = bundleId
        guard left else { return }
        switch step {
        case .rule where was == Self.sampleApp && ruled: returned = true
        case .rule, .snooze, .site: break
        default: return
        }
        draw()
    }

    /// The core's decision after each change. The site's force lifting means the user left its
    /// tab, which only the core sees. The sample's force coming back ends its snooze, whether the
    /// user put the rule back or the snooze ran out.
    func forcedChanged(_ forced: Bool) {
        let lifted = self.forced && !forced
        let back = !self.forced && forced
        self.forced = forced
        if back, step == .snooze, snoozed, front == Self.sampleApp {
            snoozed = false
            ruleBack = true
            draw()
        }
        guard lifted, step == .site, siteRuled, !siteLeft else { return }
        siteLeft = true
        draw()
    }

    /// A command chosen in the menu, which may add or remove the rule a step asks for, or snooze
    /// it. A rule added or removed takes the snooze with it.
    func chose(_ command: Command) {
        switch command {
        case .addRule(.app(let bundleId), let says) where bundleId == Self.sampleApp:
            rule = says
            snoozed = false
        case .removeRule(.app(let bundleId)) where bundleId == Self.sampleApp:
            rule = nil
            snoozed = false
        case .set(.app(let bundleId), .on, _) where bundleId == Self.sampleApp && step == .snooze && ruled:
            snoozed = true
        case .addRule(.domain, _) where step == .site: siteRuled = true
        case .removeRule(.domain) where step == .site: siteRuled = false
        default: return
        }
        draw()
    }

    /// The menu drops from the moon, near the panel, so the panel moves aside while it's open.
    func menuOpened() {
        panel.animator().setFrameOrigin(NSPoint(x: panel.frame.minX - Self.menuWidth, y: panel.frame.minY))
    }

    func menuClosed() {
        panel.animator().setFrameOrigin(NSPoint(x: panel.frame.minX + Self.menuWidth, y: panel.frame.minY))
    }

    /// Night Shift on or off everywhere, after each change, however it was made.
    func switchChanged(on: Bool) {
        guard switchedOn != on else { return }
        switchedOn = on
        if step == .peek { update() }
    }

    func shortcutPressed() {
        keysDown = true
        update()
    }

    func shortcutTapped() {
        keysDown = false
        tried(.tap)
    }

    func shortcutReleased() {
        keysDown = false
        tried(.hold)
    }

    private func tried(_ kind: Try) {
        guard step == .peek else { return }
        lastTry = kind
        let firstTry = !tried
        tried = true
        // The keycaps may be handling the mouse-up that got here.
        DispatchQueue.main.async { [weak self] in firstTry ? self?.draw() : self?.update() }
    }

    // MARK: Steps

    private func advance() {
        if step == .moon, !begun {
            begun = true
            begin()
        }
        let hadSample = usesSample
        index += 1
        // Snoozing needs the rule the step before asks for.
        if step == .snooze, !ruled { index += 1 }
        if hadSample, !usesSample { closeSample() }
        log.info("tour at step \(String(describing: self.step), privacy: .public)")
        if step == .rule { openSample() }
        if step == .site { NSWorkspace.shared.open(Self.site) }
        draw()
    }

    private var stepLabel: String {
        step == .end ? "Tour" : "\(index + 1) of \(steps.count - 1)"
    }

    private var keys: String { shortcut?.description ?? "" }

    private var texts: (title: String, body: String) {
        let browser = self.browser?.name ?? ""
        switch step {
        case .moon:
            return ("The moon in the menu bar is Nightcap", "It shows what Night Shift is doing right now.")
        case .rule where returned:
            return (
                "And it comes back",
                "Leave the sample and Night Shift returns, as warm as before. Every rule works this way.")
        case .rule where ruled && front == Self.sampleApp:
            return (
                "That’s true colour",
                "Night Shift is off while the sample is in front, and the moon shows a “z”. Now click another window, or the desktop.")
        case .rule where front == Self.sampleApp:
            return ("Give it a rule", "Click the moon to open the menu, then choose “Turn Off in Tour Sample” › “Always”.")
        case .rule:
            return ("Try a rule", "Bring Tour Sample to the front: click its window, or its icon in the Dock.")
        case .snooze where ruleBack:
            return (
                "And the rule’s back",
                "True colour again in the sample. Rules work the other way too: while Night Shift is off, “Turn On in Mail” › “Always” keeps it on in Mail.")
        case .snooze where snoozed && front == Self.sampleApp:
            return (
                "That’s a snooze",
                "Night Shift is back in the sample until the time you chose, then the rule returns by itself. To end it sooner, choose “Turn Off in Tour Sample” › “Put the Rule Back”.")
        case .snooze where snoozed:
            return (
                "Put the rule back",
                "Bring Tour Sample back to the front, then choose “Turn Off in Tour Sample” › “Put the Rule Back”.")
        case .snooze where !ruled && front == Self.sampleApp:
            return ("Snoozing needs a rule", "Choose “Turn Off in Tour Sample” › “Always” to give the sample its rule again.")
        case .snooze where !ruled:
            return (
                "Snoozing needs a rule",
                "Bring Tour Sample back to the front, then choose “Turn Off in Tour Sample” › “Always”.")
        case .snooze where front == Self.sampleApp:
            return (
                "Snooze it",
                "The rule keeps Night Shift off here. To give it a break, click the moon, then choose “Turn On in Tour Sample” and how long for.")
        case .snooze:
            return (
                "Snooze a rule",
                "A rule can take a break. Bring Tour Sample back to the front: click its window, or its icon in the Dock.")
        case .site where siteLeft:
            return (
                "Sites work the same way",
                "Leave the site’s tab and Night Shift returns. A site’s rule covers all its pages, in Safari and Chrome.")
        case .site where siteRuled:
            return ("That’s true colour, on one site", "Night Shift is off while its tab is in front. Now switch to another tab.")
        case .site where front == self.browser?.bundleId:
            return (
                "Now a site",
                "\(browser) has Nightcap’s site open. Click the moon, then choose “Turn Off at This Site” › “Always”.")
        case .site:
            return ("Now a site", "Bring \(browser) back to the front, on Nightcap’s site.")
        case .peek:
            return (
                "Peek at true colour",
                "Hold \(keys) in any app to see true colour for as long as you hold it. Tap it to turn Night Shift off, and again to turn it back on.")
        case .end:
            return (
                "That’s the tour",
                "What you changed during it goes with it, and your own rules are as they were. Everything else is in the moon’s menu: turning Night Shift on or off, everywhere or just in the app in front, for as long as you choose, and Settings, where you’ll find the tour again.")
        }
    }

    private var hintText: String {
        if keysDown { return "True colour while you hold. Let go and the warmth comes back." }
        if !switchedOn { return "Night Shift is off. Tap the shortcut to turn it back on." }
        switch lastTry {
        case .hold: return "That was a peek. Tap it instead to turn Night Shift off."
        case .tap: return "Night Shift is back on. Now try holding it."
        case nil: return "Hold the keys on your keyboard, or here."
        }
    }

    /// The step's buttons, secondary first: Skip until the step is done, then Continue.
    private var buttons: [NSButton] {
        let next: () -> Void = { [weak self] in self?.advance() }
        let done: Bool
        switch step {
        case .moon: done = true
        case .rule: done = returned
        case .snooze: done = ruleBack
        case .site: done = siteLeft
        case .peek: done = tried
        case .end:
            return [firstClick(Look.button("Done", .primary) { [weak self] in self?.end() })]
        }
        let buttons = done
            ? [Look.button("Continue", .primary, next)]
            : [Look.button("Skip", .regular, next)]
        return buttons.map(firstClick)
    }

    /// The panel never takes the focus, so a click on it is often its first.
    private func firstClick(_ button: NSButton) -> NSButton {
        (button as? CapsuleButton)?.actsOnFirstClick = true
        return button
    }

    // MARK: Drawing

    /// Keeps the panel's top right corner where it is as its height follows the step.
    private func draw() {
        let content = self.content()
        let glass = panel.glass
        glass.subviews.forEach { $0.removeFromSuperview() }
        content.translatesAutoresizingMaskIntoConstraints = false
        glass.addSubview(content)
        NSLayoutConstraint.activate([
            content.leadingAnchor.constraint(equalTo: glass.leadingAnchor),
            content.topAnchor.constraint(equalTo: glass.topAnchor),
        ])
        fit(content.fittingSize)
    }

    /// Redraws only the keycaps and their hint, which a press changes.
    private func update() {
        keyCaps?.down = keysDown
        if let hint {
            hint.attributedStringValue = NSAttributedString(
                string: hintText, attributes: hint.attributedStringValue.attributes(at: 0, effectiveRange: nil))
        }
        if let content = panel.glass.subviews.first { fit(content.fittingSize) }
    }

    private func fit(_ size: NSSize) {
        let frame = panel.frame
        panel.setFrame(
            NSRect(x: frame.maxX - size.width, y: frame.maxY - size.height, width: size.width, height: size.height),
            display: true)
    }

    private func content() -> NSView {
        let inner = Self.width - 2 * Self.padding
        let label = Look.label(stepLabel, size: 11, weight: .semibold, color: .nightcapMoon, width: inner)
        var header: [NSView] = [label, NSView()]
        if step != .end {
            let endTour = FirstClickButton(title: "End Tour", target: nil, action: nil)
            endTour.isBordered = false
            endTour.attributedTitle = NSAttributedString(
                string: "End Tour",
                attributes: [.font: NSFont.systemFont(ofSize: 11), .foregroundColor: NSColor.secondaryLabelColor])
            Look.attach(endTour) { [weak self] in self?.end() }
            header.append(endTour)
        }
        let top = NSStackView(views: header)
        top.orientation = .horizontal
        top.distribution = .fill
        label.setContentHuggingPriority(.required, for: .horizontal)

        let (title, body) = texts
        var views: [NSView] = [
            top,
            Look.label(title, size: 15, weight: .semibold, color: .labelColor, width: inner),
            // The system's label colours, which stay readable on the panel's glass.
            Look.label(body, size: 12, weight: .regular, color: .secondaryLabelColor, width: inner),
        ]
        if standsIn, step == .moon {
            views.append(Look.label(
                "Night Shift is off right now. When you continue, Nightcap turns it on for the tour, and back off once it ends.",
                size: 12, weight: .regular, color: .nightcapMoon, width: inner))
        }
        switch step {
        case .moon: views.append(legend(width: inner))
        case .peek: views.append(peekKeys(width: inner))
        default: break
        }
        let lastText = views.last
        let buttons = self.buttons
        if !buttons.isEmpty {
            let row = NSStackView(views: [NSView()] + buttons)
            row.orientation = .horizontal
            row.spacing = 8
            views.append(row)
        }
        let stack = NSStackView(views: views)
        stack.orientation = .vertical
        stack.alignment = .leading
        stack.spacing = 8
        stack.edgeInsets = NSEdgeInsets(top: 14, left: Self.padding, bottom: 14, right: Self.padding)
        stack.widthAnchor.constraint(equalToConstant: Self.width).isActive = true
        for view in views where view is NSStackView || view is Group {
            view.widthAnchor.constraint(equalTo: stack.widthAnchor, constant: -2 * Self.padding).isActive = true
        }
        if !buttons.isEmpty, let lastText { stack.setCustomSpacing(12, after: lastText) }
        return stack
    }

    /// The moon's three looks, as the status item draws them.
    private func legend(width: CGFloat) -> NSView {
        let rows: [(MenuIcon, String, String)] = [
            (.on, "On", "Night Shift is warming the screen."),
            (.off, "Off", "True colour, from the schedule or from you."),
            (.forcedOff, "Forced off", "Nightcap is holding it off for an app or a site."),
        ]
        var views: [NSView] = []
        for (index, (icon, name, meaning)) in rows.enumerated() {
            if index > 0 { views.append(Hairline()) }
            let moon = NSImageView(image: StatusMenu.image(for: icon)!)
            moon.contentTintColor = .labelColor
            moon.widthAnchor.constraint(equalToConstant: 16).isActive = true
            moon.heightAnchor.constraint(equalToConstant: 16).isActive = true
            let title = Look.label(name, size: 13, weight: .regular, color: .labelColor, width: 72)
            title.widthAnchor.constraint(equalToConstant: 72).isActive = true
            let text = Look.label(meaning, size: 11, weight: .regular, color: .secondaryLabelColor, width: width - 20 - 16 - 72 - 20)
            let row = NSStackView(views: [moon, title, text])
            row.orientation = .horizontal
            row.alignment = .centerY
            row.spacing = 10
            views.append(Look.padded(row, top: 8, left: 10, bottom: 8, right: 10))
        }
        let rowsStack = NSStackView(views: views)
        rowsStack.orientation = .vertical
        rowsStack.alignment = .leading
        rowsStack.spacing = 0
        for view in views {
            view.widthAnchor.constraint(equalTo: rowsStack.widthAnchor).isActive = true
        }
        return Group(rowsStack)
    }

    /// The shortcut's keys, pressed down while it's held, on the keyboard or with the mouse, and
    /// a hint under them that follows what the user tries.
    private func peekKeys(width: CGFloat) -> NSView {
        let modifiers = shortcut?.modifiers.ks_symbolicRepresentation ?? ""
        let keys = modifiers.map(String.init) + [String(self.keys.dropFirst(modifiers.count))]
        let caps = KeyCaps(keys: keys, label: self.keys, press: press, release: release)
        caps.down = keysDown
        keyCaps = caps
        let hint = Look.label(hintText, size: 12, weight: .regular, color: .secondaryLabelColor, width: width)
        hint.alignment = .center
        self.hint = hint
        let stack = NSStackView(views: [caps, hint])
        stack.orientation = .vertical
        stack.alignment = .centerX
        stack.spacing = 8
        stack.edgeInsets = NSEdgeInsets(top: 4, left: 0, bottom: 0, right: 0)
        return stack
    }

    // MARK: The sample

    /// Tour Sample.app, beside the CLI in Nightcap's bundle: an app of its own, so it stays the
    /// front app as the menu opens, which a window of Nightcap's wouldn't (macOS 27 hands the
    /// focus on as a status item's own app opens its menu).
    private func openSample() {
        let url = Bundle.main.bundleURL.appendingPathComponent("Contents/Helpers/Tour Sample.app")
        let configuration = NSWorkspace.OpenConfiguration()
        configuration.addsToRecentItems = false
        let log = self.log
        NSWorkspace.shared.openApplication(at: url, configuration: configuration) { _, error in
            if let error {
                log.error("couldn't open the tour's sample: \(error.localizedDescription, privacy: .public)")
            }
        }
        guard terminations == nil else { return }
        terminations = NSWorkspace.shared.notificationCenter.addObserver(
            forName: NSWorkspace.didTerminateApplicationNotification, object: nil, queue: .main
        ) { [weak self] notification in
            let app = notification.userInfo?[NSWorkspace.applicationUserInfoKey] as? NSRunningApplication
            MainActor.assumeIsolated {
                guard let self, app?.bundleIdentifier == Self.sampleApp, self.usesSample else { return }
                self.openSample()
            }
        }
    }

    private func closeSample() {
        if let terminations { NSWorkspace.shared.notificationCenter.removeObserver(terminations) }
        terminations = nil
        for app in NSRunningApplication.runningApplications(withBundleIdentifier: Self.sampleApp) {
            app.terminate()
        }
    }

    #if DEBUG
    /// The panel at `step` as a PNG, on the menu's colour, for comparing against the prototype.
    func snapshot(
        _ step: Step, front: String? = nil, ruled: Bool = false, returned: Bool = false, snoozed: Bool = false,
        ruleBack: Bool = false, siteRuled: Bool = false, siteLeft: Bool = false
    ) -> Data? {
        index = steps.firstIndex(of: step)!
        self.front = front
        rule = ruled ? .off : nil
        self.returned = returned
        self.snoozed = snoozed
        self.ruleBack = ruleBack
        self.siteRuled = siteRuled
        self.siteLeft = siteLeft
        let content = self.content()
        let size = content.fittingSize
        let backdrop = SnapshotBackdrop(frame: NSRect(origin: .zero, size: size))
        content.frame = backdrop.bounds
        backdrop.addSubview(content)
        backdrop.appearance = NSApp.appearance
        backdrop.layoutSubtreeIfNeeded()
        guard let bitmap = backdrop.bitmapImageRepForCachingDisplay(in: backdrop.bounds) else { return nil }
        backdrop.cacheDisplay(in: backdrop.bounds, to: bitmap)
        return bitmap.representation(using: .png, properties: [:])
    }
    #endif
}

/// A plain button that acts on a click into the panel when it isn't key.
private final class FirstClickButton: NSButton {
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }
}

/// The shortcut's keys on a tray, each a raised cap that sinks while the shortcut is held. Holding
/// them with the mouse works the shortcut as its keys would.
private final class KeyCaps: NSView {
    private static let cap: CGFloat = 36
    private static let gap: CGFloat = 6
    private static let inset = NSSize(width: 12, height: 10)
    private let keys: [String]
    private let press: () -> Void
    private let release: () -> Void
    var down = false {
        didSet { needsDisplay = true }
    }

    init(keys: [String], label: String, press: @escaping () -> Void, release: @escaping () -> Void) {
        self.keys = keys
        self.press = press
        self.release = release
        super.init(frame: .zero)
        setAccessibilityElement(true)
        setAccessibilityRole(.button)
        setAccessibilityLabel(label)
        setAccessibilityHelp("Hold to see true colour, tap to turn Night Shift off or on")
    }

    required init?(coder: NSCoder) {
        fatalError("not built from a nib")
    }

    private var capWidths: [CGFloat] {
        keys.map { key in
            let width = (key as NSString).size(withAttributes: [.font: Self.font]).width + 16
            return max(Self.cap, width.rounded(.up))
        }
    }

    private static let font = NSFont.systemFont(ofSize: 17)

    override var intrinsicContentSize: NSSize {
        let widths = capWidths
        return NSSize(
            width: widths.reduce(0, +) + Self.gap * CGFloat(widths.count - 1) + 2 * Self.inset.width,
            height: Self.cap + 1.5 + 2 * Self.inset.height)
    }

    override var isFlipped: Bool { true }

    override func draw(_ dirtyRect: NSRect) {
        effectiveAppearance.performAsCurrentDrawingAppearance {
            let tray = NSBezierPath(roundedRect: bounds.insetBy(dx: 0.25, dy: 0.25), xRadius: 12, yRadius: 12)
            NSColor.nightcapWindow.setFill()
            tray.fill()
            NSColor.nightcapControlBorder.setStroke()
            tray.lineWidth = 0.5
            tray.stroke()
            var x = Self.inset.width
            let sink: CGFloat = down ? 1.5 : 0
            for (key, width) in zip(keys, capWidths) {
                let frame = NSRect(x: x, y: Self.inset.height + sink, width: width, height: Self.cap)
                if !down {
                    NSColor.nightcapControlBorder.setFill()
                    NSBezierPath(roundedRect: frame.offsetBy(dx: 0, dy: 1.5), xRadius: 7, yRadius: 7).fill()
                }
                let cap = NSBezierPath(roundedRect: frame, xRadius: 7, yRadius: 7)
                NSColor.nightcapControl.setFill()
                cap.fill()
                NSColor.nightcapControlBorder.setStroke()
                cap.lineWidth = 0.5
                cap.stroke()
                let attributes: [NSAttributedString.Key: Any] = [.font: Self.font, .foregroundColor: NSColor.labelColor]
                let text = (key as NSString).size(withAttributes: attributes)
                (key as NSString).draw(
                    at: NSPoint(x: frame.midX - text.width / 2, y: frame.midY - text.height / 2),
                    withAttributes: attributes)
                x += width + Self.gap
            }
        }
    }

    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    override func mouseDown(with event: NSEvent) {
        press()
    }

    override func mouseUp(with event: NSEvent) {
        release()
    }
}

#if DEBUG
/// The menu's colour behind a snapshot of the panel, which the panel's glass would show.
private final class SnapshotBackdrop: NSView {
    override func draw(_ dirtyRect: NSRect) {
        effectiveAppearance.performAsCurrentDrawingAppearance {
            NSColor.appearance(light: 0xf0f0f2, dark: 0x2a2a2d).setFill()
            bounds.fill()
        }
    }
}
#endif
