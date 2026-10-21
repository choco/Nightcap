#if DEBUG
import AppKit
import KeyboardShortcuts
import NightcapCore

/// Dev builds only: with `NIGHTCAP_SNAPSHOT` set to a folder, Nightcap renders the settings
/// window's tabs, onboarding's steps and the tour's panel, light and dark, from the mockups'
/// scenarios into that folder, then quits.
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
                    identity: .app("com.figma.Desktop"), label: "From your config", snoozed: false,
                    action: RuleAction(title: "Snooze", command: .snooze(.app("com.figma.Desktop")))),
                SettingsRule(
                    identity: .domain("figma.com"), label: "Snoozed until 07:12", snoozed: true,
                    action: RuleAction(title: "Unsnooze", command: .unsnooze(.domain("figma.com")))),
                SettingsRule(
                    identity: .app("com.apple.Photos"), label: "Ad-hoc", snoozed: false,
                    action: RuleAction(title: "Remove", command: .removeRule(.app("com.apple.Photos")))),
                SettingsRule(
                    identity: .domain("youtube.com"), label: "Ad-hoc", snoozed: false,
                    action: RuleAction(title: "Remove", command: .removeRule(.domain("youtube.com")))),
            ])
        let window = SettingsWindow(
            current: { settings }, choose: { _ in }, perform: { _ in }, checkForUpdates: {},
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
                rules: [SettingsRule(
                    identity: .app("com.apple.Photos"), label: "From your config", snoozed: false,
                    action: RuleAction(title: "Snooze", command: .snooze(.app("com.apple.Photos"))))],
                sun: Sun(
                    sunset: Calendar.current.date(bySettingHour: 18, minute: 41, second: 0, of: Date())!,
                    sunrise: Calendar.current.date(bySettingHour: 7, minute: 52, second: 0, of: Date())!),
                accessibilityGranted: { false }, askForAccessibility: {}, choose: { _ in },
                onNotificationsRead: {}, onFinish: { _ in })
        }
        let newUser = steps(setUp)
        var alreadySetUp = setUp
        alreadySetUp.alreadySetUp = true
        alreadySetUp.schedule = .sunsetToSunrise
        let nightShiftUser = steps(alreadySetUp)
        let states: [(String, Onboarding, Onboarding.Step, Onboarding.WebsitesPage, Bool)] = [
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
            standsIn: true, shortcut: KeyboardShortcuts.Shortcut(.n, modifiers: [.control, .option, .command]),
            browser: Tour.Browser(bundleId: "com.apple.Safari", name: "Safari"),
            press: {}, release: {}, begin: {}, onEnd: { _ in })
        let sample = Tour.sampleApp
        let tourStates: [(String, Tour.Step, front: String?, ruled: Bool, returned: Bool, siteRuled: Bool, siteLeft: Bool)] = [
            ("moon", .moon, nil, false, false, false, false),
            ("rule", .rule, nil, false, false, false, false),
            ("rule-in-front", .rule, sample, false, false, false, false),
            ("rule-ruled", .rule, sample, true, false, false, false),
            ("rule-returned", .rule, nil, true, true, false, false),
            ("site", .site, "com.apple.Safari", false, false, false, false),
            ("site-ruled", .site, "com.apple.Safari", false, false, true, false),
            ("site-left", .site, nil, false, false, true, true),
            ("peek", .peek, nil, false, false, false, false),
            ("end", .end, nil, false, false, false, false),
        ]
        for (name, appearance) in [("light", NSAppearance.Name.aqua), ("dark", .darkAqua)] {
            NSApp.appearance = NSAppearance(named: appearance)
            for (state, step, front, ruled, returned, siteRuled, siteLeft) in tourStates {
                let url = URL(fileURLWithPath: folder).appendingPathComponent("app-tour-\(state)-\(name).png")
                guard let png = tour.snapshot(
                    step, front: front, ruled: ruled, returned: returned, siteRuled: siteRuled, siteLeft: siteLeft)
                else {
                    fatalError("couldn't render the tour at \(state)")
                }
                try! png.write(to: url)
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
        let image = StatusMenu.image(for: icon)!
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
            let diameter = StatusMenu.dotSize * scale
            let centre = NSPoint(
                x: size.width / 2 + StatusMenu.dotOffset.x * scale,
                y: size.height / 2 + StatusMenu.dotOffset.y * scale)
            NSColor.systemOrange.setFill()
            NSBezierPath(ovalIn: NSRect(
                x: centre.x - diameter / 2, y: centre.y - diameter / 2, width: diameter, height: diameter)).fill()
        }
        let warning = StatusMenu.warning!
        warning.draw(in: NSRect(x: 0, y: 0, width: warning.size.width * 3, height: warning.size.height * 3))
        NSGraphicsContext.restoreGraphicsState()
        return bitmap.representation(using: .png, properties: [:])!
    }
}
#endif
