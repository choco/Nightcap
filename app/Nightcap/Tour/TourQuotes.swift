import Foundation
import NightcapCore

/// The menu's items the tour's steps quote (ADR-0016), in the words the menu's place rows use in
/// the situation each step describes, so a step quotes what the menu shows: the sample in front
/// with no rule, with its rule, then with the rule snoozed; the site in front in its browser;
/// and the example in front with Night Shift off. TourQuoteWordingTests checks each against the
/// menu a staged core leads to. The tour runs with Night Shift on, so the sample's row turns it
/// off until the sample has its rule.
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
        let sampleOff = words.turn(.off, at: .app(name: sample.name))
        addSampleRule = Wording.Quote(row: sampleOff, item: words.always)
        snoozeSample = words.turn(.on, at: .app(name: sample.name))
        putSampleRuleBack = Wording.Quote(row: sampleOff, item: words.putTheRuleBack)
        addSiteRule = site.map { site in
            Wording.Quote(row: words.turn(.off, at: .site(domain: site.domain)), item: words.always)
        }
        addExampleRule = Wording.Quote(row: words.turn(.on, at: .app(name: example.name)), item: words.always)
    }
}
