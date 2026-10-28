import AppKit
import KeyboardShortcuts
import NightcapCore

enum StageArt {
    private static var words: Wording { .system }

    /// The scene's view, sized to its content. `strength` warms the mocks of a screen, the timeline
    /// marks `now`, and the menu shows `shortcut`.
    static func view(
        for scene: Scene, strength: UInt8, sun: Sun?, now: Date, shortcut: KeyboardShortcuts.Shortcut?
    ) -> NSView {
        switch scene {
        case .strength: StrengthArt(strength: strength)
        case .timeline(let schedule): timeline(schedule, sun: sun, now: now)
        case .byHand: byHand()
        case .apps: apps(strength: strength)
        case .address: address()
        case .accessibility(let granted): accessibility(granted: granted)
        case .sites: sites(strength: strength)
        case .lastThings(let app): lastThings(app: app, strength: strength)
        case .ready: ready(shortcut: shortcut)
        }
    }

    // MARK: The scenes

    /// The Night Shift steps' 24-hour bar: when it's on, with a marker for now.
    private static func timeline(_ schedule: Schedule, sun: Sun?, now date: Date) -> NSView {
        let (start, end, sunset): (TimeOfDay, TimeOfDay, Bool) =
            switch schedule {
            case .custom(let from, let to): (from, to, false)
            default:
                (
                    sun.map { time(of: $0.sunset) } ?? TimeOfDay(hour: 19, minute: 0),
                    sun.map { time(of: $0.sunrise) } ?? TimeOfDay(hour: 7, minute: 0), true
                )
            }
        let now = time(of: date)
        let on = covers(start, end, now)
        let (from, to) = (words.time(start), words.time(end))
        let caption = words.timeline(onNow: on, from: start, to: end, followsLocation: sunset)
        let hours = [0, 6, 12, 18].map { words.time(TimeOfDay(hour: $0, minute: 0)) }
        let nowLabel = words.timelineNow
        return Drawn(NSSize(width: 600, height: 112)) { bounds in
            let width = bounds.width
            let x = { (time: TimeOfDay) in width * CGFloat(Int(time.hour) * 60 + Int(time.minute)) / 1440 }
            let label = NSFont.systemFont(ofSize: 11)
            // Each time sits just right of its mark, with its glyph just left of it, whatever
            // the clock makes its width.
            let glyph = { (time: TimeOfDay, text: String) in
                NSPoint(x: x(time) + 6 - Paint.width(text, label) / 2 - 9, y: 8)
            }
            Paint.glyph("moon.fill", 9, .labelColor, centredAt: glyph(start, from))
            Paint.text(from, label, .labelColor, centredAt: NSPoint(x: x(start) + 6, y: 8))
            Paint.glyph("sun.max.fill", 9, .labelColor, centredAt: glyph(end, to))
            Paint.text(to, label, .labelColor, centredAt: NSPoint(x: x(end) + 6, y: 8))
            let bar = NSRect(x: 0, y: 24, width: width, height: 40)
            Paint.lifted { Paint.rounded(bar, 9, .nightcapGroup) }
            NSGraphicsContext.saveGraphicsState()
            NSBezierPath(roundedRect: bar, xRadius: 9, yRadius: 9).addClip()
            let band = Paint.warm(0.45)
            if x(start) > x(end) {
                band.setFill()
                NSRect(x: x(start), y: bar.minY, width: width - x(start), height: bar.height).fill()
                NSRect(x: 0, y: bar.minY, width: x(end), height: bar.height).fill()
            } else {
                band.setFill()
                NSRect(x: x(start), y: bar.minY, width: x(end) - x(start), height: bar.height).fill()
            }
            NSColor.nightcapSeparator.setFill()
            for quarter in 1...3 {
                NSRect(x: width * CGFloat(quarter) / 4, y: bar.minY, width: 0.5, height: bar.height).fill()
            }
            NSColor.controlAccentColor.setFill()
            NSRect(x: x(now) - 1, y: bar.minY, width: 2, height: bar.height).fill()
            NSGraphicsContext.restoreGraphicsState()
            let small = NSFont.monospacedDigitSystemFont(ofSize: 10, weight: .regular)
            let nowFont = NSFont.systemFont(ofSize: 10, weight: .semibold)
            let nowHalf = (nowLabel as NSString).size(withAttributes: [.font: nowFont]).width / 2 + 4
            // An hour's label gives way to "Now" where the two would overlap.
            for (index, hour) in hours.enumerated() {
                let half = (hour as NSString).size(withAttributes: [.font: small]).width / 2
                // 00:00 sits at the bar's left edge rather than centred on it.
                let centre = index == 0 ? half : width * CGFloat(index) / 4
                guard abs(centre - x(now)) > half + nowHalf else { continue }
                Paint.text(hour, small, .nightcapSecondary, centredAt: NSPoint(x: centre, y: 79))
            }
            Paint.text(nowLabel, nowFont, .controlAccentColor, centredAt: NSPoint(x: x(now), y: 79))
            Paint.text(caption, .systemFont(ofSize: 12), .labelColor, centredAt: NSPoint(x: width / 2, y: 104))
        }
    }

    /// "By hand": Control Center's Display module, with Night Shift's button.
    private static func byHand() -> NSView {
        let card = NSRect(x: 0, y: 0, width: 320, height: 104)
        let pill = NSRect(x: 14, y: 66, width: 104, height: 24)
        let view = Drawn(NSSize(width: 320, height: 132)) { _ in
            Paint.lifted { Paint.rounded(card, 16, Paint.menu) }
            Paint.text(
                words.display, .systemFont(ofSize: 12, weight: .semibold), .labelColor, at: NSPoint(x: 14, y: 12))
            let slider = NSRect(x: 14, y: 34, width: 292, height: 20)
            Paint.rounded(slider, 10, .nightcapTile)
            Paint.rounded(
                NSRect(x: slider.minX, y: slider.minY, width: slider.width * 0.68, height: slider.height), 10,
                .nightcapControl)
            Paint.rounded(pill, 12, .controlAccentColor)
            Paint.glyph("moon.fill", 10, .white, centredAt: NSPoint(x: pill.minX + 16, y: pill.midY))
            Paint.text(
                words.nightShift, .systemFont(ofSize: 12), .white, at: NSPoint(x: pill.minX + 26, y: pill.minY + 4))
            for (index, name) in [words.trueTone, words.darkMode].enumerated() {
                let other = NSRect(x: 128 + CGFloat(index) * 92, y: pill.minY, width: 84, height: 24)
                Paint.rounded(other, 12, .nightcapTile)
                Paint.text(
                    name, .systemFont(ofSize: 12), .nightcapSecondary, centredAt: NSPoint(x: other.midX, y: other.midY))
            }
            Paint.text(
                words.byHandCaption, .systemFont(ofSize: 12), .labelColor, centredAt: NSPoint(x: card.midX, y: 124))
        }
        view.addSubview(Ring(around: pill.insetBy(dx: -3, dy: -3), radius: 15))
        return view
    }

    /// Mail in front with Night Shift on, then Figma, in true colour, with the moon's "z".
    private static func apps(strength: UInt8) -> NSView {
        let screen = MiniScreen(NSSize(width: 360, height: 150), app: "Mail", otherApp: "Figma", forcesOff: true)
        screen.add(Drawn(NSSize(width: 196, height: 112)) { Paint.mail($0) }, at: NSPoint(x: 20, y: 24))
        let figma = Drawn(NSSize(width: 212, height: 108)) { Paint.figma($0) }
        screen.add(figma, at: NSPoint(x: 128, y: 30))
        Loop.middle(figma)
        screen.warm(strength)
        return captioned(screen, words.mailInFront, words.figmaInFront)
    }

    /// The address bar, fading to the one part Nightcap keeps.
    private static func address() -> NSView {
        let mono = NSFont.monospacedSystemFont(ofSize: 11, weight: .regular)
        let parts = ["https://www.", "youtube.com", "/watch?v=8xJ3kQ&t=42s"]
        let widths = parts.map { Paint.width($0, mono) }
        let field = NSRect(x: 60, y: 6, width: 410, height: 22)
        let textX = field.minX + 26
        let view = Drawn(NSSize(width: 480, height: 158)) { _ in
            let card = NSRect(x: 0, y: 0, width: 480, height: 130)
            Paint.lifted { Paint.rounded(card, 9, .nightcapGroup) }
            NSGraphicsContext.saveGraphicsState()
            NSBezierPath(roundedRect: card, xRadius: 9, yRadius: 9).addClip()
            Paint.chrome.setFill()
            NSRect(x: 0, y: 0, width: 480, height: 34).fill()
            NSColor.nightcapWindow.setFill()
            NSRect(x: 0, y: 34, width: 480, height: 96).fill()
            NSGraphicsContext.restoreGraphicsState()
            Paint.trafficLights(at: NSPoint(x: 10, y: 13), size: 8)
            Paint.rounded(field, 6, .nightcapGroup)
            Paint.glyph("lock.fill", 9, .nightcapSecondary, centredAt: NSPoint(x: field.minX + 13, y: field.midY))
            Paint.text(parts[1], mono, .labelColor, at: NSPoint(x: textX + widths[0], y: field.minY + 4))
            Paint.picture(NSRect(x: 16, y: 46, width: 150, height: 72), dark: true)
            for (y, width) in [(50.0, 180.0), (66, 140), (80, 160)] {
                Paint.rounded(NSRect(x: 180, y: y, width: width, height: 6), 3, .nightcapTile)
            }
            Paint.text(
                words.addressCaption, .systemFont(ofSize: 12), .labelColor, centredAt: NSPoint(x: 240, y: 148))
        }
        let highlight = Drawn(NSSize(width: widths[1] + 6, height: 16)) { bounds in
            Paint.rounded(bounds, 4, NSColor.controlAccentColor.withAlphaComponent(0.18))
        }
        highlight.frame.origin = NSPoint(x: textX + widths[0] - 3, y: field.minY + 3)
        view.addSubview(highlight, positioned: .below, relativeTo: nil)
        Loop.fades(highlight, values: [0, 0, 1, 1, 0], times: [0, 0.25, 0.4, 0.85, 1], duration: 5)
        for (index, part) in [(0, parts[0]), (2, parts[2])] {
            let piece = Drawn(NSSize(width: widths[index] + 2, height: 16)) { _ in
                Paint.text(part, mono, .labelColor, at: NSPoint(x: 0, y: 1))
            }
            piece.frame.origin = NSPoint(
                x: textX + (index == 0 ? 0 : widths[0] + widths[1]), y: field.minY + 3)
            view.addSubview(piece)
            Loop.fades(piece, values: [1, 1, 0.15, 0.15, 1], times: [0, 0.25, 0.4, 0.85, 1], duration: 5)
        }
        return view
    }

    /// System Settings at Accessibility, Nightcap's switch on once it's allowed.
    private static func accessibility(granted: Bool) -> NSView {
        Drawn(NSSize(width: 480, height: 164)) { _ in
            let card = NSRect(x: 0, y: 0, width: 480, height: 164)
            Paint.lifted { Paint.rounded(card, 10, .nightcapGroup) }
            NSGraphicsContext.saveGraphicsState()
            NSBezierPath(roundedRect: card, xRadius: 10, yRadius: 10).addClip()
            NSColor.nightcapWindow.setFill()
            NSRect(x: 0, y: 0, width: 136, height: 164).fill()
            NSColor.nightcapSeparator.setFill()
            NSRect(x: 136, y: 0, width: 0.5, height: 164).fill()
            NSGraphicsContext.restoreGraphicsState()
            Paint.trafficLights(at: NSPoint(x: 12, y: 10), size: 7)
            for (y, width) in [(30.0, 84.0), (44, 66)] {
                Paint.rounded(NSRect(x: 16, y: y, width: width, height: 8), 4, .nightcapTile)
            }
            let selected = NSRect(x: 10, y: 58, width: 116, height: 18)
            Paint.rounded(selected, 5, .controlAccentColor)
            Paint.text(words.privacyAndSecurity, .systemFont(ofSize: 10), .white, at: NSPoint(x: 16, y: 60))
            Paint.rounded(NSRect(x: 16, y: 84, width: 74, height: 8), 4, .nightcapTile)
            Paint.text(
                words.backToAccessibility, .systemFont(ofSize: 12, weight: .semibold), .labelColor,
                at: NSPoint(x: 152, y: 12))
            let list = NSRect(x: 152, y: 38, width: 312, height: 74)
            Paint.rounded(list, 7, .clear, stroke: .nightcapSeparator)
            NSApp.applicationIconImage.draw(in: NSRect(x: list.minX + 6, y: list.minY + 6, width: 26, height: 26))
            Paint.text(
                "Nightcap", .systemFont(ofSize: 12), .labelColor, at: NSPoint(x: list.minX + 38, y: list.minY + 11))
            Paint.toggle(NSRect(x: list.maxX - 40, y: list.minY + 11, width: 28, height: 16), on: granted)
            NSColor.nightcapSeparator.setFill()
            NSRect(x: list.minX, y: list.midY, width: list.width, height: 0.5).fill()
            Paint.rounded(NSRect(x: list.minX + 9, y: list.midY + 8, width: 20, height: 20), 5, .nightcapTile)
            Paint.rounded(NSRect(x: list.minX + 38, y: list.midY + 15, width: 90, height: 7), 3.5, .nightcapTile)
            Paint.toggle(NSRect(x: list.maxX - 40, y: list.midY + 10, width: 28, height: 16), on: true, faded: true)
        }
    }

    /// Safari with an ordinary tab in front, then YouTube, in true colour.
    private static func sites(strength: UInt8) -> NSView {
        let screen = MiniScreen(NSSize(width: 360, height: 150), app: "Safari", otherApp: nil, forcesOff: true)
        let browser = Drawn(NSSize(width: 320, height: 120)) { bounds in
            Paint.browser(bounds, tab: words.inbox, address: "mail.example.com", video: false)
        }
        let youtube = Drawn(NSSize(width: 320, height: 120)) { bounds in
            Paint.browser(bounds, tab: "YouTube", address: "youtube.com", video: true)
        }
        screen.add(browser, at: NSPoint(x: 20, y: 22))
        screen.add(youtube, at: NSPoint(x: 20, y: 22))
        Loop.middle(youtube)
        screen.warm(strength)
        return captioned(screen, words.anyOtherSite, words.youtubeInFront)
    }

    /// The notification after overruling a rule, and the moon in the menu bar at login.
    private static func lastThings(app: String, strength: UInt8) -> NSView {
        let screen = MiniScreen(NSSize(width: 300, height: 140), app: app, otherApp: nil, forcesOff: false)
        screen.add(Drawn(NSSize(width: 160, height: 102)) { Paint.figma($0) }, at: NSPoint(x: 16, y: 26))
        screen.warm(strength, loops: false)
        let notice = words.snoozedNotice(app)
        let banner = Drawn(NSSize(width: 226, height: 46)) { bounds in
            Paint.lifted { Paint.rounded(bounds, 12, NSColor(white: 0.97, alpha: 0.97)) }
            NSApp.applicationIconImage.draw(in: NSRect(x: 6, y: 9, width: 28, height: 28))
            Paint.text(
                notice.title, .systemFont(ofSize: 9, weight: .semibold), NSColor(white: 0.11, alpha: 1),
                at: NSPoint(x: 38, y: 9))
            Paint.text(notice.body, .systemFont(ofSize: 9), NSColor(white: 0.11, alpha: 1), at: NSPoint(x: 38, y: 23))
        }
        screen.add(banner, at: NSPoint(x: 66, y: 22))
        Loop.fades(banner, values: [0, 0, 1, 1, 0, 0], times: [0, 0.12, 0.22, 0.8, 0.92, 1], duration: 5)
        let left = captioned(screen, words.overruleARule, nil)
        let font = NSFont.systemFont(ofSize: 12)
        // The clock ends inside the bar, as far from its edge as Finder is, whatever the user's
        // clock makes its width, and the moon sits just left of it.
        let clock = min(246, 300 - 14 - Paint.width(words.loginClock, font))
        let moon = NSRect(x: clock - 6 - 26, y: 4, width: 26, height: 22)
        let bar = Drawn(NSSize(width: 300, height: 56)) { _ in
            Paint.lifted { Paint.rounded(NSRect(x: 0, y: 0, width: 300, height: 30), 8, Paint.bar) }
            Paint.text("Finder", .systemFont(ofSize: 12, weight: .bold), .labelColor, at: NSPoint(x: 14, y: 7))
            Paint.text(words.file, font, .labelColor, at: NSPoint(x: 66, y: 7))
            Paint.text(words.edit, font, .labelColor, at: NSPoint(x: 98, y: 7))
            Paint.moon(.off, .labelColor, in: NSRect(x: moon.midX - 7, y: moon.midY - 7, width: 14, height: 14))
            Paint.text(words.loginClock, font, .labelColor, at: NSPoint(x: clock, y: 7))
            Paint.text(
                words.loginCaption, .systemFont(ofSize: 11), .nightcapSecondary, centredAt: NSPoint(x: 150, y: 48))
        }
        bar.addSubview(Ring(around: moon, radius: 5))
        return side(by: left, bar)
    }

    /// The menu at 21:42, as it shows with Mail in front and Night Shift on by sunset to sunrise
    /// until 07:00: the strength, then everywhere's row and Mail's, each turning it off. The stage
    /// leaves each row's submenu closed, so a row holds only its first item, which marks it as
    /// having one.
    private static var readyMenu: Menu {
        let calendar = Calendar.current
        let evening = calendar.date(bySettingHour: 21, minute: 42, second: 0, of: Date())!
        let tomorrow = calendar.date(byAdding: .day, value: 1, to: evening)!
        let sunrise = calendar.date(bySettingHour: 7, minute: 0, second: 0, of: tomorrow)!
        let length: SignedDuration = 30 * 60
        let row = { (place: Place, title: String) in
            let first = Menu.Choice(
                title: words.forLength(length), command: .set(place: place, state: .off, length: .for(length)))
            return Menu.Entry.row(
                Menu.PlaceRow(place: place, title: title, site: nil, subtitle: nil, items: [.choice(first)]))
        }
        return Menu(
            moon: .on, problemLine: nil,
            nightShift: Menu.NightShiftItem(on: true, line: words.followingTheSchedule(until: sunrise), command: .off),
            entries: [
                .strength(60), row(.everywhere, words.turn(.off)), .separator,
                row(.app("com.apple.mail"), words.turn(.off, at: .app(name: "Mail"))),
            ])
    }

    /// Nightcap's menu, open under the moon, as the app draws it on macOS 27: the Night Shift
    /// item, the strength between the menu's suns, everywhere's row, then the front app's
    /// (ADR-0018). It stops there: Settings and Quit, under them, don't fit the stage.
    private static func ready(shortcut: KeyboardShortcuts.Shortcut?) -> NSView {
        let menu = readyMenu
        let reason = menu.nightShift.line.map { MenuArt.wrapped($0).split(separator: "\n").map(String.init) } ?? []
        // Down the menu: the Night Shift item, whose lines set its height, then each entry, a
        // separator 9 points high and anything else 21, then the menu's bottom edge.
        let top = 23.5 + 12 * CGFloat(reason.count)
        let heights = menu.entries.map { entry -> CGFloat in if case .separator = entry { 9 } else { 21 } }
        // A status item's menu opens under it, to the right, unless that would leave the screen,
        // as it would this close to the edge: then it stays inside.
        let box = NSRect(x: 460 - 4 - 212, y: 36, width: 212, height: top + heights.reduce(0, +) + 2.5)
        let font = NSFont.systemFont(ofSize: 12)
        // The clock ends inside the bar, as far from its edge as Mail is, whatever the user's clock
        // makes its width, and the moon sits left of it.
        let clock = min(410, 460 - 16 - Paint.width(words.eveningClock, font))
        return Drawn(NSSize(width: 460, height: box.maxY + 6)) { _ in
            Paint.lifted { Paint.rounded(NSRect(x: 0, y: 0, width: 460, height: 30), 8, Paint.bar) }
            Paint.text("Mail", .systemFont(ofSize: 12, weight: .bold), .labelColor, at: NSPoint(x: 16, y: 7))
            Paint.text(words.file, font, .labelColor, at: NSPoint(x: 56, y: 7))
            Paint.text(words.edit, font, .labelColor, at: NSPoint(x: 88, y: 7))
            let moon = NSRect(x: clock - 38, y: 5, width: 26, height: 20)
            Paint.rounded(moon, 10, Paint.highlight)
            Paint.moon(menu.moon, .labelColor, in: NSRect(x: moon.midX - 7, y: moon.midY - 7, width: 14, height: 14))
            Paint.text(words.eveningClock, font, .labelColor, at: NSPoint(x: clock, y: 7))
            Paint.lifted { Paint.rounded(box, 10, Paint.menu) }
            let item = NSFont.systemFont(ofSize: 11)
            // The menu's columns: the checkmark, the item's image, and the title.
            let check = box.minX + 10
            let image = box.minX + 25
            let title = box.minX + 38
            // `text` from `x`, its middle `y` down the menu.
            let line = { (text: String, font: NSFont, colour: NSColor, x: CGFloat, y: CGFloat) in
                let height = (text as NSString).size(withAttributes: [.font: font]).height
                Paint.text(text, font, colour, at: NSPoint(x: x, y: box.minY + y - height / 2))
            }
            // The Night Shift item: the check, the moon and the shortcut sit across its lines.
            if menu.nightShift.on {
                Paint.glyph("checkmark", 9, .labelColor, centredAt: NSPoint(x: check, y: box.minY + 25))
            }
            Paint.moon(menu.moon, .labelColor, in: NSRect(x: image - 6.5, y: box.minY + 18.5, width: 13, height: 13))
            line(Wording.system.nightShift, item, .labelColor, title, 13.5)
            for (index, text) in reason.enumerated() {
                line(text, .systemFont(ofSize: 10), .nightcapSecondary, title, 26 + CGFloat(index) * 12)
            }
            if let keys = shortcut?.description {
                line(keys, item, .nightcapSecondary, box.maxX - 15 - Paint.width(keys, item), 25)
            }
            var y = top
            for (entry, height) in zip(menu.entries, heights) {
                let middle = y + height / 2
                y += height
                switch entry {
                case .strength(let value):
                    strength(value, in: box, at: box.minY + middle, from: title)
                case .row(let row):
                    let centre = NSPoint(x: image, y: box.minY + middle)
                    if let symbol = MenuArt.symbol(for: row.place) {
                        Paint.glyph(symbol, 10, .labelColor, centredAt: centre)
                    } else if case .app(let bundleId) = row.place,
                        let app = NSWorkspace.shared.urlForApplication(withBundleIdentifier: bundleId)
                    {
                        Apps.icon(at: app, side: 14).draw(
                            in: NSRect(x: centre.x - 7, y: centre.y - 7, width: 14, height: 14), from: .zero,
                            operation: .sourceOver, fraction: 1, respectFlipped: true, hints: nil)
                    }
                    line(row.title, item, .labelColor, title, middle)
                    if !row.items.isEmpty {
                        Paint.glyph(
                            "chevron.right", 9, .labelColor, centredAt: NSPoint(x: box.maxX - 20, y: box.minY + middle))
                    }
                case .separator:
                    NSColor.nightcapSeparator.setFill()
                    NSRect(x: box.minX + 10, y: box.minY + middle, width: box.width - 20, height: 0.5).fill()
                }
            }
        }
    }

    /// The strength in a menu `box`, as a slider in a menu draws: grey, with a white knob, between
    /// the suns, the small one from `x`.
    private static func strength(_ value: UInt8, in box: NSRect, at y: CGFloat, from x: CGFloat) {
        Paint.template(MenuArt.sun(large: false), .nightcapSecondary, in: NSRect(x: x, y: y - 4.5, width: 9, height: 9))
        Paint.template(
            MenuArt.sun(large: true), .nightcapSecondary,
            in: NSRect(x: box.maxX - 26, y: y - 6.5, width: 13, height: 13))
        let track = NSRect(x: box.minX + 52, y: y - 1.5, width: box.width - 83, height: 3)
        let knob = track.minX + 7.5 + (track.width - 15) * (CGFloat(value) / 100)
        Paint.rounded(track, 1.5, NSColor.labelColor.withAlphaComponent(0.15))
        Paint.rounded(
            NSRect(x: track.minX, y: track.minY, width: knob - track.minX, height: 3), 1.5,
            NSColor.labelColor.withAlphaComponent(0.45))
        Paint.rounded(
            NSRect(x: knob - 7.5, y: y - 5.5, width: 15, height: 11), 5.5, .white, stroke: .nightcapControlBorder)
    }

    // MARK: Pieces

    /// `view` with a caption under it; with two captions, they take turns as the loop does.
    private static func captioned(_ view: NSView, _ first: String, _ second: String?) -> NSView {
        let size = NSSize(width: view.frame.width, height: view.frame.height + 22)
        let column = Drawn(size) { _ in }
        column.addSubview(view)
        view.frame.origin = .zero
        let font = NSFont.systemFont(ofSize: 11)
        let captionA = Drawn(NSSize(width: size.width, height: 16)) { bounds in
            Paint.text(first, font, .nightcapSecondary, centredAt: NSPoint(x: bounds.midX, y: 8))
        }
        captionA.frame.origin = NSPoint(x: 0, y: view.frame.height + 6)
        column.addSubview(captionA)
        if let second {
            let captionB = Drawn(NSSize(width: size.width, height: 16)) { bounds in
                Paint.text(second, font, .nightcapSecondary, centredAt: NSPoint(x: bounds.midX, y: 8))
            }
            captionB.frame.origin = captionA.frame.origin
            column.addSubview(captionB)
            Loop.middle(captionB)
            Loop.ends(captionA)
        }
        return column
    }

    private static func side(by left: NSView, _ right: NSView) -> NSView {
        let gap: CGFloat = 40
        let height = max(left.frame.height, right.frame.height)
        let row = Drawn(NSSize(width: left.frame.width + gap + right.frame.width, height: height)) { _ in }
        left.frame.origin = NSPoint(x: 0, y: (height - left.frame.height) / 2)
        right.frame.origin = NSPoint(x: left.frame.width + gap, y: (height - right.frame.height) / 2)
        row.addSubview(left)
        row.addSubview(right)
        return row
    }

    private static func time(of date: Date) -> TimeOfDay {
        let parts = Calendar.current.dateComponents([.hour, .minute], from: date)
        return TimeOfDay(hour: UInt8(parts.hour!), minute: UInt8(parts.minute!))
    }

    private static func covers(_ start: TimeOfDay, _ end: TimeOfDay, _ now: TimeOfDay) -> Bool {
        let minutes = { (time: TimeOfDay) in Int(time.hour) * 60 + Int(time.minute) }
        let (from, to, at) = (minutes(start), minutes(end), minutes(now))
        return from <= to ? (from <= at && at < to) : (at >= from || at < to)
    }
}
