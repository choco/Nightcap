import Testing

struct NightShiftStrengthTests {
    @Test func aCommittedWriteReportsTheReadbackEvenWhenCoreBrightnessRefusesIt() {
        for accepted in [false, true] {
            var events: [String] = []
            var observed: Float = 0.4
            var reports: [UInt8] = []
            let strength = NightShiftStrength(
                read: {
                    events.append("read")
                    return observed
                },
                write: { value, commit in
                    events.append("write")
                    #expect(value == 0.6)
                    #expect(commit)
                    if accepted { observed = value }
                    return accepted
                },
                onStrength: {
                    events.append("report")
                    reports.append($0)
                })

            strength.set(60, commit: true)

            #expect(events == ["write", "read", "report"])
            #expect(reports == [accepted ? 60 : 40])
        }
    }

    @Test func aPreviewNeverReadsOrReportsTheSavedStrength() {
        var writes = 0
        let strength = NightShiftStrength(
            read: {
                Issue.record("a preview read the strength")
                return 0.5
            },
            write: { value, commit in
                #expect(value == 0.3)
                #expect(!commit)
                writes += 1
                return true
            },
            onStrength: { _ in Issue.record("a preview reported the saved strength") })

        strength.set(30, commit: false)
        #expect(writes == 1)
    }

    @Test func anUnavailableOrInvalidReadbackDoesNotInventAStrength() {
        for observed: Float? in [nil, -0.1, 1.1, .nan, .infinity] {
            let strength = NightShiftStrength(
                read: { observed }, write: { _, _ in true },
                onStrength: { _ in Issue.record("an invalid readback was reported") })
            #expect(strength.value == nil)
            strength.set(60, commit: true)
        }
    }

    @Test func readsRoundTheFractionToTheNearestWholeStrengthWithoutReporting() {
        for (fraction, expected): (Float, UInt8) in [(0, 0), (0.456, 46), (1, 100)] {
            let strength = NightShiftStrength(
                read: { fraction }, write: { _, _ in true },
                onStrength: { _ in Issue.record("a poll announced a change") })
            #expect(strength.value == expected)
        }
    }
}
