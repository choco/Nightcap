import Foundation
import NightcapCore
import Testing

/// 22:00 on 16 October, `seconds` later.
private func tenPM(_ seconds: TimeInterval) -> Date {
    at("2026-10-16T22:00:00+02:00") + seconds
}

/// The schedule's fade in over the half hour from 22:00.
private func halfHour() throws -> TintCurve {
    try #require(TintCurve([TintPoint(at: tenPM(0), tint: 0), TintPoint(at: tenPM(1800), tint: 100)]))
}

/// The tint a write moves along, as the core gives it, which the adapter moves along in steps as
/// macOS does (ADR-0021).
struct TintCurveTests {
    @Test func theTintRunsInStraightLinesBetweenThePointsAndStaysPastThem() throws {
        let curve = try #require(
            TintCurve([
                TintPoint(at: tenPM(0), tint: 0), TintPoint(at: tenPM(100), tint: 50),
                TintPoint(at: tenPM(200), tint: 0),
            ]))

        #expect(curve.tint(at: tenPM(-10)) == 0)
        #expect(curve.tint(at: tenPM(50)) == 0.25)
        #expect(curve.tint(at: tenPM(150)) == 0.25)
        #expect(curve.tint(at: tenPM(300)) == 0)
    }

    @Test func eachStepFadesForTwoSecondsToWhereTheCurveWillBe() throws {
        let curve = try halfHour()

        let step = curve.step(at: tenPM(900))

        #expect(step.tint == curve.tint(at: tenPM(902)))
        #expect(step.fade == 2)
        #expect(!step.last)
    }

    @Test func theLastStepFadesToTheEndInTheTimeLeft() throws {
        let step = try halfHour().step(at: tenPM(1799))

        #expect(step.tint == 1)
        #expect(step.fade == 1)
        #expect(step.last)
    }

    @Test func handingOverFadesToTheNextPointInTheTimeLeft() throws {
        let curve = try halfHour()

        let handOver = try #require(curve.handOver(at: tenPM(900)))

        #expect(handOver.tint == 1)
        #expect(handOver.fade == 900)
        #expect(curve.handOver(at: tenPM(1800)) == nil)
    }

    @Test func aCurveWithNoPointsIsNone() {
        #expect(TintCurve([]) == nil)
    }
}
