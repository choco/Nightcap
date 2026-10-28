import Foundation
import NightcapCore

/// What the notifications say: an outside change adopted as a snooze, the first launch after an
/// update, and a problem that appeared on its own.
extension Wording {
    /// Night Shift switched outside Nightcap, as in Control Center: "Night Shift turned on outside
    /// Nightcap".
    func turnedOutsideNightcap(_ state: OnOff) -> String {
        switch state {
        case .on: text("Night Shift turned on outside Nightcap")
        case .off: text("Night Shift turned off outside Nightcap")
        }
    }

    /// The rule an outside change to `state` snoozed, and until when: "Snoozed the Figma rule until
    /// 07:00.", or with no end "Snoozed the Figma rule until you turn it off."
    func snoozed(_ rule: Place, _ state: OnOff, until end: Date?) -> String {
        let name =
            switch rule {
            case .app(let name): name
            case .site(let domain): domain
            }
        return switch (end, state) {
        case (let end?, _): text("Snoozed the \(name) rule until \(time(end)).")
        case (nil, .on): text("Snoozed the \(name) rule until you turn it off.")
        case (nil, .off): text("Snoozed the \(name) rule until you turn it on.")
        }
    }

    func updated(to version: String) -> String {
        text("Nightcap updated to \(version)")
    }

    /// Under an update's title: a click opens its changes.
    var whatsNew: String { text("Click to see what’s new.") }

    /// macOS set sunset to sunrise aside, until Location Services is on.
    var scheduleSetAside: (title: String, body: String) {
        (
            text("Sunset to sunrise needs Location Services"),
            text("macOS turned Night Shift’s schedule off.") + " "
                + text("Open Nightcap’s settings to turn Location Services back on, or to keep the schedule off.")
        )
    }

    /// Sunset to sunrise without the sun times.
    var noSunTimes: (title: String, body: String) {
        (
            text("Nightcap can’t follow sunset to sunrise"),
            text("macOS hasn’t worked out sunrise and sunset.") + " "
                + text("Custom hours in Night Shift’s settings work without them.")
        )
    }

    /// Website checking without the Accessibility grant.
    var accessibilityNotAllowed: (title: String, body: String) {
        (
            text("Website checking needs Accessibility"),
            text("Open Nightcap’s settings to allow it, or to stop checking websites.")
        )
    }

    /// The login item removed while Nightcap is to open at login.
    var openAtLoginRemoved: (title: String, body: String) {
        (
            text("Nightcap no longer opens at login"),
            text("Open Nightcap’s settings to turn it back on, or to keep it off.")
        )
    }

    /// The config can't be used, and why.
    func configInvalid(_ error: FileError) -> (title: String, body: String) {
        (text("Nightcap’s config is invalid"), located(error))
    }

    /// The ad-hoc rules file can't be used, and why.
    func adHocRulesOff(_ error: FileError) -> (title: String, body: String) {
        (text("Ad-hoc rules are off"), located(error))
    }
}
