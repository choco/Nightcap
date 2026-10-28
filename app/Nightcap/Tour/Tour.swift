import AppKit
import KeyboardShortcuts
import NightcapCore
import os

/// The tour (ADR-0016): a floating panel at the top of the screen that walks through the moon, a
/// rule the user makes for Tour Sample and sees act, snoozing that rule, the same rule for a site
/// where website checking can, and the peek. Every step can be skipped, and the tour ended.
/// It runs in a trial of the core's, which holds the rules added during it in memory and, where
/// the tour finds Night Shift off, stands in for the schedule with it on; the steps follow the
/// core's answers to the queries, and quote the menu's items as `Menu` words them.
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

    /// What happened during a step, which the core's answers show only as it happens: the user
    /// left the sample with its rule, the rule came back from its snooze, and the user left the
    /// site's tab with its rule.
    struct Milestones {
        var returned = false
        var ruleBack = false
        var siteLeft = false
    }

    private enum Try {
        case tap, hold
    }

    /// What the panel shows for its step, apart from the keycaps and their hint.
    private struct Shown: Equatable {
        let title: String
        let body: String
        let standsIn: Bool
        let done: Bool
    }

    /// Tour Sample.app, beside the CLI in Nightcap's bundle: an app of its own, so it stays the
    /// front app as the menu opens, which a window of Nightcap's wouldn't (macOS 27 hands the
    /// focus on as a status item's own app opens its menu).
    private static let sampleURL = Bundle.main.bundleURL.appendingPathComponent("Contents/Helpers/Tour Sample.app")
    /// The sample, by the ID its rule names and the name the menu calls it by once it's in front.
    static let sample = TourApp(Bundle(url: sampleURL)!)
    /// The app the steps give as an example of a rule that says on. Mail is part of macOS, so its
    /// bundle is always there.
    static let example = TourApp(
        Bundle(url: NSWorkspace.shared.urlForApplication(withBundleIdentifier: "com.apple.mail")!)!)
    private static let siteURL = URL(string: "https://nightcap.choco.me")!
    private static let width: CGFloat = 330
    private static let padding: CGFloat = 16
    /// How far the panel moves aside while the menu is open, about the menu's width.
    private static let menuWidth: CGFloat = 320

    private let log = Logger(subsystem: Bundle.main.bundleIdentifier!, category: "tour")
    private let shortcut: KeyboardShortcuts.Shortcut?
    private let browser: Browser?
    /// The project's site in `browser`, for the site step.
    private let site: TourSite?
    private let words = Wording.system
    /// The menu's items the steps quote.
    private let quotes: TourQuotes
    private let steps: [Step]
    private var index = 0
    /// The tour begins its trial once the user continues from its first step.
    private var begun = false
    /// What the steps follow, from the moment the core runs the trial.
    private var report: TourFacts?
    /// Whether the tour asks its trial to stand in for the schedule, from Night Shift as the core
    /// last heard, as the tour finds it before the trial begins.
    private var asksForStandIn = false
    private var milestones = Milestones()
    private var drawn: Shown?
    private var keysDown = false
    private var lastTry: Try?
    private var tried = false
    private let panel = FloatingPanel()
    private weak var keyCaps: KeyCaps?
    private weak var hint: NSTextField?
    /// Reopens the sample if the user quits it during the rule step.
    private var terminations: NSObjectProtocol?
    private let press: () -> Void
    private let release: () -> Void
    private let begin: (_ trial: AppCommand) -> Void
    private let onEnd: (_ begun: Bool) -> Void

    /// `press` and `release` work the shortcut as its keys would, for the keycaps; `begin` hands
    /// the core the trial's begin as the user continues from the first step, and `onEnd` runs
    /// once the tour is over, with whether the trial began. Without `browser`, the tour has no
    /// site step.
    init(
        shortcut: KeyboardShortcuts.Shortcut?,
        browser: Browser?,
        press: @escaping () -> Void,
        release: @escaping () -> Void,
        begin: @escaping (_ trial: AppCommand) -> Void,
        onEnd: @escaping (_ begun: Bool) -> Void
    ) {
        self.shortcut = shortcut
        self.browser = browser
        site = browser.map { TourSite(domain: Self.siteURL.host()!, browser: $0.bundleId) }
        quotes = TourQuotes(sample: Self.sample, site: site, example: Self.example, words: words)
        self.press = press
        self.release = release
        self.begin = begin
        self.onEnd = onEnd
        // Without a shortcut there's nothing to peek with.
        steps = [.moon, .rule, .snooze] + (browser == nil ? [] : [.site]) + (shortcut == nil ? [] : [.peek]) + [.end]
    }

    private var step: Step { steps[index] }

    /// What every step after the first follows.
    private var facts: TourFacts { report! }

    /// The rule the rule step asks for, which the snooze step snoozes.
    private var ruled: Bool { facts.sample.rule == .off }

    private var snoozed: Bool { ruled && facts.sample.snoozed }

    private var siteRuled: Bool { facts.site?.rule == .off }

    /// Whether the tour has Night Shift on in place of the schedule: as the core answers once the
    /// trial has begun, and until then as the tour will ask.
    private var standsIn: Bool { report?.standsIn ?? asksForStandIn }

    /// Tour Sample stays open through the steps that use it.
    private var usesSample: Bool { step == .rule || step == .snooze }

    /// At the top right, under the menu bar, where the moon is: on macOS 27 an app isn't told
    /// where its status item is, so nothing points at it. The first step follows `snapshot`.
    func start(_ snapshot: Snapshot) {
        refresh(snapshot)
        if let screen = NSScreen.main?.visibleFrame {
            panel.setFrameOrigin(
                NSPoint(
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

    /// Takes the core's snapshot after each change, however it was made, and redraws what it
    /// changes.
    func refresh(_ snapshot: Snapshot) {
        let report = TourFacts(snapshot, sample: Self.sample.bundleId, site: site?.domain)
        if let before = self.report, let after = report { follow(from: before, to: after) }
        self.report = report
        asksForStandIn = TourFacts.asksForStandIn(snapshot)
        guard reported else { return }
        if shown != drawn {
            draw()
        } else if step == .peek {
            update()
        }
    }

    /// Whether the step can be drawn: every step after the first follows what the core answers
    /// once it has decided the trial's begin.
    private var reported: Bool { step == .moon || report != nil }

    /// Records what the step at hand saw happen between two answers.
    private func follow(from before: TourFacts, to after: TourFacts) {
        let left = { (place: TourPlace?, now: TourPlace?) in
            place?.inFront == true && place?.rule == .off && now?.inFront == false
        }
        switch step {
        case .rule where left(before.sample, after.sample):
            milestones.returned = true
        case .snooze where before.sample.snoozed && !after.sample.snoozed && after.sample.rule == .off:
            milestones.ruleBack = true
        case .site where left(before.site, after.site):
            milestones.siteLeft = true
        default: break
        }
    }

    /// The menu drops from the moon, near the panel, so the panel moves aside while it's open.
    func menuOpened() {
        panel.animator().setFrameOrigin(NSPoint(x: panel.frame.minX - Self.menuWidth, y: panel.frame.minY))
    }

    func menuClosed() {
        panel.animator().setFrameOrigin(NSPoint(x: panel.frame.minX + Self.menuWidth, y: panel.frame.minY))
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
            begin(.beginTrial(standsIn: asksForStandIn))
        }
        let hadSample = usesSample
        index += 1
        // Snoozing needs the rule the step before asks for.
        if step == .snooze, !ruled { index += 1 }
        if hadSample, !usesSample { closeSample() }
        log.info("tour at step \(String(describing: self.step), privacy: .public)")
        if step == .rule { openSample() }
        if step == .site { NSWorkspace.shared.open(Self.siteURL) }
        if reported { draw() }
    }

    private var stepLabel: String {
        step == .end ? words.tour : words.step(index + 1, of: steps.count - 1)
    }

    private var keys: String { shortcut?.description ?? "" }

    private var texts: Wording.Said {
        let sample = Self.sample.name
        let browser = self.browser?.name ?? ""
        let addSampleRule = words.quoted(quotes.addSampleRule)
        let putBack = words.quoted(quotes.putSampleRuleBack)
        switch step {
        case .moon:
            return words.theMoonIsNightcap
        case .rule where milestones.returned:
            return words.itComesBack
        case .rule where ruled && facts.sample.inFront:
            return words.trueColourInTheSample
        case .rule where facts.sample.inFront:
            return words.giveItARule(addSampleRule)
        case .rule:
            return words.tryARule(sample)
        case .snooze where milestones.ruleBack:
            return words.theRulesBack(words.quoted(quotes.addExampleRule), in: Self.example.name)
        case .snooze where snoozed && facts.sample.inFront:
            return words.thatsASnooze(putBack)
        case .snooze where snoozed:
            return words.putTheRuleBack(sample, putBack)
        case .snooze where !ruled && facts.sample.inFront:
            return words.snoozingNeedsARule(addSampleRule)
        case .snooze where !ruled:
            return words.snoozingNeedsARule(sample, addSampleRule)
        case .snooze where facts.sample.inFront:
            return words.snoozeIt(words.quoted(row: quotes.snoozeSample))
        case .snooze:
            return words.snoozeARule(sample)
        case .site where milestones.siteLeft:
            return words.sitesWorkTheSameWay
        case .site where siteRuled:
            return words.trueColourOnOneSite
        case .site where facts.site?.inFront == true:
            return words.nowASite(browser, words.quoted(quotes.addSiteRule!))
        case .site:
            return words.nowASite(browser)
        case .peek:
            return words.peekAtTrueColour(keys)
        case .end:
            return words.thatsTheTour
        }
    }

    private var hintText: String {
        if keysDown { return words.trueColourWhileYouHold }
        if report?.everywhereOn == false { return words.tapToTurnItBackOn }
        switch lastTry {
        case .hold: return words.thatWasAPeek
        case .tap: return words.nowTryHolding
        case nil: return words.holdTheKeys
        }
    }

    /// Whether the step has shown what it's for, which turns its Skip into Continue.
    private var done: Bool {
        switch step {
        case .moon, .end: true
        case .rule: milestones.returned
        case .snooze: milestones.ruleBack
        case .site: milestones.siteLeft
        case .peek: tried
        }
    }

    private var shown: Shown {
        let (title, body) = texts
        return Shown(title: title, body: body, standsIn: step == .moon && standsIn, done: done)
    }

    /// The step's buttons, secondary first: Skip until the step is done, then Continue.
    private var buttons: [NSButton] {
        if step == .end {
            return [firstClick(CapsuleButton(words.done, .primary) { [weak self] in self?.end() })]
        }
        let next: () -> Void = { [weak self] in self?.advance() }
        return [
            firstClick(
                done ? CapsuleButton(words.continueTour, .primary, next) : CapsuleButton(words.skip, .regular, next))
        ]
    }

    /// The panel never takes the focus, so a click on it is often its first.
    private func firstClick(_ button: NSButton) -> NSButton {
        (button as? CapsuleButton)?.actsOnFirstClick = true
        return button
    }

    // MARK: Drawing

    /// Keeps the panel's top right corner where it is as its height follows the step.
    private func draw() {
        drawn = shown
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
        let label = NSTextField.label(stepLabel, size: 11, weight: .semibold, color: .nightcapMoon, width: inner)
        var header: [NSView] = [label, NSView()]
        if step != .end {
            let endTour = FirstClickButton(title: words.endTour, target: nil, action: nil)
            endTour.isBordered = false
            endTour.attributedTitle = NSAttributedString(
                string: words.endTour,
                attributes: [.font: NSFont.systemFont(ofSize: 11), .foregroundColor: NSColor.secondaryLabelColor])
            endTour.onAction { [weak self] in self?.end() }
            header.append(endTour)
        }
        let top = NSStackView(views: header)
        top.orientation = .horizontal
        top.distribution = .fill
        label.setContentHuggingPriority(.required, for: .horizontal)

        let (title, body) = texts
        var views: [NSView] = [
            top,
            NSTextField.label(title, size: 15, weight: .semibold, color: .labelColor, width: inner),
            // The system's label colours, which stay readable on the panel's glass.
            NSTextField.label(body, size: 12, weight: .regular, color: .secondaryLabelColor, width: inner),
        ]
        if standsIn, step == .moon {
            views.append(
                NSTextField.label(
                    words.onForTheTourOnceYouContinue, size: 12, weight: .regular, color: .nightcapMoon, width: inner))
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
        var views: [NSView] = []
        for (index, (icon, name, meaning)) in words.moonLegend.enumerated() {
            if index > 0 { views.append(Hairline()) }
            let moon = NSImageView(image: MenuArt.moon(for: icon)!)
            moon.contentTintColor = .labelColor
            moon.widthAnchor.constraint(equalToConstant: 16).isActive = true
            moon.heightAnchor.constraint(equalToConstant: 16).isActive = true
            let title = NSTextField.label(name, size: 13, weight: .regular, color: .labelColor, width: 72)
            title.widthAnchor.constraint(equalToConstant: 72).isActive = true
            let text = NSTextField.label(
                meaning, size: 11, weight: .regular, color: .secondaryLabelColor, width: width - 20 - 16 - 72 - 20)
            let row = NSStackView(views: [moon, title, text])
            row.orientation = .horizontal
            row.alignment = .centerY
            row.spacing = 10
            views.append(Padded(row, top: 8, left: 10, bottom: 8, right: 10))
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
        let hint = NSTextField.label(hintText, size: 12, weight: .regular, color: .secondaryLabelColor, width: width)
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

    private func openSample() {
        let configuration = NSWorkspace.OpenConfiguration()
        configuration.addsToRecentItems = false
        let log = self.log
        NSWorkspace.shared.openApplication(at: Self.sampleURL, configuration: configuration) { _, error in
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
                guard let self, app?.bundleIdentifier == Self.sample.bundleId, self.usesSample else { return }
                self.openSample()
            }
        }
    }

    private func closeSample() {
        if let terminations { NSWorkspace.shared.notificationCenter.removeObserver(terminations) }
        terminations = nil
        for app in NSRunningApplication.runningApplications(withBundleIdentifier: Self.sample.bundleId) {
            app.terminate()
        }
    }

    #if DEBUG
    /// The panel at `step` as a PNG, on the menu's colour, for comparing against the prototype:
    /// with `facts` as the core's answers would give them.
    func snapshot(_ step: Step, facts: TourFacts, milestones: Milestones = Milestones()) -> Data? {
        index = steps.firstIndex(of: step)!
        report = facts
        self.milestones = milestones
        let content = self.content()
        let size = content.fittingSize
        let backdrop = SnapshotBackdrop(frame: NSRect(origin: .zero, size: size))
        content.frame = backdrop.bounds
        backdrop.addSubview(content)
        backdrop.appearance = NSApp.appearance
        return backdrop.png()
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
        setAccessibilityHelp(Wording.system.keyCapsHelp)
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
                let attributes: [NSAttributedString.Key: Any] = [
                    .font: Self.font, .foregroundColor: NSColor.labelColor,
                ]
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
final class SnapshotBackdrop: NSView {
    override func draw(_ dirtyRect: NSRect) {
        effectiveAppearance.performAsCurrentDrawingAppearance {
            NSColor.appearance(light: 0xf0f0f2, dark: 0x2a2a2d).setFill()
            bounds.fill()
        }
    }
}
#endif

extension TourApp {
    /// The app in `bundle`, by the name the menu calls it once it's in front: the front app's
    /// `NSRunningApplication.localizedName`, which is the bundle's localized display name, else its
    /// localized name. Finder's name, `FileManager.displayName`, comes from the bundle's file name
    /// and Finder's settings, and can differ.
    fileprivate init(_ bundle: Bundle) {
        let name =
            bundle.object(forInfoDictionaryKey: "CFBundleDisplayName")
            ?? bundle.object(forInfoDictionaryKey: "CFBundleName")
        self.init(bundleId: bundle.bundleIdentifier!, name: name as! String)
    }
}
