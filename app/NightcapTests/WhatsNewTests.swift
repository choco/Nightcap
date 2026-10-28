import Foundation
import NightcapCore
import Testing

/// The version a staged core runs, as the app's launch reads it.
private func launching() throws -> About {
    try staged("2026-10-16T12:00:00+02:00").snapshot().about()
}

/// When a launch says what's new: the first launch of another version than the one that last
/// launched, once.
struct WhatsNewTests {
    @Test func aFirstRunHasNothingNewToTell() throws {
        let about = try launching()
        withScratchDefaults { defaults in
            let whatsNew = WhatsNew(defaults: defaults, words: .staged)

            #expect(whatsNew.launched(about) == nil)
            #expect(defaults.string(forKey: "lastVersion") == about.version)
        }
    }

    @Test func theFirstLaunchOfANewVersionSaysWhatsNewOnce() throws {
        let about = try launching()
        withScratchDefaults { defaults in
            defaults.set("1.0.0", forKey: "lastVersion")
            let whatsNew = WhatsNew(defaults: defaults, words: .staged)

            let notice = whatsNew.launched(about)

            #expect(notice == Notice(updatedTo: about.version, words: .staged))
            #expect(notice?.title == "Nightcap updated to \(about.version)")
            #expect(whatsNew.launched(about) == nil)
        }
    }

    @Test func theSameVersionLaunchedAgainHasNothingNew() throws {
        let about = try launching()
        withScratchDefaults { defaults in
            defaults.set(about.version, forKey: "lastVersion")
            let whatsNew = WhatsNew(defaults: defaults, words: .staged)

            #expect(whatsNew.launched(about) == nil)
        }
    }
}
