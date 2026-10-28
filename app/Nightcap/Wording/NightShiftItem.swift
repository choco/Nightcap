import Foundation
import NightcapCore

/// The line under the menu's Night Shift item: what decides the screen (ADR-0018).
extension Wording {
    /// While Night Shift is unavailable, which is all the menu says.
    var unavailable: String { text("Unavailable") }

    /// With no schedule, and nothing else deciding.
    var noSchedule: String { text("No schedule") }

    /// The schedule decides, and only macOS knows when it next turns: sunset to sunrise without
    /// the sun times.
    var followingTheSchedule: String { text("Following the schedule") }

    /// The schedule decides until it next turns, at `turn`: "Following the schedule until 07:00".
    func followingTheSchedule(until turn: Date) -> String {
        text("Following the schedule until \(time(turn))")
    }

    /// A trial stands in for the schedule with Night Shift on: only the tour begins one
    /// (ADR-0016).
    var onForTheTour: String { text("On for the tour") }

    /// A change everywhere decides: "Off until 23:30", or with no end "Off until you turn it on".
    func change(_ state: OnOff, until end: Date?) -> String {
        switch (state, end) {
        case (.on, let end?): text("On until \(time(end))")
        case (.off, let end?): text("Off until \(time(end))")
        case (.on, nil): text("On until you turn it off")
        case (.off, nil): text("Off until you turn it on")
        }
    }

    /// A change at an app or site decides: "On in Figma until 07:00".
    func change(_ state: OnOff, at place: Place, until end: Date?) -> String {
        switch (state, place, end) {
        case (.on, .app(let name), let end?): text("On in \(name) until \(time(end))")
        case (.off, .app(let name), let end?): text("Off in \(name) until \(time(end))")
        case (.on, .site(let domain), let end?): text("On at \(domain) until \(time(end))")
        case (.off, .site(let domain), let end?): text("Off at \(domain) until \(time(end))")
        case (.on, .app(let name), nil): text("On in \(name) until you turn it off")
        case (.off, .app(let name), nil): text("Off in \(name) until you turn it on")
        case (.on, .site(let domain), nil): text("On at \(domain) until you turn it off")
        case (.off, .site(let domain), nil): text("Off at \(domain) until you turn it on")
        }
    }

    /// An app's or site's rule decides: "Off in Figma: your config’s rule".
    func rule(_ says: OnOff, at place: Place, whose: Whose) -> String {
        let rule = self.whose(whose)
        return switch (says, place) {
        case (.on, .app(let name)): text("On in \(name): \(rule)")
        case (.off, .app(let name)): text("Off in \(name): \(rule)")
        case (.on, .site(let domain)): text("On at \(domain): \(rule)")
        case (.off, .site(let domain)): text("Off at \(domain): \(rule)")
        }
    }
}
