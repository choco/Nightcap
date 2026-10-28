import Foundation
import NightcapCore
import Testing

private let site = "nightcap.choco.me"

private let away = TourPlace(inFront: false, rule: nil, snoozed: false)

/// What the tour's steps follow, from `core`'s answers, whose switch is always the Night Shift
/// item's.
private func facts(_ core: Core, site: String? = site) throws -> TourFacts {
    let facts = try #require(TourFacts(TourFacts.Answers(asking: core), sample: tourSample, site: site))
    #expect(facts.everywhereOn == core.menu().nightShift.on)
    return facts
}

/// What a running tour's steps follow, as the core's answers have it (ADR-0016).
struct TourFactsTests {
    @Test func noTourRunsNoFacts() throws {
        let core = try onByTheSchedule()

        #expect(TourFacts(TourFacts.Answers(asking: core), sample: tourSample, site: site) == nil)
    }

    @Test func theSamplesFactsFollowItToTheFrontIntoItsRuleAndAway() throws {
        let core = try onByTheSchedule(tourStarted)
        #expect(try facts(core).sample == away)

        core.bringToFront(tourSample)
        #expect(try facts(core).sample == TourPlace(inFront: true, rule: nil, snoozed: false))
        core.send(command: try #require(core.appRow).choice(titled: "Always").command)
        #expect(try facts(core).sample == TourPlace(inFront: true, rule: .off, snoozed: false))
        core.bringToFront("com.apple.mail")

        #expect(try facts(core).sample == TourPlace(inFront: false, rule: .off, snoozed: false))
    }

    @Test func aSnoozeFromTheSamplesRowAndPuttingTheRuleBackShowInItsFacts() throws {
        let core = try onByTheSchedule(
            tourStarted, front(tourSample), .command(.addRule(identity: .app(tourSample), says: .off)))

        core.send(command: try #require(core.appRow).choice(titled: "For 30 Minutes").command)
        #expect(try facts(core).sample == TourPlace(inFront: true, rule: .off, snoozed: true))
        core.send(command: try #require(core.appRow).choice(titled: "Put the Rule Back").command)

        #expect(try facts(core).sample == TourPlace(inFront: true, rule: .off, snoozed: false))
    }

    @Test func theSitesOwnRuleShowsInItsFactsAndATapChangesOnlyTheSwitch() throws {
        let core = try onByTheSchedule(
            config: "websites = true\n", tourStarted, front("com.apple.Safari", browser: true),
            host("www.nightcap.choco.me"))
        let always = try #require(core.siteRow).choice(titled: "Always")
        #expect(always.command == .addRule(identity: .domain(site), says: .off))
        core.send(command: always.command)
        let ruled = try facts(core)
        #expect(ruled.site == TourPlace(inFront: true, rule: .off, snoozed: false))
        #expect(ruled.everywhereOn)

        core.send(command: .toggle)

        #expect(
            try facts(core)
                == TourFacts(standsIn: ruled.standsIn, everywhereOn: false, sample: ruled.sample, site: ruled.site))
    }

    @Test func aTourWithoutASiteStepFollowsNoSite() throws {
        #expect(try facts(try onByTheSchedule(tourStarted), site: nil).site == nil)
    }
}
