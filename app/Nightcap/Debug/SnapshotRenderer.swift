#if DEBUG
import AppKit
import KeyboardShortcuts
import NightcapCore

/// Dev builds only: with `NIGHTCAP_SNAPSHOT` set to a folder, which it makes if it's missing,
/// Nightcap renders the settings window's tabs, onboarding's steps, the tour's panel and the
/// pointer to the moon, light and dark, from the mockups' scenarios into that folder, then quits. Settings and onboarding draw
/// cores staged in memory, brought to each scenario by their own code.
enum SnapshotRenderer {
    static func runIfAsked() {
        guard let folder = ProcessInfo.processInfo.environment["NIGHTCAP_SNAPSHOT"] else { return }
        try! FileManager.default.createDirectory(atPath: folder, withIntermediateDirectories: true)
        let folderURL = URL(fileURLWithPath: folder)
        func write(_ name: String, _ png: Data?) {
            guard let png else { fatalError("couldn't render \(name)") }
            try! png.write(to: folderURL.appendingPathComponent(name))
        }
        func inEachAppearance(_ render: (_ appearance: String) -> Void) {
            for (name, appearance) in [("light", NSAppearance.Name.aqua), ("dark", .darkAqua)] {
                NSApp.appearance = NSAppearance(named: appearance)
                render(name)
            }
        }
        let today = { (hour: Int, minute: Int) in
            Calendar.current.date(bySettingHour: hour, minute: minute, second: 0, of: Date())!
        }
        let rules = """
            [rules.off]
            app = ["com.figma.Desktop"]
            domain = ["figma.com"]
            """
        let sunrise = Calendar.current.date(byAdding: .day, value: 1, to: today(7, 12))!
        // A new user's shortcut, whatever the dev build has recorded.
        let shortcut = KeyboardShortcuts.Name.nightShift.initialShortcut!
        // A self-managed release in the evening, whose config broke after its rules loaded: the
        // reload keeps them, beside the check that says why.
        let settingsCore = try! Core.staged(
            scenario: Scenario(
                now: today(21, 0), timeZone: TimeZone.current.identifier, bundleWritable: true, config: rules,
                steps: [
                    .fact(
                        .statusChanged(
                            enabled: true, schedule: .sunsetToSunrise, sunScheduleSetAside: false, writtenAt: nil,
                            switchedBy: .schedule)),
                    .fact(.permissionsChanged(accessibility: false, notifications: .granted)),
                    .fact(.permissionsChanged(accessibility: false, notifications: .denied)),
                    .fact(.loginItemChanged(item: .enabled)),
                    .command(.websites(.on)),
                    .command(.addRule(identity: .app("com.apple.Photos"), says: .off)),
                    .command(.addRule(identity: .app("com.apple.iBooksX"), says: .on)),
                    .command(.set(place: .domain("figma.com"), state: .on, length: .until(sunrise))),
                    .command(.set(place: .app("com.apple.mail"), state: .off, length: .until(today(22, 0)))),
                    .config(rules + "\ndomian = [\"figma.com\"]"),
                ]))
        // A refused command fails a scenario's step, so the reload comes after.
        settingsCore.send(command: .reload)
        // An empty folder, never filled, so sites show the globe whatever this Mac has fetched.
        let icons = SiteIcons(
            directory: FileManager.default.temporaryDirectory.appending(path: UUID().uuidString), online: false)
        let window = SettingsWindow(
            current: { settingsCore.snapshot() }, icons: icons, choose: { _ in },
            perform: { _ in },
            checkForUpdates: {}, export: { _ in "" }, takeTour: {}, onClose: {})
        inEachAppearance { appearance in
            for tab in SettingsWindow.Tab.allCases {
                write("app-\(tab.rawValue)-\(appearance).png", window.snapshot(tab, shortcut: shortcut))
            }
        }
        // A self-managed release that doesn't open at login yet, with Photos' rule in its config, at
        // 21:42, the evening the stage's mocks show.
        let evening = today(21, 42)
        let steps = { (schedule: Schedule) in
            let core = try! Core.staged(
                scenario: Scenario(
                    now: evening, timeZone: TimeZone.current.identifier, bundleWritable: true,
                    config: "[rules.off]\napp = [\"com.apple.Photos\"]",
                    steps: [
                        .fact(
                            .statusChanged(
                                enabled: false, schedule: schedule, sunScheduleSetAside: false, writtenAt: nil,
                                switchedBy: .schedule)),
                        .fact(.strengthChanged(value: 60)),
                        .fact(.loginItemChanged(item: .notRegistered)),
                    ]))
            return Onboarding(
                core.snapshot(),
                icons: icons,
                sun: Sun(sunset: today(18, 41), sunrise: today(7, 52)),
                now: { evening },
                shortcut: shortcut,
                askForAccessibility: {}, choose: { _ in }, askForNotifications: {}, onFinish: { _ in })
        }
        let newUser = steps(.off)
        let nightShiftUser = steps(.sunsetToSunrise)
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
        inEachAppearance { appearance in
            for (state, onboarding, step, page, reviewing) in states {
                write(
                    "app-onboarding-\(state)-\(appearance).png",
                    onboarding.snapshot(step, page: page, reviewing: reviewing))
            }
        }
        let tour = Tour(
            shortcut: shortcut,
            browser: "Safari",
            press: {}, release: {}, begin: { _ in }, onEnd: { _ in })
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
        inEachAppearance { appearance in
            for (state, step, facts, milestones) in tourStates {
                write("app-tour-\(state)-\(appearance).png", tour.snapshot(step, facts: facts, milestones: milestones))
            }
        }
        inEachAppearance { appearance in
            for (state, hidden) in [("shown", false), ("hidden", true)] {
                let content = PointOut(hidden: hidden, onOK: {}, takeTour: {})
                write(
                    "app-point-out-\(state)-\(appearance).png",
                    SnapshotBackdrop.png(of: content.view, size: content.size))
            }
        }
        for icon in [Menu.Moon.on, .off, .forcedOff] {
            for problems in [false, true] {
                write("icon-\(icon)-\(problems ? "problems" : "plain").png", menuBarIcon(icon, problems: problems))
            }
        }
        exit(0)
    }

    /// The menu-bar icon at 8x, black on white as a template draws in a light menu bar, with the
    /// dot where the status item lays it over the moon.
    private static func menuBarIcon(_ icon: Menu.Moon, problems: Bool) -> Data {
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

/// The menu's colour, opaque, behind a snapshot of a panel, which the panel's glass would show.
final class SnapshotBackdrop: NSView {
    /// `content` at `size` on the backdrop, in the app's appearance, as a PNG.
    static func png(of content: NSView, size: NSSize) -> Data? {
        let backdrop = SnapshotBackdrop(frame: NSRect(origin: .zero, size: size))
        content.frame = backdrop.bounds
        backdrop.addSubview(content)
        backdrop.appearance = NSApp.appearance
        return backdrop.png()
    }

    override func draw(_ dirtyRect: NSRect) {
        effectiveAppearance.performAsCurrentDrawingAppearance {
            Paint.menu.withAlphaComponent(1).setFill()
            bounds.fill()
        }
    }
}
#endif
