import AppKit
import KeyboardShortcuts
import NightcapCore

/// What onboarding's stage shows for a step: a small live mock of macOS that acts out what the
/// step describes (design/README.md, "In the app").
enum Scene: Equatable {
    case strength
    case timeline(Schedule)
    case byHand
    case apps
    case address
    case accessibility(granted: Bool)
    case sites
    case lastThings(app: String)
    case ready
}

/// Today's sunset and sunrise, as CoreBrightness schedules by them, for the timeline.
struct Sun: Equatable {
    let sunset: Date
    let sunrise: Date
}

enum StageArt {
    /// The scene's view, sized to its content. `strength` warms the mocks of a screen.
    static func view(for scene: Scene, strength: UInt8, sun: Sun?) -> NSView {
        switch scene {
        case .strength: StrengthArt(strength: strength)
        case .timeline(let schedule): timeline(schedule, sun: sun)
        case .byHand: byHand()
        case .apps: apps(strength: strength)
        case .address: address()
        case .accessibility(let granted): accessibility(granted: granted)
        case .sites: sites(strength: strength)
        case .lastThings(let app): lastThings(app: app, strength: strength)
        case .ready: ready()
        }
    }

    // MARK: The scenes

    /// The Night Shift steps' 24-hour bar: when it's on, with a marker for now.
    private static func timeline(_ schedule: Schedule, sun: Sun?) -> NSView {
        let (start, end, sunset): (TimeOfDay, TimeOfDay, Bool) =
            switch schedule {
            case .custom(let from, let to): (from, to, false)
            default: (sun.map { time(of: $0.sunset) } ?? TimeOfDay(hour: 19, minute: 0),
                      sun.map { time(of: $0.sunrise) } ?? TimeOfDay(hour: 7, minute: 0), true)
            }
        let now = time(of: Date())
        let on = covers(start, end, now)
        let caption =
            (on ? "On now, until \(clock(end))." : "Comes on at \(clock(start)), and goes off at \(clock(end)).")
            + (sunset ? " The times follow your location." : "")
        return Drawn(NSSize(width: 600, height: 112)) { bounds in
            let width = bounds.width
            let x = { (time: TimeOfDay) in width * CGFloat(Int(time.hour) * 60 + Int(time.minute)) / 1440 }
            let label = NSFont.systemFont(ofSize: 11)
            Paint.glyph("moon.fill", 9, .labelColor, centredAt: NSPoint(x: x(start) - 18, y: 8))
            Paint.text(clock(start), label, .labelColor, centredAt: NSPoint(x: x(start) + 6, y: 8))
            Paint.glyph("sun.max.fill", 9, .labelColor, centredAt: NSPoint(x: x(end) - 18, y: 8))
            Paint.text(clock(end), label, .labelColor, centredAt: NSPoint(x: x(end) + 6, y: 8))
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
            let nowHalf = ("Now" as NSString).size(withAttributes: [.font: nowFont]).width / 2 + 4
            // An hour's label gives way to "Now" where the two would overlap.
            for (index, hour) in ["00:00", "06:00", "12:00", "18:00"].enumerated() {
                let half = (hour as NSString).size(withAttributes: [.font: small]).width / 2
                // 00:00 sits at the bar's left edge rather than centred on it.
                let centre = index == 0 ? half : width * CGFloat(index) / 4
                guard abs(centre - x(now)) > half + nowHalf else { continue }
                Paint.text(hour, small, .nightcapSecondary, centredAt: NSPoint(x: centre, y: 79))
            }
            Paint.text("Now", nowFont, .controlAccentColor, centredAt: NSPoint(x: x(now), y: 79))
            Paint.text(caption, .systemFont(ofSize: 12), .labelColor, centredAt: NSPoint(x: width / 2, y: 104))
        }
    }

    /// "By hand": Control Center's Display module, with Night Shift's button.
    private static func byHand() -> NSView {
        let card = NSRect(x: 0, y: 0, width: 320, height: 104)
        let pill = NSRect(x: 14, y: 66, width: 104, height: 24)
        let view = Drawn(NSSize(width: 320, height: 132)) { _ in
            Paint.lifted { Paint.rounded(card, 16, Paint.menu) }
            Paint.text("Display", .systemFont(ofSize: 12, weight: .semibold), .labelColor, at: NSPoint(x: 14, y: 12))
            let slider = NSRect(x: 14, y: 34, width: 292, height: 20)
            Paint.rounded(slider, 10, .nightcapTile)
            Paint.rounded(NSRect(x: slider.minX, y: slider.minY, width: slider.width * 0.68, height: slider.height), 10, .nightcapControl)
            Paint.rounded(pill, 12, .controlAccentColor)
            Paint.glyph("moon.fill", 10, .white, centredAt: NSPoint(x: pill.minX + 16, y: pill.midY))
            Paint.text("Night Shift", .systemFont(ofSize: 12), .white, at: NSPoint(x: pill.minX + 26, y: pill.minY + 4))
            for (index, name) in ["True Tone", "Dark Mode"].enumerated() {
                let other = NSRect(x: 128 + CGFloat(index) * 92, y: pill.minY, width: 84, height: 24)
                Paint.rounded(other, 12, .nightcapTile)
                Paint.text(name, .systemFont(ofSize: 12), .nightcapSecondary, centredAt: NSPoint(x: other.midX, y: other.midY))
            }
            Paint.text(
                "Control Center › Display, or the top of Nightcap’s menu", .systemFont(ofSize: 12), .labelColor,
                centredAt: NSPoint(x: card.midX, y: 124))
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
        return captioned(
            screen, "Mail in front: Night Shift on", "Figma in front: true colour, and the moon gets a “z”")
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
                "Of the whole address, Nightcap keeps only the site’s name.", .systemFont(ofSize: 12), .labelColor,
                centredAt: NSPoint(x: 240, y: 148))
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
            Paint.text("Privacy & Security", .systemFont(ofSize: 10), .white, at: NSPoint(x: 16, y: 60))
            Paint.rounded(NSRect(x: 16, y: 84, width: 74, height: 8), 4, .nightcapTile)
            Paint.text("‹ Accessibility", .systemFont(ofSize: 12, weight: .semibold), .labelColor, at: NSPoint(x: 152, y: 12))
            let list = NSRect(x: 152, y: 38, width: 312, height: 74)
            Paint.rounded(list, 7, .clear, stroke: .nightcapSeparator)
            NSApp.applicationIconImage.draw(in: NSRect(x: list.minX + 6, y: list.minY + 6, width: 26, height: 26))
            Paint.text("Nightcap", .systemFont(ofSize: 12), .labelColor, at: NSPoint(x: list.minX + 38, y: list.minY + 11))
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
        let browser = Drawn(NSSize(width: 320, height: 120)) { bounds in Paint.browser(bounds, tab: "Inbox", address: "mail.example.com", video: false) }
        let youtube = Drawn(NSSize(width: 320, height: 120)) { bounds in Paint.browser(bounds, tab: "YouTube", address: "youtube.com", video: true) }
        screen.add(browser, at: NSPoint(x: 20, y: 22))
        screen.add(youtube, at: NSPoint(x: 20, y: 22))
        Loop.middle(youtube)
        screen.warm(strength)
        return captioned(screen, "Any other site: Night Shift on", "youtube.com in front: true colour")
    }

    /// The notification after overruling a rule, and the moon in the menu bar at login.
    private static func lastThings(app: String, strength: UInt8) -> NSView {
        let screen = MiniScreen(NSSize(width: 300, height: 140), app: app, otherApp: nil, forcesOff: false)
        screen.add(Drawn(NSSize(width: 160, height: 102)) { Paint.figma($0) }, at: NSPoint(x: 16, y: 26))
        screen.warm(strength, loops: false)
        let banner = Drawn(NSSize(width: 226, height: 46)) { bounds in
            Paint.lifted { Paint.rounded(bounds, 12, NSColor(white: 0.97, alpha: 0.97)) }
            NSApp.applicationIconImage.draw(in: NSRect(x: 6, y: 9, width: 28, height: 28))
            Paint.text("Night Shift turned on outside Nightcap", .systemFont(ofSize: 9, weight: .semibold), NSColor(white: 0.11, alpha: 1), at: NSPoint(x: 38, y: 9))
            Paint.text("Snoozed the \(app) rule until 07:00.", .systemFont(ofSize: 9), NSColor(white: 0.11, alpha: 1), at: NSPoint(x: 38, y: 23))
        }
        screen.add(banner, at: NSPoint(x: 66, y: 22))
        Loop.fades(banner, values: [0, 0, 1, 1, 0, 0], times: [0, 0.12, 0.22, 0.8, 0.92, 1], duration: 5)
        let left = captioned(screen, "Overrule a rule in Control Center, and Nightcap says so.", nil)
        let moon = NSRect(x: 214, y: 4, width: 26, height: 22)
        let bar = Drawn(NSSize(width: 300, height: 56)) { _ in
            Paint.lifted { Paint.rounded(NSRect(x: 0, y: 0, width: 300, height: 30), 8, Paint.bar) }
            let font = NSFont.systemFont(ofSize: 12)
            Paint.text("Finder", .systemFont(ofSize: 12, weight: .bold), .labelColor, at: NSPoint(x: 14, y: 7))
            Paint.text("File", font, .labelColor, at: NSPoint(x: 66, y: 7))
            Paint.text("Edit", font, .labelColor, at: NSPoint(x: 98, y: 7))
            Paint.template("MenuBarOff", .labelColor, in: NSRect(x: moon.midX - 7, y: moon.midY - 7, width: 14, height: 14))
            Paint.text("Tue 08:30", font, .labelColor, at: NSPoint(x: moon.maxX + 6, y: 7))
            Paint.text("Log in, and the moon is already there.", .systemFont(ofSize: 11), .nightcapSecondary, centredAt: NSPoint(x: 150, y: 48))
        }
        bar.addSubview(Ring(around: moon, radius: 5))
        return side(by: left, bar)
    }

    /// Nightcap's menu, open under the moon, as the app draws it on macOS 27: the Night Shift
    /// item, the strength between the menu's suns, everywhere's row, then the front app's
    /// (ADR-0018). It stops there: Settings and Quit, under them, don't fit the stage.
    private static func ready() -> NSView {
        Drawn(NSSize(width: 460, height: 164)) { _ in
            let font = NSFont.systemFont(ofSize: 12)
            Paint.lifted { Paint.rounded(NSRect(x: 0, y: 0, width: 460, height: 30), 8, Paint.bar) }
            Paint.text("Mail", .systemFont(ofSize: 12, weight: .bold), .labelColor, at: NSPoint(x: 16, y: 7))
            Paint.text("File", font, .labelColor, at: NSPoint(x: 56, y: 7))
            Paint.text("Edit", font, .labelColor, at: NSPoint(x: 88, y: 7))
            let moon = NSRect(x: 372, y: 5, width: 26, height: 20)
            Paint.rounded(moon, 10, Paint.highlight)
            Paint.template("MenuBarOn", .labelColor, in: NSRect(x: moon.midX - 7, y: moon.midY - 7, width: 14, height: 14))
            Paint.text("21:42", font, .labelColor, at: NSPoint(x: 410, y: 7))
            // A status item's menu opens under it, to the right, unless that would leave the
            // screen, as it would this close to the edge: then it stays inside.
            let menu = NSRect(x: 460 - 4 - 212, y: 36, width: 212, height: 122)
            Paint.lifted { Paint.rounded(menu, 10, Paint.menu) }
            let item = NSFont.systemFont(ofSize: 11)
            // The menu's columns: the checkmark, the item's image, and the title.
            let check = menu.minX + 10
            let image = menu.minX + 25
            let title = menu.minX + 38
            // `text` from `x`, its middle `y` down the menu.
            let line = { (text: String, font: NSFont, colour: NSColor, x: CGFloat, y: CGFloat) in
                let height = (text as NSString).size(withAttributes: [.font: font]).height
                Paint.text(text, font, colour, at: NSPoint(x: x, y: menu.minY + y - height / 2))
            }
            let chevron = { (y: CGFloat) in
                Paint.glyph("chevron.right", 9, .labelColor, centredAt: NSPoint(x: menu.maxX - 20, y: menu.minY + y))
            }
            // The Night Shift item: the check, the moon and the shortcut sit across its lines.
            Paint.glyph("checkmark", 9, .labelColor, centredAt: NSPoint(x: check, y: menu.minY + 25))
            Paint.template("MenuBarOn", .labelColor, in: NSRect(x: image - 6.5, y: menu.minY + 18.5, width: 13, height: 13))
            line("Night Shift", item, .labelColor, title, 13.5)
            let reason = StatusMenu.wrapped("Following the schedule until 07:00").split(separator: "\n")
            for (index, text) in reason.enumerated() {
                line(String(text), .systemFont(ofSize: 10), .nightcapSecondary, title, 26 + CGFloat(index) * 12)
            }
            if let keys = KeyboardShortcuts.getShortcut(for: .nightShift)?.description {
                line(keys, item, .nightcapSecondary, menu.maxX - 15 - Paint.width(keys, item), 25)
            }
            // The strength, as a slider in a menu draws: grey, with a white knob.
            let strength: CGFloat = 0.6
            let y = menu.minY + 58
            Paint.template(StatusMenu.sun(large: false), .nightcapSecondary, in: NSRect(x: title, y: y - 4.5, width: 9, height: 9))
            Paint.template(StatusMenu.sun(large: true), .nightcapSecondary, in: NSRect(x: menu.maxX - 26, y: y - 6.5, width: 13, height: 13))
            let track = NSRect(x: menu.minX + 52, y: y - 1.5, width: menu.width - 83, height: 3)
            let knob = track.minX + 7.5 + (track.width - 15) * strength
            Paint.rounded(track, 1.5, NSColor.labelColor.withAlphaComponent(0.15))
            Paint.rounded(NSRect(x: track.minX, y: track.minY, width: knob - track.minX, height: 3), 1.5, NSColor.labelColor.withAlphaComponent(0.45))
            Paint.rounded(NSRect(x: knob - 7.5, y: y - 5.5, width: 15, height: 11), 5.5, .white, stroke: .nightcapControlBorder)
            Paint.glyph("timer", 10, .labelColor, centredAt: NSPoint(x: image, y: menu.minY + 79))
            line("Turn Off", item, .labelColor, title, 79)
            chevron(79)
            NSColor.nightcapSeparator.setFill()
            NSRect(x: menu.minX + 10, y: menu.minY + 94, width: menu.width - 20, height: 0.5).fill()
            if let mail = NSWorkspace.shared.urlForApplication(withBundleIdentifier: "com.apple.mail") {
                NSWorkspace.shared.icon(forFile: mail.path).draw(
                    in: NSRect(x: image - 7, y: menu.minY + 102, width: 14, height: 14), from: .zero, operation: .sourceOver,
                    fraction: 1, respectFlipped: true, hints: nil)
            }
            line("Turn Off in Mail", item, .labelColor, title, 109)
            chevron(109)
        }
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

    private static func clock(_ time: TimeOfDay) -> String {
        String(format: "%02d:%02d", time.hour, time.minute)
    }
}

/// The strength step's two screens, daytime in true colour and the evening warmed as the slider
/// says.
final class StrengthArt: NSView {
    var strength: UInt8 {
        didSet { evening.needsDisplay = true; caption.needsDisplay = true }
    }
    private let evening: Drawn
    private let caption: Drawn

    init(strength: UInt8) {
        self.strength = strength
        let screen = NSSize(width: 236, height: 140)
        var warmth: () -> UInt8 = { strength }
        evening = Drawn(screen) { bounds in
            Paint.warm(Paint.alpha(for: warmth())).setFill()
            NSBezierPath(roundedRect: bounds, xRadius: 8, yRadius: 8).fill()
        }
        caption = Drawn(NSSize(width: screen.width, height: 16)) { bounds in
            Paint.text(
                "Evening: Night Shift, \(Paint.warmth(warmth()).lowercased())", .systemFont(ofSize: 11),
                .nightcapSecondary, centredAt: NSPoint(x: bounds.midX, y: 8))
        }
        super.init(frame: NSRect(x: 0, y: 0, width: screen.width * 2 + 78, height: 162))
        warmth = { [weak self] in self?.strength ?? strength }
        let day = MiniScreen(screen, app: "Mail", otherApp: nil, forcesOff: false, daytime: true)
        day.add(Drawn(NSSize(width: 196, height: 102)) { Paint.page($0) }, at: NSPoint(x: 20, y: 26))
        let night = MiniScreen(screen, app: "Mail", otherApp: nil, forcesOff: false)
        night.add(Drawn(NSSize(width: 196, height: 102)) { Paint.page($0) }, at: NSPoint(x: 20, y: 26))
        night.add(evening, at: .zero)
        day.frame.origin = .zero
        night.frame.origin = NSPoint(x: screen.width + 78, y: 0)
        addSubview(day)
        addSubview(night)
        let arrow = Drawn(NSSize(width: 22, height: 22)) { bounds in
            Paint.glyph("arrow.right", 16, .nightcapSecondary, centredAt: NSPoint(x: bounds.midX, y: bounds.midY))
        }
        arrow.frame.origin = NSPoint(x: screen.width + 28, y: 59)
        addSubview(arrow)
        let dayCaption = Drawn(NSSize(width: screen.width, height: 16)) { bounds in
            Paint.text("Daytime: true colour", .systemFont(ofSize: 11), .nightcapSecondary, centredAt: NSPoint(x: bounds.midX, y: 8))
        }
        dayCaption.frame.origin = NSPoint(x: 0, y: 146)
        caption.frame.origin = NSPoint(x: night.frame.minX, y: 146)
        addSubview(dayCaption)
        addSubview(caption)
    }

    required init?(coder: NSCoder) {
        fatalError("not built from a nib")
    }

    override var isFlipped: Bool { true }
    override var intrinsicContentSize: NSSize { frame.size }
}

/// A small Mac screen: wallpaper, a menu bar naming the app in front, and windows on top.
final class MiniScreen: NSView {
    private let size: NSSize

    /// With `otherApp`, the menu bar names it in the middle of each loop, as when that app comes
    /// to the front; `forcesOff` gives the moon its "z" then. A `daytime` screen shows the moon
    /// off and an afternoon time.
    init(_ size: NSSize, app: String, otherApp: String?, forcesOff: Bool, daytime: Bool = false) {
        self.size = size
        super.init(frame: NSRect(origin: .zero, size: size))
        wantsLayer = true
        layer?.cornerRadius = 8
        layer?.masksToBounds = true
        let backdrop = Drawn(size) { bounds in
            NSGradient(starting: NSColor(srgbRed: 0.62, green: 0.72, blue: 0.88, alpha: 1),
                       ending: NSColor(srgbRed: 0.78, green: 0.71, blue: 0.86, alpha: 1))?.draw(in: bounds, angle: -20)
        }
        addSubview(backdrop)
        let bar = Drawn(NSSize(width: size.width, height: 15)) { bounds in
            NSColor(white: 0.97, alpha: 0.88).setFill()
            bounds.fill()
        }
        addSubview(bar)
        let ink = NSColor(white: 0.11, alpha: 1)
        let name = { (app: String) in
            Drawn(NSSize(width: 60, height: 15)) { _ in
                Paint.text(app, .systemFont(ofSize: 8, weight: .bold), ink, at: NSPoint(x: 0, y: 3))
            }
        }
        let first = name(app)
        first.frame.origin = NSPoint(x: 8, y: 0)
        addSubview(first)
        if let otherApp {
            let second = name(otherApp)
            second.frame.origin = first.frame.origin
            addSubview(second)
            Loop.middle(second)
            Loop.ends(first)
        }
        let status = { (image: String) in
            Drawn(NSSize(width: 40, height: 15)) { _ in
                Paint.template(image, ink, in: NSRect(x: 0, y: 3, width: 9, height: 9))
                Paint.text(daytime ? "14:00" : "21:42", .systemFont(ofSize: 8), ink, at: NSPoint(x: 13, y: 3))
            }
        }
        let on = status(daytime ? "MenuBarOff" : "MenuBarOn")
        on.frame.origin = NSPoint(x: size.width - 44, y: 0)
        addSubview(on)
        if forcesOff {
            let forced = status("MenuBarForcedOff")
            forced.frame.origin = on.frame.origin
            addSubview(forced)
            Loop.middle(forced)
            Loop.ends(on)
        }
    }

    required init?(coder: NSCoder) {
        fatalError("not built from a nib")
    }

    override var isFlipped: Bool { true }
    override var intrinsicContentSize: NSSize { size }

    func add(_ view: NSView, at origin: NSPoint) {
        view.frame.origin = origin
        addSubview(view)
    }

    /// Night Shift's tint over the whole screen, lifting while the loop's other app is in front.
    func warm(_ strength: UInt8, loops: Bool = true) {
        let tint = Drawn(size) { bounds in
            Paint.warm(Paint.alpha(for: strength)).setFill()
            bounds.fill()
        }
        add(tint, at: .zero)
        if loops { Loop.ends(tint, lifting: true) }
    }
}

/// A flipped view drawn by a closure, at a fixed size.
final class Drawn: NSView {
    private let paint: (NSRect) -> Void

    init(_ size: NSSize, _ paint: @escaping (NSRect) -> Void) {
        self.paint = paint
        super.init(frame: NSRect(origin: .zero, size: size))
    }

    required init?(coder: NSCoder) {
        fatalError("not built from a nib")
    }

    override var isFlipped: Bool { true }
    override var intrinsicContentSize: NSSize { frame.size }

    override func draw(_ dirtyRect: NSRect) {
        paint(bounds)
    }
}

/// A slow pulsing ring around a control the scene points at.
final class Ring: NSView {
    init(around rect: NSRect, radius: CGFloat) {
        super.init(frame: rect)
        wantsLayer = true
        layer?.cornerRadius = radius
        layer?.borderWidth = 2
        layer?.borderColor = NSColor.controlAccentColor.withAlphaComponent(0.55).cgColor
        Loop.pulse(self)
    }

    required init?(coder: NSCoder) {
        fatalError("not built from a nib")
    }
}

/// The scenes' loops, which run slowly and stop with Reduce Motion, each mock then showing how
/// its loop ends.
enum Loop {
    private static var reduced: Bool { NSWorkspace.shared.accessibilityDisplayShouldReduceMotion }

    /// Shown only in the middle of each loop, while the other app is in front.
    static func middle(_ view: NSView) {
        guard !reduced else { return }
        view.alphaValue = 0
        fades(view, values: [0, 0, 1, 1, 0, 0], times: [0, 0.28, 0.34, 0.66, 0.72, 1], duration: 7)
    }

    /// Shown at the loop's ends, gone in the middle; under Reduce Motion, gone for good.
    static func ends(_ view: NSView, lifting: Bool = false) {
        guard !reduced else {
            view.alphaValue = 0
            return
        }
        let times: [NSNumber] = lifting ? [0, 0.28, 0.36, 0.64, 0.72, 1] : [0, 0.28, 0.34, 0.66, 0.72, 1]
        fades(view, values: [1, 1, 0, 0, 1, 1], times: times, duration: 7)
    }

    static func fades(_ view: NSView, values: [CGFloat], times: [NSNumber], duration: CFTimeInterval) {
        guard !reduced else { return }
        view.wantsLayer = true
        let fade = CAKeyframeAnimation(keyPath: "opacity")
        fade.values = values
        fade.keyTimes = times
        fade.duration = duration
        fade.repeatCount = .infinity
        fade.calculationMode = .linear
        view.layer?.add(fade, forKey: "loop")
    }

    static func pulse(_ view: NSView) {
        guard !reduced else { return }
        let pulse = CABasicAnimation(keyPath: "opacity")
        pulse.fromValue = 1
        pulse.toValue = 0.15
        pulse.duration = 0.9
        pulse.autoreverses = true
        pulse.repeatCount = .infinity
        pulse.timingFunction = CAMediaTimingFunction(name: .easeInEaseOut)
        view.layer?.add(pulse, forKey: "pulse")
    }
}

/// Drawing for the scenes, in flipped views.
enum Paint {
    static var menu: NSColor { .appearance(light: 0xf0f0f2, dark: 0x2a2a2d, lightAlpha: 0.97, darkAlpha: 0.97) }
    static var bar: NSColor { .appearance(light: 0xf6f6f8, dark: 0x1c1c1f, lightAlpha: 0.95, darkAlpha: 0.9) }
    static var chrome: NSColor { .appearance(light: 0xececef, dark: 0x2a2a2d) }
    static var highlight: NSColor { .appearance(light: 0x000000, dark: 0xffffff, lightAlpha: 0.1, darkAlpha: 0.14) }

    /// Night Shift's warmth, the brand's `tint`.
    static func warm(_ alpha: CGFloat) -> NSColor {
        NSColor(srgbRed: 1, green: 138 / 255, blue: 46 / 255, alpha: alpha)
    }

    static func alpha(for strength: UInt8) -> CGFloat {
        0.08 + 0.34 * CGFloat(strength) / 100
    }

    static func warmth(_ strength: UInt8) -> String {
        strength < 34 ? "A little warm" : strength < 67 ? "Warm" : "Very warm"
    }

    static func rounded(_ rect: NSRect, _ radius: CGFloat, _ fill: NSColor, stroke: NSColor? = nil) {
        let path = NSBezierPath(roundedRect: rect, xRadius: radius, yRadius: radius)
        fill.setFill()
        path.fill()
        if let stroke {
            stroke.setStroke()
            path.lineWidth = 0.5
            path.stroke()
        }
    }

    /// What follows is drawn with the soft shadow of something floating.
    static func lifted(_ body: () -> Void) {
        NSGraphicsContext.saveGraphicsState()
        let shadow = NSShadow()
        shadow.shadowOffset = NSSize(width: 0, height: -4)
        shadow.shadowBlurRadius = 11
        shadow.shadowColor = NSColor.black.withAlphaComponent(0.16)
        shadow.set()
        body()
        NSGraphicsContext.restoreGraphicsState()
    }

    static func width(_ text: String, _ font: NSFont) -> CGFloat {
        (text as NSString).size(withAttributes: [.font: font]).width
    }

    static func text(_ text: String, _ font: NSFont, _ colour: NSColor, at point: NSPoint) {
        (text as NSString).draw(at: point, withAttributes: [.font: font, .foregroundColor: colour])
    }

    static func text(_ text: String, _ font: NSFont, _ colour: NSColor, centredAt point: NSPoint) {
        let size = (text as NSString).size(withAttributes: [.font: font])
        self.text(text, font, colour, at: NSPoint(x: point.x - size.width / 2, y: point.y - size.height / 2))
    }

    static func glyph(_ symbol: String, _ size: CGFloat, _ colour: NSColor, centredAt point: NSPoint) {
        guard let image = NSImage(systemSymbolName: symbol, accessibilityDescription: nil)?
            .withSymbolConfiguration(.init(pointSize: size, weight: .regular).applying(.init(paletteColors: [colour])))
        else { return }
        image.draw(in: NSRect(
            x: point.x - image.size.width / 2, y: point.y - image.size.height / 2,
            width: image.size.width, height: image.size.height),
            from: .zero, operation: .sourceOver, fraction: 1, respectFlipped: true, hints: nil)
    }

    /// One of the asset catalog's template images, such as the menu-bar moons, in `colour`.
    static func template(_ name: String, _ colour: NSColor, in rect: NSRect) {
        guard let image = NSImage(named: name) else { return }
        template(image, colour, in: rect)
    }

    /// A template image, such as the menu's suns, in `colour`.
    static func template(_ image: NSImage, _ colour: NSColor, in rect: NSRect) {
        let tinted = NSImage(size: rect.size, flipped: false) { bounds in
            image.draw(in: bounds)
            colour.set()
            bounds.fill(using: .sourceAtop)
            return true
        }
        tinted.draw(in: rect, from: .zero, operation: .sourceOver, fraction: 1, respectFlipped: true, hints: nil)
    }

    static func trafficLights(at origin: NSPoint, size: CGFloat) {
        for (index, colour) in [NSColor.systemRed, .systemYellow, .systemGreen].enumerated() {
            colour.withAlphaComponent(0.9).setFill()
            NSBezierPath(ovalIn: NSRect(
                x: origin.x + CGFloat(index) * (size + 4), y: origin.y, width: size, height: size)).fill()
        }
    }

    static func toggle(_ rect: NSRect, on: Bool, faded: Bool = false) {
        let alpha: CGFloat = faded ? 0.5 : 1
        rounded(rect, rect.height / 2, on ? NSColor.controlAccentColor.withAlphaComponent(alpha) : .nightcapTile)
        let knob = rect.height - 4
        let x = on ? rect.maxX - knob - 2 : rect.minX + 2
        rounded(NSRect(x: x, y: rect.minY + 2, width: knob, height: knob), knob / 2, NSColor.white.withAlphaComponent(alpha))
    }

    /// A light window with lines of text, as a mini screen's windows have; screens stay light, as
    /// a picture of one would.
    static func window(_ rect: NSRect, _ fill: NSColor = .white) {
        lifted { rounded(rect, 5, fill) }
    }

    private static let line = NSColor(white: 0, alpha: 0.08)

    static func page(_ bounds: NSRect) {
        window(bounds)
        rounded(NSRect(x: 10, y: 10, width: bounds.width * 0.46, height: 5), 2.5, NSColor.systemBlue.withAlphaComponent(0.8))
        rounded(NSRect(x: 10, y: 22, width: bounds.width * 0.78, height: 5), 2.5, line)
        picture(NSRect(x: 10, y: 36, width: 84, height: 56), dark: false)
        rounded(NSRect(x: 104, y: 40, width: 72, height: 5), 2.5, line)
        rounded(NSRect(x: 104, y: 52, width: 58, height: 5), 2.5, line)
    }

    static func mail(_ bounds: NSRect) {
        window(bounds)
        for (y, share) in [(12.0, 0.5), (26, 0.8), (38, 0.72), (54, 0.6)] {
            rounded(NSRect(x: 10, y: y, width: bounds.width * share, height: 5), 2.5, line)
        }
    }

    /// A design in Figma: two overlapping discs and three swatches.
    static func figma(_ bounds: NSRect) {
        window(bounds, NSColor(srgbRed: 0.97, green: 0.95, blue: 0.91, alpha: 1))
        let disc = min(bounds.height * 0.5, 54)
        NSColor.systemBlue.setFill()
        NSBezierPath(ovalIn: NSRect(x: 20, y: 12, width: disc, height: disc)).fill()
        NSGraphicsContext.saveGraphicsState()
        NSGraphicsContext.current?.compositingOperation = .multiply
        NSColor.systemPink.setFill()
        NSBezierPath(ovalIn: NSRect(x: 20 + disc * 0.6, y: 12 + disc * 0.5, width: disc, height: disc)).fill()
        NSGraphicsContext.restoreGraphicsState()
        guard bounds.width > 180 else { return }
        for (index, colour) in [NSColor.systemBlue, .systemPink, .systemGreen].enumerated() {
            rounded(NSRect(x: bounds.width - 62, y: 16 + CGFloat(index) * 24, width: 18, height: 18), 5, colour)
        }
    }

    static func picture(_ rect: NSRect, dark: Bool) {
        let start = dark ? NSColor(srgbRed: 0.12, green: 0.17, blue: 0.29, alpha: 1) : .systemBlue
        let path = NSBezierPath(roundedRect: rect, xRadius: 4, yRadius: 4)
        NSGradient(starting: start, ending: .systemPink)?.draw(in: path, angle: -40)
    }

    static func browser(_ bounds: NSRect, tab: String, address: String, video: Bool) {
        window(bounds, NSColor(white: 0.96, alpha: 1))
        NSGraphicsContext.saveGraphicsState()
        NSBezierPath(roundedRect: bounds, xRadius: 5, yRadius: 5).addClip()
        NSColor(white: 0.9, alpha: 1).setFill()
        NSRect(x: 0, y: 0, width: bounds.width, height: 20).fill()
        rounded(NSRect(x: 8, y: 4, width: 92, height: 18), 4, NSColor(white: 0.96, alpha: 1))
        let ink = NSColor(white: 0.11, alpha: 1)
        text(tab, .systemFont(ofSize: 8), ink, at: NSPoint(x: 14, y: 7))
        text(address, .monospacedSystemFont(ofSize: 8, weight: .regular), NSColor(white: 0.37, alpha: 1), at: NSPoint(x: 8, y: 23))
        NSColor.white.setFill()
        NSRect(x: 0, y: 38, width: bounds.width, height: bounds.height - 38).fill()
        if video {
            picture(NSRect(x: 12, y: 47, width: 140, height: 64), dark: true)
            rounded(NSRect(x: 166, y: 52, width: 110, height: 6), 3, line)
            rounded(NSRect(x: 166, y: 66, width: 80, height: 6), 3, line)
        } else {
            for (y, share) in [(50.0, 0.4), (64, 0.7), (78, 0.6)] {
                rounded(NSRect(x: 10, y: y, width: bounds.width * share, height: 5), 2.5, line)
            }
        }
        NSGraphicsContext.restoreGraphicsState()
    }
}
