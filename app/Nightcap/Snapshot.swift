#if DEBUG
import AppKit
import KeyboardShortcuts
import NightcapCore

/// Dev builds only: with `NIGHTCAP_SNAPSHOT` set to a folder, Nightcap renders the settings
/// window's tabs, onboarding's steps, the tour's panel and the pointer to the moon, light and
/// dark, from the mockups' scenarios into that folder, then quits.
enum Snapshot {
    static func runIfAsked() {
        guard let folder = ProcessInfo.processInfo.environment["NIGHTCAP_SNAPSHOT"] else { return }
        let settings = Settings(
            websiteChecking: SettingsSwitch(on: true, locked: false, command: .websites(.off)),
            openAtLogin: SettingsSwitch(on: true, locked: false, command: .openAtLogin(.off)),
            checks: [
                SettingsCheck(
                    kind: .nightShift, state: .ok,
                    text: "Available, following its schedule: sunset to sunrise.", restore: nil, accept: nil),
                SettingsCheck(
                    kind: .accessibility, state: .drifted,
                    text: "Accessibility isn’t allowed, so website checking can’t read the front tab.",
                    restore: FixButton(title: "Allow…", fix: .askForAccessibility),
                    accept: FixButton(title: "Stop Checking Websites", fix: .stopCheckingWebsites)),
                SettingsCheck(
                    kind: .notifications, state: .drifted,
                    text: "Notifications were allowed, but they’re switched off in System Settings.",
                    restore: FixButton(title: "Allow Notifications…", fix: .allowNotifications),
                    accept: FixButton(title: "Keep Notifications Off", fix: .keepNotificationsOff)),
                SettingsCheck(
                    kind: .configFile, state: .broken,
                    text: "The config file is invalid: line 4: unknown field `domian`.",
                    restore: FixButton(title: "Reload", fix: .reload), accept: nil),
                SettingsCheck(kind: .adHocRules, state: .ok, text: "2 ad-hoc rules.", restore: nil, accept: nil),
            ],
            configPath: "\(NSHomeDirectory())/.config/nightcap/config.toml",
            rules: [
                SettingsRule(
                    identity: .app("com.figma.Desktop"), says: .off, label: "From your config", snoozed: false,
                    action: nil),
                SettingsRule(
                    identity: .domain("figma.com"), says: .off, label: "Snoozed until 07:12", snoozed: true,
                    action: RuleAction(title: "Put Back", command: .reset(.domain("figma.com")))),
                SettingsRule(
                    identity: .app("com.apple.Photos"), says: .off, label: "Ad-hoc", snoozed: false,
                    action: RuleAction(title: "Remove", command: .removeRule(.app("com.apple.Photos")))),
                SettingsRule(
                    identity: .app("com.apple.iBooksX"), says: .on, label: "Ad-hoc", snoozed: false,
                    action: RuleAction(title: "Remove", command: .removeRule(.app("com.apple.iBooksX")))),
            ],
            forNow: [
                SettingsLayers(
                    place: .app("com.apple.mail"), label: "Night Shift off until 22:00",
                    action: RuleAction(title: "End", command: .reset(.app("com.apple.mail"))))
            ])
        // An empty folder, never filled, so sites show the globe whatever this Mac has fetched.
        let icons = SiteIcons(
            directory: FileManager.default.temporaryDirectory.appending(path: UUID().uuidString), online: false)
        let window = SettingsWindow(
            current: { settings }, icons: icons, choose: { _ in }, perform: { _ in }, checkForUpdates: {},
            export: { _ in "" }, takeTour: {}, onClose: {})
        for (name, appearance) in [("light", NSAppearance.Name.aqua), ("dark", .darkAqua)] {
            NSApp.appearance = NSAppearance(named: appearance)
            for tab in SettingsWindow.Tab.allCases {
                let url = URL(fileURLWithPath: folder).appendingPathComponent("app-\(tab.rawValue)-\(name).png")
                guard let png = window.snapshot(tab) else { fatalError("couldn't render \(tab)") }
                try! png.write(to: url)
            }
        }
        let setUp = OnboardingNightShift(
            askStrength: true, askSchedule: true, alreadySetUp: false, strength: 60, schedule: .off)
        let steps = { (nightShift: OnboardingNightShift) in
            Onboarding(
                steps: OnboardingSteps(nightShift: nightShift, websitesFromConfig: nil, opensAtLogin: false),
                rules: {
                    [
                        SettingsRule(
                            identity: .app("com.apple.Photos"), says: .off, label: "From your config", snoozed: false,
                            action: nil)
                    ]
                },
                icons: icons,
                sun: Sun(
                    sunset: Calendar.current.date(bySettingHour: 18, minute: 41, second: 0, of: Date())!,
                    sunrise: Calendar.current.date(bySettingHour: 7, minute: 52, second: 0, of: Date())!),
                accessibilityGranted: { false }, askForAccessibility: {}, choose: { _ in },
                notifications: { .notRequested }, askForNotifications: {}, onFinish: { _ in })
        }
        let newUser = steps(setUp)
        var alreadySetUp = setUp
        alreadySetUp.alreadySetUp = true
        alreadySetUp.schedule = .sunsetToSunrise
        let nightShiftUser = steps(alreadySetUp)
        let states: [(String, Onboarding, OnboardingFlow.Step, OnboardingFlow.WebsitesPage, Bool)] = [
            ("already-set-up", nightShiftUser, .alreadySetUp, .ask, false),
            ("review", nightShiftUser, .strength, .ask, true),
            ("strength", newUser, .strength, .ask, false),
            ("schedule", newUser, .schedule, .ask, false),
            ("apps", newUser, .apps, .ask, false),
            ("websites", newUser, .websites, .ask, false),
            ("accessibility", newUser, .websites, .accessibility, false),
            ("sites", newUser, .websites, .sites, false),
            ("last-things", newUser, .lastThings, .ask, false),
            ("ready", newUser, .ready, .ask, false),
        ]
        for (name, appearance) in [("light", NSAppearance.Name.aqua), ("dark", .darkAqua)] {
            NSApp.appearance = NSAppearance(named: appearance)
            for (state, onboarding, step, page, reviewing) in states {
                let url = URL(fileURLWithPath: folder).appendingPathComponent("app-onboarding-\(state)-\(name).png")
                guard let png = onboarding.snapshot(step, page: page, reviewing: reviewing) else {
                    fatalError("couldn't render \(state)")
                }
                try! png.write(to: url)
            }
        }
        let tour = Tour(
            shortcut: KeyboardShortcuts.Shortcut(.n, modifiers: [.control, .option, .command]),
            browser: Tour.Browser(bundleId: "com.apple.Safari", name: "Safari"),
            current: { (nil, .off) }, press: {}, release: {}, begin: { _ in }, onEnd: { _ in })
        let away = TourPlace(inFront: false, rule: nil, snoozed: false)
        let inFront = TourPlace(inFront: true, rule: nil, snoozed: false)
        let ruled = TourPlace(inFront: false, rule: .off, snoozed: false)
        let ruledInFront = TourPlace(inFront: true, rule: .off, snoozed: false)
        let snoozed = TourPlace(inFront: true, rule: .off, snoozed: true)
        let facts = { (sample: TourPlace, site: TourPlace) in
            TourFacts(standsIn: true, everywhereOn: true, sample: sample, site: site)
        }
        let tourStates: [(String, Tour.Step, TourFacts, Tour.Milestones)] = [
            ("moon", .moon, facts(away, away), .init()),
            ("rule", .rule, facts(away, away), .init()),
            ("rule-in-front", .rule, facts(inFront, away), .init()),
            ("rule-ruled", .rule, facts(ruledInFront, away), .init()),
            ("rule-returned", .rule, facts(ruled, away), .init(returned: true)),
            ("snooze", .snooze, facts(ruled, away), .init()),
            ("snooze-in-front", .snooze, facts(ruledInFront, away), .init()),
            ("snooze-snoozed", .snooze, facts(snoozed, away), .init()),
            ("snooze-back", .snooze, facts(ruledInFront, away), .init(ruleBack: true)),
            ("snooze-no-rule", .snooze, facts(inFront, away), .init()),
            ("site", .site, facts(away, inFront), .init()),
            ("site-ruled", .site, facts(away, ruledInFront), .init()),
            ("site-left", .site, facts(away, ruled), .init(siteLeft: true)),
            ("peek", .peek, facts(away, away), .init()),
            ("end", .end, facts(away, away), .init()),
        ]
        for (name, appearance) in [("light", NSAppearance.Name.aqua), ("dark", .darkAqua)] {
            NSApp.appearance = NSAppearance(named: appearance)
            for (state, step, facts, milestones) in tourStates {
                let url = URL(fileURLWithPath: folder).appendingPathComponent("app-tour-\(state)-\(name).png")
                guard let png = tour.snapshot(step, facts: facts, milestones: milestones) else {
                    fatalError("couldn't render the tour at \(state)")
                }
                try! png.write(to: url)
            }
        }
        for (name, appearance) in [("light", NSAppearance.Name.aqua), ("dark", .darkAqua)] {
            NSApp.appearance = NSAppearance(named: appearance)
            for (state, hidden) in [("shown", false), ("hidden", true)] {
                let content = PointOut(hidden: hidden, onOK: {}, takeTour: {})
                let backdrop = SnapshotBackdrop(frame: NSRect(origin: .zero, size: content.size))
                content.view.frame = backdrop.bounds
                backdrop.addSubview(content.view)
                backdrop.appearance = NSApp.appearance
                guard let png = backdrop.png() else { fatalError("couldn't render the pointer") }
                try! png.write(
                    to: URL(fileURLWithPath: folder).appendingPathComponent("app-point-out-\(state)-\(name).png"))
            }
        }
        for icon in [MenuIcon.on, .off, .forcedOff] {
            for problems in [false, true] {
                let name = "icon-\(icon)-\(problems ? "problems" : "plain").png"
                try! menuBarIcon(icon, problems: problems)
                    .write(to: URL(fileURLWithPath: folder).appendingPathComponent(name))
            }
        }
        exit(0)
    }

    /// The menu-bar icon at 8x, black on white as a template draws in a light menu bar, with the
    /// dot where the status item lays it over the moon.
    private static func menuBarIcon(_ icon: MenuIcon, problems: Bool) -> Data {
        let image = MenuArt.moon(for: icon)!
        let scale: CGFloat = 8
        let size = NSSize(width: image.size.width * scale + 64, height: image.size.height * scale + 64)
        let bitmap = NSBitmapImageRep(
            bitmapDataPlanes: nil, pixelsWide: Int(size.width), pixelsHigh: Int(size.height),
            bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
            colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
        NSGraphicsContext.saveGraphicsState()
        NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: bitmap)
        NSColor.white.setFill()
        NSRect(origin: .zero, size: size).fill()
        image.draw(in: NSRect(x: 32, y: 32, width: size.width - 64, height: size.height - 64))
        if problems {
            let diameter = MenuArt.dotSize * scale
            let centre = NSPoint(
                x: size.width / 2 + MenuArt.dotOffset.x * scale,
                y: size.height / 2 + MenuArt.dotOffset.y * scale)
            NSColor.systemOrange.setFill()
            NSBezierPath(
                ovalIn: NSRect(
                    x: centre.x - diameter / 2, y: centre.y - diameter / 2, width: diameter, height: diameter)
            ).fill()
        }
        let warning = MenuArt.warning!
        warning.draw(in: NSRect(x: 0, y: 0, width: warning.size.width * 3, height: warning.size.height * 3))
        NSGraphicsContext.restoreGraphicsState()
        return bitmap.representation(using: .png, properties: [:])!
    }
}

extension NSView {
    /// The view as a PNG, laid out and drawn as it stands.
    func png() -> Data? {
        layoutSubtreeIfNeeded()
        displayIfNeeded()
        guard let bitmap = bitmapImageRepForCachingDisplay(in: bounds) else { return nil }
        cacheDisplay(in: bounds, to: bitmap)
        return bitmap.representation(using: .png, properties: [:])
    }
}
#endif
