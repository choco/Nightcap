import Foundation
import NightcapCore
import Testing

private let sample = TourApp(bundleId: tourSample, name: "Tour Sample")
private let site = TourSite(domain: "nightcap.choco.me", browser: "com.apple.Safari")
private let mail = TourApp(bundleId: "com.apple.mail", name: "Mail")

/// The row's item that sends a command `wanted` picks out, as the tour quotes it, and its command.
private func quoted(_ row: Menu.PlaceRow?, _ wanted: (Command) -> Bool) throws -> (Wording.Quote, Command) {
    let row = try #require(row)
    let choices = row.items.compactMap { item in
        if case .choice(let choice) = item { choice } else { nil }
    }
    let item = try #require(choices.first { wanted($0.command) })
    return (Wording.Quote(row: row.title, item: item.title), item.command)
}

private func ruleFor(_ identity: RuleIdentity) -> (Command) -> Bool {
    { command in
        if case .addRule(let ruled, _) = command { ruled == identity } else { false }
    }
}

/// The menu's items the tour quotes, as the menu shows them in the situation each step describes.
struct TourQuoteWordingTests {
    @Test func theToursQuotesAreTheMenusWords() {
        let quotes = TourQuotes(sample: sample, site: site, example: mail, words: .staged)

        #expect(quotes.addSampleRule == Wording.Quote(row: "Turn Off in Tour Sample", item: "Always"))
        #expect(quotes.snoozeSample == "Turn On in Tour Sample")
        #expect(quotes.putSampleRuleBack == Wording.Quote(row: "Turn Off in Tour Sample", item: "Put the Rule Back"))
        #expect(quotes.addSiteRule == Wording.Quote(row: "Turn Off at This Site", item: "Always"))
        #expect(quotes.addExampleRule == Wording.Quote(row: "Turn On in Mail", item: "Always"))
    }

    @Test func eachQuoteIsWhereTheMenuShowsItInTheSituationItsStepDescribes() throws {
        let quotes = TourQuotes(sample: sample, site: site, example: mail, words: .staged)
        let core = try staged(
            "2026-10-16T20:00:00+02:00", config: "websites = true\n", reported(false), tourBegunStandingIn,
            reported(true))
        let snoozes = { (command: Command) in
            if case .set(_, _, .for) = command { true } else { false }
        }

        core.bringToFront(tourSample)
        let (addSampleRule, always) = try quoted(core.appRow, ruleFor(.app(tourSample)))
        #expect(addSampleRule == quotes.addSampleRule)
        core.send(command: always)
        let row = try #require(core.appRow)
        #expect(row.title == quotes.snoozeSample)
        core.send(command: try quoted(row, snoozes).1)
        let (putBack, _) = try quoted(core.appRow) { $0 == .reset(.app(tourSample)) }
        #expect(putBack == quotes.putSampleRuleBack)

        core.bringToFront("com.apple.Safari", browser: true)
        core.reportHost("nightcap.choco.me")
        #expect(try quoted(core.siteRow, ruleFor(.domain("nightcap.choco.me"))).0 == quotes.addSiteRule)

        core.send(command: .off)
        core.report(
            fact: .statusChanged(enabled: false, schedule: customHours, sunScheduleSetAside: false, writtenAt: nil))
        core.bringToFront("com.apple.mail")
        #expect(try quoted(core.appRow, ruleFor(.app("com.apple.mail"))).0 == quotes.addExampleRule)
    }

    @Test func aTourWithoutASiteStepQuotesNoSite() {
        #expect(TourQuotes(sample: sample, site: nil, example: mail, words: .staged).addSiteRule == nil)
    }

    @Test func aStepQuotesTheItemAfterItsRow() {
        let quote = Wording.Quote(row: "Turn Off in Tour Sample", item: "Always")

        #expect(Wording.staged.quoted(quote) == "“Turn Off in Tour Sample” › “Always”")
    }
}
