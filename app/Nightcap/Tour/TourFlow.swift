import NightcapCore

/// The tour's progression, separate from its panel and macOS (ADR-0016). A trial's begin is
/// asynchronous: Continue waits for its snapshot before the sample opens or another step starts.
struct TourFlow {
    enum Step { case moon, rule, snooze, site, peek, end }
    enum Try { case tap, hold }

    struct Milestones {
        var returned = false
        var ruleBack = false
        var siteLeft = false
    }

    enum Effect: Equatable {
        case beginTrial(standsIn: Bool)
        case openSample, closeSample, openSite
        case finish(begun: Bool)
    }

    private enum State {
        case introduction(standsIn: Bool)
        case awaitingTrial(standsIn: Bool)
        case running(Step, TourFacts)
        case finished
    }

    private let sample: String
    private let site: String?
    private let steps: [Step]
    private var state = State.introduction(standsIn: false)
    private(set) var milestones = Milestones()
    private(set) var keysDown = false
    private(set) var lastTry: Try?

    init(sample: String, site: String?, hasShortcut: Bool) {
        self.sample = sample
        self.site = site
        steps = [.moon, .rule, .snooze] + (site == nil ? [] : [.site]) + (hasShortcut ? [.peek] : []) + [.end]
    }

    var step: Step {
        switch state {
        case .introduction, .awaitingTrial: .moon
        case .running(let step, _): step
        case .finished: .end
        }
    }

    var facts: TourFacts? {
        if case .running(_, let facts) = state { facts } else { nil }
    }

    var standsIn: Bool {
        switch state {
        case .introduction(let standsIn), .awaitingTrial(let standsIn): standsIn
        case .running(_, let facts): facts.standsIn
        case .finished: false
        }
    }

    var canAdvance: Bool {
        switch state {
        case .introduction: true
        case .running(let step, _): step != .end
        case .awaitingTrial, .finished: false
        }
    }

    var position: Int { (steps.firstIndex(of: step) ?? 0) + 1 }
    var stepCount: Int { steps.count - 1 }
    var usesSample: Bool { step == .rule || step == .snooze }

    var done: Bool {
        switch step {
        case .moon, .end: true
        case .rule: milestones.returned
        case .snooze: milestones.ruleBack
        case .site: milestones.siteLeft
        case .peek: lastTry != nil
        }
    }

    mutating func refresh(_ snapshot: Snapshot) -> [Effect] {
        switch state {
        case .introduction:
            state = .introduction(standsIn: TourFacts.asksForStandIn(snapshot))
        case .awaitingTrial:
            if let facts = TourFacts(snapshot, sample: sample, site: site) {
                state = .running(.rule, facts)
                return [.openSample]
            }
        case .running(let step, let before):
            guard let after = TourFacts(snapshot, sample: sample, site: site) else { return end() }
            let left = { (place: TourPlace?, now: TourPlace?) in
                place?.inFront == true && place?.rule == .off && now?.inFront == false
            }
            switch step {
            case .rule where left(before.sample, after.sample): milestones.returned = true
            case .snooze where before.sample.snoozed && !after.sample.snoozed && after.sample.rule == .off:
                milestones.ruleBack = true
            case .site where left(before.site, after.site): milestones.siteLeft = true
            default: break
            }
            state = .running(step, after)
        case .finished: break
        }
        return []
    }

    mutating func advance() -> [Effect] {
        switch state {
        case .introduction(let standsIn):
            state = .awaitingTrial(standsIn: standsIn)
            return [.beginTrial(standsIn: standsIn)]
        case .running(let step, let facts):
            guard let index = steps.firstIndex(of: step), index + 1 < steps.count else { return [] }
            var next = steps[index + 1]
            // Snoozing needs the rule the step before asks for.
            if next == .snooze, facts.sample.rule != .off { next = steps[index + 2] }
            let hadSample = usesSample
            state = .running(next, facts)
            var effects: [Effect] = hadSample && !usesSample ? [.closeSample] : []
            if next == .site { effects.append(.openSite) }
            return effects
        case .awaitingTrial, .finished: return []
        }
    }

    /// Ends once, including while BeginTrial is still in flight: EndTrial must follow it.
    mutating func end() -> [Effect] {
        let begun: Bool
        switch state {
        case .introduction: begun = false
        case .awaitingTrial, .running: begun = true
        case .finished: return []
        }
        state = .finished
        keysDown = false
        return [.closeSample, .finish(begun: begun)]
    }

    mutating func shortcutPressed() {
        keysDown = true
    }

    mutating func tried(_ kind: Try) {
        keysDown = false
        if step == .peek { lastTry = kind }
    }

    #if DEBUG
    /// A fixed panel for the dev build's renderer, without carrying out any actions.
    mutating func stage(_ step: Step, facts: TourFacts, milestones: Milestones) {
        state = .running(step, facts)
        self.milestones = milestones
    }
    #endif
}
