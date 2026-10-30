import Foundation
import NightcapCore
import Testing

/// A write mark CoreBrightness keeps, `seconds` after 23:00 on 16 October.
private func mark(_ seconds: TimeInterval) -> Date {
    Date(timeIntervalSince1970: 1_792_184_400 + seconds)
}

/// Whose write a status report is the first to show (ADR-0004): the schedule's switches leave no
/// new mark, and of the new marks, the one saying what Nightcap last wrote is its own.
struct NightShiftWritesTests {
    @Test func aReportWithNoNewMarkIsTheSchedules() {
        var writes = NightShiftWrites()

        #expect(writes.report(enabled: true, writtenAt: nil) == .schedule)
        #expect(writes.report(enabled: false, writtenAt: mark(0)) == .outside)
        #expect(writes.report(enabled: false, writtenAt: mark(0)) == .schedule)
    }

    @Test func nightcapsWriteIsTheFirstNewMarkSayingWhatItWrote() {
        var writes = NightShiftWrites()
        _ = writes.report(enabled: true, writtenAt: nil)
        writes.wrote(false)

        // CoreBrightness can call back several times for one change, and before the write lands.
        #expect(writes.report(enabled: true, writtenAt: nil) == .schedule)
        #expect(writes.report(enabled: false, writtenAt: mark(0)) == .nightcap)
        #expect(writes.report(enabled: false, writtenAt: mark(0)) == .schedule)
    }

    @Test func aNewMarkSayingOtherwiseIsAnOutsideChange() {
        var writes = NightShiftWrites()
        writes.wrote(false)

        #expect(writes.report(enabled: true, writtenAt: mark(0)) == .outside)
        #expect(writes.report(enabled: false, writtenAt: mark(1)) == .outside)
    }

    @Test func aWriteIsMadeOnceWhileItsOnItsWayAndNotAtAllWhereNightShiftIsAlreadySo() {
        var writes = NightShiftWrites()

        #expect(!writes.needs(true, current: true))
        #expect(writes.needs(false, current: true))
        writes.wrote(false)
        #expect(!writes.needs(false, current: true))

        _ = writes.report(enabled: false, writtenAt: mark(0))
        #expect(writes.needs(true, current: false))
    }
}
