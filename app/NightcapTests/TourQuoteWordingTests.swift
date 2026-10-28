import Foundation
import NightcapCore
import Testing

private let words = Wording.staged

/// The menu's titles the tour quotes, as the core reports them for a running tour.
struct TourQuoteWordingTests {
    @Test func theToursQuotesAreTheMenusWords() throws {
        let titles = try #require(try onByTheSchedule(tourStarted).tourReport()).titles
        let quote = { (path: MenuPath) in Wording.Quote(row: path.row, item: path.item) }

        #expect(quote(titles.addSampleRule) == words.addSampleRule("Tour Sample"))
        #expect(titles.snoozeSample == words.snoozeSample("Tour Sample"))
        #expect(quote(titles.putSampleRuleBack) == words.putSampleRuleBack("Tour Sample"))
        #expect(titles.addSiteRule.map(quote) == words.addSiteRule("nightcap.choco.me"))
        #expect(quote(titles.addExampleRule) == words.addExampleRule("Mail"))
    }

    @Test func aStepQuotesTheItemAfterItsRow() {
        let quote = Wording.Quote(row: "Turn Off in Tour Sample", item: "Always")

        #expect(words.quoted(quote) == "“Turn Off in Tour Sample” › “Always”")
    }
}
