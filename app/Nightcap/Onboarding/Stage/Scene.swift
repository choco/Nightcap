import NightcapCore

/// What onboarding's stage shows for a step: a small live mock of macOS that acts out what the
/// step describes (design/README.md, "In the app").
enum Scene: Equatable {
    case strength
    case timeline(Schedule)
    case byHand
    case apps
    case address
    case accessibility(granted: Bool)
    case sites
    case lastThings(app: String)
    case ready
}
