import Foundation
import NightcapCore

/// What the menu shows (ADR-0018), decided from the core's answers and worded through `Wording`:
/// the moon, the problem line, the Night Shift item, the strength and the place rows, each item
/// with the command it sends. The status item draws it, onboarding's stage copies it, and the
/// tour quotes it.
struct Menu: Equatable {
    /// The menu-bar icon.
    enum Moon: Equatable {
        case on, off
        /// An app or site keeps Night Shift off with everywhere on.
        case forcedOff
        case unavailable
    }

    /// The menu's first item, after the problem line: everywhere's switch.
    struct NightShiftItem: Equatable {
        /// Checked while Night Shift is on everywhere.
        let on: Bool
        /// What decides the screen, under the title: "Off in Figma: your config’s rule".
        let line: String?
        /// The switch; nil while Night Shift is unavailable, which disables the item.
        let command: Command?
    }

    /// The line above the Night Shift item while any problem exists. Choosing it opens Settings
    /// where the problem is, and while it shows the moon carries a dot.
    struct ProblemLine: Equatable {
        /// "2 Things Need Attention…".
        let title: String
        /// What needs it: "Accessibility and notifications".
        let subtitle: String
    }

    /// Under the Night Shift item, in order.
    enum Entry: Equatable {
        /// The strength's slider, 0 to 100. Moving it sends `previewStrength`, and letting go
        /// sends `strength`.
        case strength(UInt8)
        case row(PlaceRow)
        case separator
    }

    /// A place row: a submenu that turns one place the other way, everywhere's under the Night
    /// Shift item, then the front app's and site's.
    struct PlaceRow: Equatable {
        let place: Place
        let title: String
        /// A site's domain, on a line of its own under the title, which says "This Site" so that
        /// a long domain doesn't widen the menu.
        let site: String?
        /// What the place says, or for everywhere's row the place in front that keeps its own say.
        let subtitle: String?
        /// The submenu; empty while there's nothing to offer, which greys the row.
        let items: [Item]
    }

    enum Item: Equatable {
        case choice(Choice)
        case pickTime(PickTime)
        case separator
    }

    /// Choosing it sends `command`.
    struct Choice: Equatable {
        let title: String
        /// A line under the title, such as when a length ends.
        var subtitle: String? = nil
        var enabled = true
        let command: Command
    }

    /// Custom…: asks for an end time in a window titled `title`, starting at `start`, and sends
    /// `set` for `place` and `state` until the time picked.
    struct PickTime: Equatable {
        /// The item's own title.
        let item: String
        let title: String
        let place: Place
        let state: OnOff
        let start: Date
    }

    let moon: Moon
    let problemLine: ProblemLine?
    let nightShift: NightShiftItem
    var entries: [Entry]
}

extension Menu {
    /// The menu for `snapshot` at `now`, in `words`, which call the front app by `appName`'s
    /// name for its bundle ID, else by the bundle ID.
    init(_ snapshot: Snapshot, now: Date, appName: (String) -> String?, words: Wording) {
        let nightShift = snapshot.nightShift()
        guard nightShift.state != .unavailable else {
            self.init(
                moon: .unavailable, problemLine: nil,
                nightShift: NightShiftItem(on: false, line: words.unavailable, command: nil), entries: [])
            return
        }
        let front = snapshot.front()
        let rows = Rows(
            nightShift: nightShift, front: front, lengths: snapshot.config().lengths, checks: snapshot.checks().checks,
            now: now, frontName: front.app.flatMap(appName), words: words)
        let decision = front.decision
        let everywhere = front.everywhere.gives
        var entries: [Entry] = []
        if let strength = nightShift.strength { entries.append(.strength(strength)) }
        if let row = rows.row(front.everywhere) { entries.append(.row(row)) }
        // The front app's row, then the front site's under it.
        let inFront = front.places.reversed().compactMap(rows.row)
        if !inFront.isEmpty { entries.append(.separator) }
        entries += inFront.map(Entry.row)
        let moon: Moon =
            switch decision.state ?? everywhere {
            case .off where decision.place != .everywhere && everywhere == .on: .forcedOff
            case .on: .on
            case .off: .off
            }
        self.init(
            moon: moon, problemLine: rows.problemLine(),
            nightShift: NightShiftItem(
                on: everywhere == .on, line: rows.line(), command: everywhere == .on ? .off : .on),
            entries: entries)
    }
}

/// What the rows read, and how they word it.
private struct Rows {
    let nightShift: NightShiftState
    let front: Front
    let lengths: [SignedDuration]
    let checks: [Check]
    let now: Date
    /// The front app's name, where it has one.
    let frontName: String?
    let words: Wording

    /// A place's row (ADR-0018), which turns the place the other way from what it gives: undoing
    /// first, a reset back to what's under its rules where that goes the row's way, and removing a
    /// rule of the user's; then its lengths, short of those that would come to the same as
    /// undoing; then at an app or site Always, its rule. Everywhere's offers nothing the switch
    /// would do anyway, and with nothing to offer is left out, where an app's or site's is greyed.
    func row(_ at: PlaceInFront) -> Menu.PlaceRow? {
        let place = at.place
        let state = at.gives.other()
        let named = named(place)
        let rule = at.rule
        // What a reset leaves: an app's or site's rule, else what it inherits; for everywhere, the
        // stand-in or the schedule.
        let under = named == nil ? at.baseSays : rule?.says ?? at.inherits?.says
        let undo: String? =
            switch (named, rule, at.inherits?.from) {
            case _ where at.rules.isEmpty || under != state: nil
            // The switch keeps everywhere's rules that say its way, so only where one does would
            // going back to the schedule differ from it. While a trial stands in, the schedule
            // isn't there to go back to.
            case (nil, _, _):
                if case .schedule(_, .some) = at.base, at.hasRule(saying: state) {
                    words.backToTheSchedule
                } else {
                    nil
                }
            case (_, .some, _): words.putTheRuleBack
            // A site follows a domain that covers it, or its browser, where that has its own say.
            case (_, nil, .domain(let domain)?): words.likeTheRest(of: .site(domain: domain))
            case (_, nil, .app(let browser)?): words.likeTheRest(of: .app(name: name(of: browser)))
            case (_, nil, _): words.likeEverywhereElse
            }
        var undoing = undo.map { [Menu.Item.choice(Menu.Choice(title: $0, command: .reset(place)))] } ?? []
        // Removing a rule is the user's to do whenever they like, even where nothing on screen
        // would change.
        if let rule, rule.setBy != .config, let identity = place.identity() {
            undoing.append(.choice(Menu.Choice(title: words.removeYourRule, command: .removeRule(identity))))
        }
        // A new rule takes off its place's rules of its length and shorter, so over a rule until a
        // time with the row's way under it, a length would come to the same as taking that off.
        let window = named.map { words.timePickerTitle(state, at: $0) } ?? words.timePickerTitle(state)
        var lengths =
            at.changedForATime && (under == state || at.hasRule(saying: state))
            ? [] : picks(at: place, state, window: window)
        if under != state, let until = untilTurn(at: place, state) { lengths.append(until) }
        let always: Menu.Choice? =
            switch (place.identity(), rule) {
            case (nil, _): nil
            case (_, let rule?) where rule.says == state: nil
            case (let identity?, let rule?) where rule.setBy == .config:
                Menu.Choice(
                    title: words.always, subtitle: words.yourConfigDecides, enabled: false,
                    command: .addRule(identity: identity, says: state))
            case (let identity?, _):
                Menu.Choice(title: words.always, command: .addRule(identity: identity, says: state))
            }
        let groups: [[Menu.Item]] = [undoing, lengths, always.map { [.choice($0)] } ?? []]
        let items = Array(groups.filter { !$0.isEmpty }.joined(separator: [.separator]))
        let offers = items.contains { item in
            switch item {
            case .choice(let choice): choice.enabled
            case .pickTime: true
            case .separator: false
            }
        }
        guard let named else {
            // The app or site in front keeps the other way, which the row can't change there.
            let keeps = front.decision.state != state ? self.named(front.decision.place) : nil
            return offers
                ? Menu.PlaceRow(
                    place: place, title: words.turn(state), site: nil, subtitle: keeps.map(words.everywhereBut),
                    items: items)
                : nil
        }
        let site: String? = if case .domain(let domain) = place { domain } else { nil }
        return Menu.PlaceRow(
            place: place,
            title: offers ? words.turn(state, at: named) : words.nothingToOffer(at: named),
            site: site,
            subtitle: words.placeSays(
                at.rules.map { Wording.Change(state: $0.says, until: $0.length.end()) },
                then: rule.map { (says: $0.says, whose: Self.whose($0.setBy)) }),
            items: offers ? items : [])
    }

    /// The line under the Night Shift item: what decides the screen.
    func line() -> String? {
        let decision = front.decision
        guard let top = decision.reasons.first else {
            // Without the sun times, only macOS knows where the schedule stands, and applies it.
            return nightShift.schedule == .sunsetToSunrise ? words.followingTheSchedule : nil
        }
        return switch (top, named(decision.place)) {
        case (.rule(let rule), let place?) where rule.length == .indefinite:
            words.rule(rule.says, at: place, whose: Self.whose(rule.setBy))
        case (.rule(let rule), let place?): words.change(rule.says, at: place, until: rule.length.end())
        case (.rule(let rule), nil): words.change(rule.says, until: rule.length.end())
        case (.standIn, _): words.onForTheTour
        case (.schedule(_, let until?), _): words.followingTheSchedule(until: until)
        case (.schedule(_, nil), _):
            nightShift.schedule == .off ? words.noSchedule : words.followingTheSchedule
        }
    }

    /// The problem line, from the checks that are problems: drift or broken.
    func problemLine() -> Menu.ProblemLine? {
        let problems = checks.filter { $0.state.isProblem() }.map(\.detail)
        guard !problems.isEmpty else { return nil }
        return Menu.ProblemLine(title: words.thingsNeedAttention(problems.count), subtitle: words.needing(problems))
    }

    /// The lengths a change can be picked for at a place, then Custom…, whose window `window`
    /// names, starting an hour from now, rounded up to the quarter hour.
    private func picks(at place: Place, _ state: OnOff, window: String) -> [Menu.Item] {
        let quarter: TimeInterval = 15 * 60
        let start = Date(
            timeIntervalSince1970: ((now.timeIntervalSince1970 + 60 * 60) / quarter).rounded(.up) * quarter)
        return lengths.map { length in
            .choice(
                Menu.Choice(
                    title: words.forLength(length), command: .set(place: place, state: state, length: .for(length))))
        } + [.pickTime(Menu.PickTime(item: words.custom, title: window, place: place, state: state, start: start))]
    }

    /// Until the schedule next turns Night Shift back from `state`, shown as the time with the
    /// moment under it, since the time alone could be any clock's. Nil without a schedule.
    private func untilTurn(at place: Place, _ state: OnOff) -> Menu.Item? {
        guard let schedule = nightShift.schedule, let until = state == .on ? nightShift.nextOff : nightShift.nextOn
        else { return nil }
        return .choice(
            Menu.Choice(
                title: words.until(until), subtitle: words.moment(of: schedule, turning: state.other()),
                command: .set(place: place, state: state, length: .untilScheduleTurns)))
    }

    /// An app or site as the words name it; everywhere's sentences are their own.
    private func named(_ place: Place) -> Wording.Place? {
        switch place {
        case .everywhere: nil
        case .app(let bundleId): .app(name: name(of: bundleId))
        case .domain(let domain): .site(domain: domain)
        }
    }

    /// An app by its name while it's in front, else by its bundle ID.
    private func name(of bundleId: String) -> String {
        (front.app == bundleId ? frontName : nil) ?? bundleId
    }

    private static func whose(_ setBy: SetBy) -> Wording.Whose {
        switch setBy {
        case .config: .config
        case .you: .yours
        case .trial: .tour
        case .outsideChange: .outsideChange
        }
    }
}
