import Foundation
import NightcapCore

/// What's new after an update. Updates install silently, so the first launch of another version
/// says so, from the version that last launched, which the app keeps in `defaults`.
struct WhatsNew {
    let defaults: UserDefaults
    let words: Wording

    private static let lastVersion = "lastVersion"

    /// The update notice as `about`'s version launches, if another one launched last, and records
    /// it as the one that last launched. A first run, with none recorded, has nothing new.
    func launched(_ about: About) -> Notice? {
        let last = defaults.string(forKey: Self.lastVersion)
        defaults.set(about.version, forKey: Self.lastVersion)
        guard let last, last != about.version else { return nil }
        return Notice(updatedTo: about.version, words: words)
    }
}
