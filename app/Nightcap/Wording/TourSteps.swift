import Foundation
import NightcapCore

/// The tour's panel (ADR-0016): each step's title and what it says, quoting the menu's items as
/// `quoted` words them, then the moon's legend, the peek's hints and the panel's own labels.
extension Wording {
    /// A step's title, and what it says under it.
    typealias Said = (title: String, body: String)

    /// The first step, on the moon.
    var theMoonIsNightcap: Said {
        (text("The moon in the menu bar is Nightcap"), text("It shows what Night Shift is doing right now."))
    }

    /// Under the first step while Night Shift is off, which the tour turns on.
    var onForTheTourOnceYouContinue: String {
        text("Night Shift is off right now.") + " "
            + text("When you continue, Nightcap turns it on for the tour, and back off once it ends.")
    }

    /// The moon's looks, as the first step lists them: each with its name and what it means.
    var moonLegend: [(moon: Menu.Moon, name: String, meaning: String)] {
        [
            (.on, text("On"), text("Night Shift is warming the screen.")),
            (.off, text("Off"), text("True colour, from the schedule or from you.")),
            (.forcedOff, text("Forced off"), text("Nightcap is holding it off for an app or a site.")),
        ]
    }

    /// The rule step, until the sample is in front.
    func tryARule(_ sample: String) -> Said {
        (text("Try a rule"), text("Bring \(sample) to the front: click its window, or its icon in the Dock."))
    }

    /// The rule step with the sample in front, quoting its row's Always.
    func giveItARule(_ always: String) -> Said {
        (text("Give it a rule"), text("Click the moon to open the menu, then choose \(always)."))
    }

    /// The rule step once the sample has its rule, in front.
    var trueColourInTheSample: Said {
        (
            text("That’s true colour"),
            text("Night Shift is off while the sample is in front, and the moon shows a “z”.") + " "
                + text("Now click another window, or the desktop.")
        )
    }

    /// The rule step once the user left the sample with its rule.
    var itComesBack: Said {
        (
            text("And it comes back"),
            text("Leave the sample and Night Shift returns, as warm as before.") + " "
                + text("Every rule works this way.")
        )
    }

    /// The snooze step, until the sample is back in front.
    func snoozeARule(_ sample: String) -> Said {
        (
            text("Snooze a rule"),
            text("A rule can take a break.") + " "
                + text("Bring \(sample) back to the front: click its window, or its icon in the Dock.")
        )
    }

    /// The snooze step with the sample in front under its rule, quoting its row.
    func snoozeIt(_ row: String) -> Said {
        (
            text("Snooze it"),
            text("The rule keeps Night Shift off here.") + " "
                + text("To give it a break, click the moon, then choose \(row) and how long for.")
        )
    }

    /// The snooze step with the rule snoozed in front, quoting what puts it back.
    func thatsASnooze(_ putBack: String) -> Said {
        (
            text("That’s a snooze"),
            text("Night Shift is back in the sample until the time you chose, then the rule returns by itself.") + " "
                + text("To end it sooner, choose \(putBack).")
        )
    }

    /// The snooze step with the rule snoozed and the sample away.
    func putTheRuleBack(_ sample: String, _ putBack: String) -> Said {
        (text("Put the rule back"), bringItBack(sample, putBack))
    }

    /// The snooze step without the rule, with the sample in front, quoting its row's Always.
    func snoozingNeedsARule(_ always: String) -> Said {
        (text("Snoozing needs a rule"), text("Choose \(always) to give the sample its rule again."))
    }

    /// The snooze step without the rule, and the sample away.
    func snoozingNeedsARule(_ sample: String, _ always: String) -> Said {
        (text("Snoozing needs a rule"), bringItBack(sample, always))
    }

    /// The snooze step once the rule came back, quoting Always in `example`'s row with Night Shift
    /// off.
    func theRulesBack(_ always: String, in example: String) -> Said {
        (
            text("And the rule’s back"),
            text("True colour again in the sample.") + " "
                + text("Rules work the other way too: while Night Shift is off, \(always) keeps it on in \(example).")
        )
    }

    /// The site step, until `browser` is in front on the site.
    func nowASite(_ browser: String) -> Said {
        (text("Now a site"), text("Bring \(browser) back to the front, on Nightcap’s site."))
    }

    /// The site step with `browser` in front on the site, quoting its row's Always.
    func nowASite(_ browser: String, _ always: String) -> Said {
        (
            text("Now a site"),
            text("\(browser) has Nightcap’s site open.") + " " + text("Click the moon, then choose \(always).")
        )
    }

    /// The site step once the site has its rule, in front.
    var trueColourOnOneSite: Said {
        (
            text("That’s true colour, on one site"),
            text("Night Shift is off while its tab is in front.") + " " + text("Now switch to another tab.")
        )
    }

    /// The site step once the user left the site's tab with its rule.
    var sitesWorkTheSameWay: Said {
        (
            text("Sites work the same way"),
            text("Leave the site’s tab and Night Shift returns.") + " "
                + text("A site’s rule covers all its pages, in Safari, Chrome, Firefox and more.")
        )
    }

    /// The peek step, with the shortcut's `keys`.
    func peekAtTrueColour(_ keys: String) -> Said {
        (
            text("Peek at true colour"),
            text("Hold \(keys) in any app to see true colour for as long as you hold it.") + " "
                + text("Tap it to turn Night Shift off, and again to turn it back on.")
        )
    }

    /// The last step.
    var thatsTheTour: Said {
        (
            text("That’s the tour"),
            text("What you changed during it goes with it, and your own rules are as they were.") + " "
                + text(
                    """
                    Everything else is in the moon’s menu: turning Night Shift on or off, everywhere or just in \
                    the app in front, for as long as you choose, and Settings, where you’ll find the tour again.
                    """)
        )
    }

    /// Under the peek's keycaps while they're held.
    var trueColourWhileYouHold: String {
        text("True colour while you hold.") + " " + text("Let go and the warmth comes back.")
    }

    /// Under the keycaps while a tap has Night Shift off.
    var tapToTurnItBackOn: String {
        text("Night Shift is off.") + " " + text("Tap the shortcut to turn it back on.")
    }

    /// Under the keycaps after a hold.
    var thatWasAPeek: String {
        text("That was a peek.") + " " + text("Tap it instead to turn Night Shift off.")
    }

    /// Under the keycaps after a tap that left Night Shift on.
    var nowTryHolding: String {
        text("Night Shift is back on.") + " " + text("Now try holding it.")
    }

    /// Under the keycaps before the first try.
    var holdTheKeys: String { text("Hold the keys on your keyboard, or here.") }

    /// The keycaps, as VoiceOver describes what they do.
    var keyCapsHelp: String { text("Hold to see true colour, tap to turn Night Shift off or on") }

    /// Where a step is in the tour: "2 of 5".
    func step(_ number: Int, of count: Int) -> String { text("\(number) of \(count)") }

    /// The last step's label, in place of where it is.
    var tour: String { text("Tour") }

    var endTour: String { text("End Tour") }

    var skip: String { text("Skip") }

    /// Once a step has shown what it's for.
    var continueTour: String { text("Continue") }

    var done: String { text("Done") }

    /// "Bring Tour Sample back to the front, then choose …".
    private func bringItBack(_ sample: String, _ choice: String) -> String {
        text("Bring \(sample) back to the front, then choose \(choice).")
    }
}
