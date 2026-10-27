import Foundation
import NightcapCore

/// The menu's titles the tour's steps quote (ADR-0016), in the place rows' own words, as the menu
/// shows them in the situation each step describes. The tour runs with Night Shift on, so the
/// sample's row turns it off until the sample has its rule.
extension Wording {
    /// A menu item as the tour quotes it: the place row it's in, then the item.
    struct Quote: Equatable {
        let row: String
        let item: String
    }

    /// The item as a step quotes it: “Turn Off in Tour Sample” › “Always”.
    func quoted(_ quote: Quote) -> String {
        text("“\(quote.row)” › “\(quote.item)”")
    }

    /// Always, in the row of `sample`, by its name, with it in front and no rule.
    func addSampleRule(_ sample: String) -> Quote {
        Quote(row: turn(.off, at: .app(name: sample)), item: always)
    }

    /// The sample's row while its rule holds, whose lengths snooze the rule.
    func snoozeSample(_ sample: String) -> String {
        turn(.on, at: .app(name: sample))
    }

    /// What ends the rule's snooze, in the sample's row.
    func putSampleRuleBack(_ sample: String) -> Quote {
        Quote(row: turn(.off, at: .app(name: sample)), item: putTheRuleBack)
    }

    /// Always, in the row of the site at `domain`, with its browser in front on it.
    func addSiteRule(_ domain: String) -> Quote {
        Quote(row: turn(.off, at: .site(domain: domain)), item: always)
    }

    /// Always, in the row of `example`, by its name, with it in front and Night Shift off: a rule
    /// that says on.
    func addExampleRule(_ example: String) -> Quote {
        Quote(row: turn(.on, at: .app(name: example)), item: always)
    }
}
