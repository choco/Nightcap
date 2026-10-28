import Foundation
import NightcapCore

/// An app the tour names: its bundle ID, which a rule names it by, and its name, which the menu
/// calls it by once it's in front.
struct TourApp: Equatable {
    let bundleId: String
    let name: String
}

/// The site the tour opens for its site step, and the browser it opens it in.
struct TourSite: Equatable {
    let domain: String
    let browser: String
}

/// What a running tour's steps follow (ADR-0016), from the core's answers, whichever way the
/// user made a change: the sample's and the site's places, the switch's side, and whether the
/// tour's trial stands in for the schedule.
struct TourFacts: Equatable {
    /// The trial stands in for the schedule with Night Shift on: the tour found it off.
    let standsIn: Bool
    /// Night Shift on everywhere, as the Night Shift item's check shows it: the switch's side.
    let everywhereOn: Bool
    let sample: TourPlace
    /// Nil where the tour has no site step.
    let site: TourPlace?
}

/// The tour sample or the site, as its step follows it.
struct TourPlace: Equatable {
    /// The sample is the front app, or the site is the one in the front tab (ADR-0017).
    let inFront: Bool
    /// What its rule says, if it has one.
    let rule: OnOff?
    /// A shorter rule at its place says the other way from its rule.
    let snoozed: Bool
}

extension TourFacts {
    /// Whether the tour asks its trial to stand in for the schedule with Night Shift on, as it
    /// finds Night Shift in `snapshot`: where it's off, as the tour's first step then says.
    static func asksForStandIn(_ snapshot: Snapshot) -> Bool {
        snapshot.nightShift().state == .off
    }

    /// What the steps follow in `snapshot` for the sample with bundle ID `sample`, and the site at
    /// `site`, or nil while no trial runs.
    init?(_ snapshot: Snapshot, sample: String, site: String?) {
        guard let trial = snapshot.trial() else { return nil }
        let front = snapshot.front()
        let everyRule = snapshot.rules()
        let followed = { (place: Place, inFront: Bool) in
            let rules = everyRule.filter { $0.place == place }
            let rule = rules.first { $0.length == .indefinite }
            // A place's temporary rules come shortest first.
            let shortest = rules.first { $0.length != .indefinite }
            return TourPlace(
                inFront: inFront, rule: rule?.says,
                snoozed: rule.flatMap { rule in shortest.map { $0.says != rule.says } } ?? false)
        }
        self.init(
            standsIn: trial.standsIn,
            everywhereOn: front.everywhere.gives == .on,
            sample: followed(.app(sample), front.app == sample),
            site: site.map { followed(.domain($0), front.site == $0) })
    }
}
