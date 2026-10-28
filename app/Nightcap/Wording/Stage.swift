import Foundation
import NightcapCore

/// The words on onboarding's stage: its captions, and what its mocks of macOS say, with their
/// clocks in the user's.
extension Wording {
    /// Under the Night Shift steps' timeline: when Night Shift is on, from `start` to `end`, and
    /// whether those times follow the Mac's location.
    func timeline(onNow: Bool, from start: TimeOfDay, to end: TimeOfDay, followsLocation: Bool) -> String {
        let when =
            onNow
            ? text("On now, until \(time(end)).")
            : text("Comes on at \(time(start)), and goes off at \(time(end)).")
        return followsLocation ? when + " " + text("The times follow your location.") : when
    }

    /// The timeline's marker for the time it is.
    var timelineNow: String { text("Now") }

    /// The mock of Control Center's Display module.
    var display: String { text("Display") }
    var trueTone: String { text("True Tone") }
    var darkMode: String { text("Dark Mode") }
    var byHandCaption: String { text("Control Center › Display, or the top of Nightcap’s menu") }

    var mailInFront: String { text("Mail in front: Night Shift on") }
    var figmaInFront: String { text("Figma in front: true colour, and the moon gets a “z”") }

    var addressCaption: String { text("Of the whole address, Nightcap keeps only the site’s name.") }

    /// The mock of System Settings at Accessibility.
    var privacyAndSecurity: String { text("Privacy & Security") }
    var backToAccessibility: String { text("‹ Accessibility") }

    /// The mock browser's ordinary tab.
    var inbox: String { text("Inbox") }
    var anyOtherSite: String { text("Any other site: Night Shift on") }
    var youtubeInFront: String { text("youtube.com in front: true colour") }

    var overruleARule: String { text("Overrule a rule in Control Center, and Nightcap says so.") }

    /// The notification the last things' stage shows: an outside change snoozed `app`'s rule
    /// until 07:00, in the notification's own words.
    func snoozedNotice(_ app: String) -> (title: String, body: String) {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = timeZone
        let seven = calendar.date(from: DateComponents(year: 2026, month: 10, day: 28, hour: 7, minute: 0))!
        return (turnedOutsideNightcap(.on), snoozed(.app(name: app), .on, until: seven))
    }
    var loginCaption: String { text("Log in, and the moon is already there.") }

    /// A mock menu bar's menus after the app's name.
    var file: String { text("File") }
    var edit: String { text("Edit") }

    /// The strength step's screens: in the daytime, and in the evening at `strength`.
    var daytime: String { text("Daytime: true colour") }

    func evening(_ strength: UInt8) -> String {
        switch Warmth(strength) {
        case .aLittle: text("Evening: Night Shift, a little warm")
        case .warm: text("Evening: Night Shift, warm")
        case .very: text("Evening: Night Shift, very warm")
        }
    }

    /// The mocks' menu-bar clocks: the evening they show, an afternoon, and a Tuesday morning at
    /// login, in the user's clock as every time reads, AM or PM and all.
    var eveningClock: String { menuBarClock(hour: 21, minute: 42) }
    var afternoonClock: String { menuBarClock(hour: 14, minute: 0) }
    var loginClock: String { menuBarClock(hour: 8, minute: 30, weekday: true) }

    /// A time on a Tuesday, as a menu bar shows it.
    private func menuBarClock(hour: Int, minute: Int, weekday: Bool = false) -> String {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = .gmt
        let tuesday = calendar.date(from: DateComponents(year: 2026, month: 10, day: 27, hour: hour, minute: minute))!
        let style = clock(in: .gmt)
        return tuesday.formatted(weekday ? style.weekday(.abbreviated) : style)
    }
}
