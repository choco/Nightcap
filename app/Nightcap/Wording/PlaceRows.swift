import Foundation
import NightcapCore

/// The place rows' words (ADR-0018): everywhere's row under the Night Shift item, then the front
/// app's and site's, with the items in their submenus and the lines under them.
extension Wording {
    /// Everywhere's row, which turns Night Shift `state` everywhere: "Turn Off".
    func turn(_ state: OnOff) -> String {
        switch state {
        case .on: text("Turn On")
        case .off: text("Turn Off")
        }
    }

    /// An app's or site's row while it has something to offer: "Turn Off in Figma". A site's says
    /// "This Site" and names the site on a line of its own under it, so that a long domain doesn't
    /// widen the menu.
    func turn(_ state: OnOff, at place: Place) -> String {
        switch (state, place) {
        case (.on, .app(let name)): text("Turn On in \(name)")
        case (.off, .app(let name)): text("Turn Off in \(name)")
        case (.on, .site): text("Turn On at This Site")
        case (.off, .site): text("Turn Off at This Site")
        }
    }

    /// An app's or site's row with nothing to offer, which is greyed: "In Figma".
    func nothingToOffer(at place: Place) -> String {
        switch place {
        case .app(let name): text("In \(name)")
        case .site: text("At This Site")
        }
    }

    /// The line under everywhere's row while the app or site in front keeps the other way, which
    /// the row can't change there: "Everywhere but in Figma".
    func everywhereBut(_ place: Place) -> String {
        switch place {
        case .app(let name): text("Everywhere but in \(name)")
        case .site(let domain): text("Everywhere but at \(domain)")
        }
    }

    /// The title of Custom…'s window, which asks when turning Night Shift `state` everywhere
    /// ends: "Turn Off Everywhere".
    func timePickerTitle(_ state: OnOff) -> String {
        switch state {
        case .on: text("Turn On Everywhere")
        case .off: text("Turn Off Everywhere")
        }
    }

    /// The title of Custom…'s window for an app or site, which names a site by its domain: "Turn
    /// On at figma.com".
    func timePickerTitle(_ state: OnOff, at place: Place) -> String {
        switch (state, place) {
        case (.on, .app(let name)): text("Turn On in \(name)")
        case (.off, .app(let name)): text("Turn Off in \(name)")
        case (.on, .site(let domain)): text("Turn On at \(domain)")
        case (.off, .site(let domain)): text("Turn Off at \(domain)")
        }
    }

    /// Takes everywhere's changes off, back to what the schedule says.
    var backToTheSchedule: String { text("Back to the Schedule") }

    /// Ends the change over a place's rule, which snoozes it.
    var putTheRuleBack: String { text("Put the Rule Back") }

    /// Ends the change at a place with no rule, back to everywhere's say.
    var likeEverywhereElse: String { text("Like Everywhere Else") }

    /// Ends the change at a site with no rule, back to the say of its browser or of a domain that
    /// covers it: "Like the Rest of Safari", "Like the Rest of figma.com".
    func likeTheRest(of place: Place) -> String {
        switch place {
        case .app(let browser): text("Like the Rest of \(browser)")
        // A key of its own: a domain shares the English with an app, but not every language says
        // both alike.
        case .site(let domain):
            String(
                localized: "Like the Rest of %@, a domain", defaultValue: "Like the Rest of \(domain)",
                bundle: #bundle, locale: locale,
                comment: """
                    A site’s place row in the menu: ends its change, back to the say of a domain that covers it, \
                    such as figma.com for docs.figma.com.
                    """)
        }
    }

    var removeYourRule: String { text("Remove Your Rule") }

    /// Gives the place a rule that turns it the row's way for good.
    var always: String { text("Always") }

    /// Under Always, greyed, where the place's rule is the config's.
    var yourConfigDecides: String { text("Your config decides") }

    /// Asks for the time a change ends.
    var custom: String { text("Custom…") }

    /// A preset length, in whole minutes: "For 30 Minutes", "For 1 Hour", "For 1 Hour 30 Minutes".
    func forLength(_ length: SignedDuration) -> String {
        let minutes = Int(length / 60)
        return switch (minutes / 60, minutes % 60) {
        case (0, let minutes): text("For \(minutes) Minutes")
        case (let hours, 0): text("For \(hours) Hours")
        case (let hours, let minutes): text("For \(hours) Hours \(minutes) Minutes")
        }
    }

    /// A change until the schedule next turns Night Shift back, at `turn`: "Until 07:00".
    func until(_ turn: Date) -> String {
        text("Until \(time(turn))")
    }

    /// The line under "Until 07:00", which says whose time it is: the moment `schedule` next turns
    /// Night Shift `state`, such as "At sunrise" for sunset to sunrise turning it off.
    func moment(of schedule: Schedule, turning state: OnOff) -> String {
        switch (schedule, state) {
        case (.sunsetToSunrise, .off): text("At sunrise")
        case (.sunsetToSunrise, .on): text("At sunset")
        // A schedule that's off never turns, so the menu never shows these two.
        case (.custom, .off), (.off, .off): text("When the schedule ends")
        case (.custom, .on), (.off, .on): text("When the schedule starts")
        }
    }

    /// The line under an app's or site's row: what its place says, top down, from its changes to
    /// its rule, such as "On until 07:00, then your config’s rule". None where it says nothing.
    func placeSays(_ changes: [Change], then rule: (says: OnOff, whose: Whose)?) -> String? {
        guard let first = changes.first else {
            return rule.map { rule in
                switch rule.says {
                case .on: text("Always on: \(whose(rule.whose))")
                case .off: text("Always off: \(whose(rule.whose))")
                }
            }
        }
        var after = changes.dropFirst().map(thenChange)
        if let rule { after.append(whose(rule.whose)) }
        return after.reduce(change(first.state, until: first.until)) { line, next in text("\(line), then \(next)") }
    }

    /// A change after another in a line: "off until 07:00".
    private func thenChange(_ change: Change) -> String {
        switch (change.state, change.until) {
        case (.on, let until?): text("on until \(time(until))")
        case (.off, let until?): text("off until \(time(until))")
        case (.on, nil): text("on until you turn it off")
        case (.off, nil): text("off until you turn it on")
        }
    }
}
