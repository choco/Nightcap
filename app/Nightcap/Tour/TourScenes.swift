import Foundation
import NightcapCore

/// The menu's items the tour's steps quote (ADR-0016), from the menu itself as it shows them in
/// the situation each step describes, so a step quotes what the menu shows: the sample in front
/// with no rule, with its rule, then with the rule snoozed; the site in front in its browser;
/// and the example in front with Night Shift off. Each scene is what the core answers in that
/// situation, which TourQuoteWordingTests checks against a staged core. The tour runs with Night
/// Shift on, so the sample's row turns it off until the sample has its rule. Each item is found
/// by the command it sends, so a renamed one carries through.
struct TourQuotes: Equatable {
    /// Always, in the sample's row with the sample in front and no rule.
    let addSampleRule: Wording.Quote
    /// The sample's row while its rule holds, whose lengths snooze the rule.
    let snoozeSample: String
    /// What ends the rule's snooze, in the sample's row.
    let putSampleRuleBack: Wording.Quote
    /// Always, in the site's row with its browser in front on it; nil without a site step.
    let addSiteRule: Wording.Quote?
    /// Always, in the example's row with it in front and Night Shift off: a rule that says on.
    let addExampleRule: Wording.Quote

    init(sample: TourApp, site: TourSite?, example: TourApp, words: Wording) {
        let place = Place.app(sample.bundleId)
        let rule = Rule(place: place, says: .off, length: .indefinite, setBy: .trial)
        let snooze = Rule(place: place, says: .on, length: .until(until: Self.snoozeEnd), setBy: .you)
        let fresh = PlaceInFront(
            place: place, gives: .on, rules: [], base: nil, inherits: Self.tourOn, afterUntil: .on, afterReset: .on)
        addSampleRule = Self.quote(Self.row(in: sample, [fresh], words: words), \.addsRule)
        let ruled = PlaceInFront(
            place: place, gives: .off, rules: [], base: .rule(rule), inherits: Self.tourOn, afterUntil: .off,
            afterReset: .off)
        snoozeSample =
            Self.row(
                in: sample, [ruled], decision: Decision(state: .off, place: place, reasons: [.rule(rule)]),
                words: words
            ).title
        let snoozed = PlaceInFront(
            place: place, gives: .on, rules: [snooze], base: .rule(rule), inherits: Self.tourOn, afterUntil: .off,
            afterReset: .off)
        let putBack = Self.row(
            in: sample, [snoozed], decision: Decision(state: .on, place: place, reasons: [.rule(snooze), .rule(rule)]),
            words: words)
        putSampleRuleBack = Self.quote(putBack) { $0 == .reset(place) }
        addSiteRule = site.map { site in
            let tab = PlaceInFront(
                place: .domain(site.domain), gives: .on, rules: [], base: nil, inherits: Self.tourOn,
                afterUntil: .on, afterReset: .on)
            let browser = PlaceInFront(
                place: .app(site.browser), gives: .on, rules: [], base: nil, inherits: Self.tourOn, afterUntil: .on,
                afterReset: .on)
            let row = Self.row(
                in: TourApp(bundleId: site.browser, name: ""), on: site.domain, [tab, browser], words: words)
            return Self.quote(row, \.addsRule)
        }
        let saysNothing = { (place: Place) in
            PlaceInFront(
                place: place, gives: .off, rules: [], base: nil, inherits: nil, afterUntil: nil, afterReset: nil)
        }
        let off = Self.row(
            in: example, [saysNothing(.app(example.bundleId))], everywhere: saysNothing(.everywhere),
            afterSwitch: [Rule(place: .everywhere, says: .on, length: .indefinite, setBy: .you)],
            decision: Decision(state: nil, place: .everywhere, reasons: []), nightShift: .off, words: words)
        addExampleRule = Self.quote(off, \.addsRule)
    }

    /// When the scene's snooze ends. The quoted items depend on none of the time, the schedule or
    /// the lengths, so the scenes leave them out.
    private static let snoozeEnd = Date(timeIntervalSince1970: 30 * 60)

    /// What a place that says nothing inherits while the tour stands in: everywhere's on.
    private static let tourOn = Inherited(from: .everywhere, says: .on)

    /// Everywhere while the tour stands in for the schedule with Night Shift on.
    private static let forTheTour = PlaceInFront(
        place: .everywhere, gives: .on, rules: [], base: .standIn, inherits: nil, afterUntil: .on, afterReset: .on)

    /// The menu's row for the first of `places`, with `app` in front, on the site at `domain` if
    /// there's one: by default with the tour standing in, which decides.
    private static func row(
        in app: TourApp, on domain: String? = nil, _ places: [PlaceInFront], everywhere: PlaceInFront = forTheTour,
        afterSwitch: [Rule] = [Rule(place: .everywhere, says: .off, length: .indefinite, setBy: .you)],
        decision: Decision = Decision(state: .on, place: .everywhere, reasons: [.standIn]),
        nightShift: NightShift = .on, words: Wording
    ) -> Menu.PlaceRow {
        let menu = Menu(
            nightShift: NightShiftState(
                state: nightShift, strength: nil, schedule: nil, setAside: false, scheduled: nil, nextOn: nil,
                nextOff: nil),
            front: Front(
                app: app.bundleId, watching: domain != nil, host: domain, site: domain, decision: decision,
                places: places, everywhere: everywhere, afterSwitch: afterSwitch),
            lengths: [], checks: [], now: snoozeEnd, appName: { _ in app.name }, words: words)
        let place = places[0].place
        return menu.rows.first { $0.place == place }!
    }

    /// The row's item that sends the command `wanted` picks out, which each scene's row offers.
    private static func quote(_ row: Menu.PlaceRow, _ wanted: (Command) -> Bool) -> Wording.Quote {
        let choices = row.items.compactMap { item in
            if case .choice(let choice) = item { choice } else { nil }
        }
        let item = choices.first { wanted($0.command) }!
        return Wording.Quote(row: row.title, item: item.title)
    }
}

extension Command {
    /// Whether it gives a place its indefinite rule, as Always does.
    fileprivate var addsRule: Bool {
        if case .addRule = self { true } else { false }
    }
}
