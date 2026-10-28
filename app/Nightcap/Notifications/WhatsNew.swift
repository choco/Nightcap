import Foundation
import NightcapCore

/// What's new after an update. Updates install silently, so the first launch of another version
/// says so, from the version that last launched, which the app keeps in `defaults`.
struct WhatsNew {
    let defaults: UserDefaults
    let words: Wording

    private static let lastVersion = "lastVersion"

    /// The update notice as `about`'s version launches, if it isn't the one that last launched,
    /// and records it as that one. A first run has nothing new; a run from before the app kept the
    /// version is an update once the user has `onboarded`.
    func launched(_ about: About, onboarded: Bool) -> Notice? {
        let last = defaults.string(forKey: Self.lastVersion)
        defaults.set(about.version, forKey: Self.lastVersion)
        guard last != about.version, last != nil || onboarded else { return nil }
        return Notice(updatedTo: about.version, words: words)
    }
}
