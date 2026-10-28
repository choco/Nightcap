import NightcapCore

/// The core's answers as the menu-bar UI reads them.
extension PlaceInFront {
    /// An app's or site's own indefinite rule, which its temporary rules sit over.
    var rule: Rule? { if case .rule(let rule) = base { rule } else { nil } }

    /// What its rules sit on says: an app's or site's rule, or for everywhere the stand-in's on,
    /// else the schedule's say once it's known.
    var baseSays: OnOff? {
        switch base {
        case .rule(let rule): rule.says
        case .standIn: .on
        case .schedule(let state, _): state
        case nil: nil
        }
    }

    /// Whether it has a rule until a time, which is its shortest.
    var changedForATime: Bool {
        rules.contains { if case .until = $0.length { true } else { false } }
    }

    /// Whether one of its rules says `state`.
    func hasRule(saying state: OnOff) -> Bool { rules.contains { $0.says == state } }
}
