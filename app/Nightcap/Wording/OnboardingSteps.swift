import Foundation
import NightcapCore

/// Onboarding's words: each step's heading and what it says, its controls and choices, its
/// buttons and dots, and what Ready sums up.
extension Wording {
    var welcomeToNightcap: String { text("Welcome to Nightcap") }

    /// The step's group and its place in it, over the heading: "Nightcap · 2 of 3", or "Done" on
    /// Ready.
    func eyebrow(_ place: (part: OnboardingFlow.Part, place: Int, count: Int)?) -> String {
        guard let place else { return text("Done") }
        switch (place.part, place.count > 1) {
        case (.nightShift, false): return nightShift
        case (.nightcap, false): return text("Nightcap")
        case (.nightShift, true): return text("Night Shift · \(place.place + 1) of \(place.count)")
        case (.nightcap, true): return text("Nightcap · \(place.place + 1) of \(place.count)")
        }
    }

    /// A step's heading and what it says under it. `reviewing` is the strength's while the user
    /// looks over Night Shift's settings, and `foundApps` whether the apps step found any.
    func page(
        _ step: OnboardingFlow.Step, _ page: OnboardingFlow.WebsitesPage, reviewing: Bool, foundApps: Bool
    ) -> (heading: String, lede: String) {
        switch (step, page) {
        case (.alreadySetUp, _):
            (
                text("You already use Night Shift"),
                text("Nightcap found your Night Shift settings.") + " "
                    + text("Keep them as they are and go straight to Nightcap, or look over them first.")
            )
        case (.strength, _) where reviewing:
            (
                text("How warm Night Shift gets"),
                text("This is your current strength.") + " " + text("Drag to change it.")
            )
        case (.strength, _):
            (
                text("This is Night Shift"),
                text("Night Shift is part of macOS.") + " "
                    + text("It shifts your screen toward warmer colours, which many people prefer in the evening.")
                    + " " + text("Choose how warm it gets.")
            )
        case (.schedule, _):
            (
                text("When should it come on?"),
                text("Night Shift can come on by itself on a schedule, or only when you turn it on.")
            )
        case (.apps, _):
            (
                text("Night Shift, with exceptions"),
                text("Warm is wrong for photos, video and design.") + " "
                    + (foundApps
                        ? text(
                            "Nightcap turns Night Shift off while an app you tick is in front, and gives it back when you leave."
                        ) + " " + text("These are on your Mac.")
                        : text(
                            "Nightcap turns Night Shift off while an app you choose is in front, and gives it back when you leave."
                        ))
            )
        case (.websites, .ask):
            (
                text("Websites too?"),
                text(
                    "Nightcap can do the same for websites: Night Shift goes off while a site you choose is in the front tab."
                )
            )
        case (.websites, .accessibility):
            (
                text("Allow Accessibility"),
                text(
                    "Nightcap reads the front tab’s address through Accessibility, which macOS asks you to allow once.")
            )
        case (.websites, .sites):
            (
                text("Which websites need true colour?"),
                text("Tick any that do.") + " "
                    + text("You can add any other site later from the menu while it’s open.")
            )
        case (.lastThings, _):
            (
                text("Two last things"),
                text("Both are up to you, and you can change either later in Settings.")
            )
        case (.ready, _):
            (
                text("Nightcap is ready"),
                text("It lives in the menu bar, under the moon.") + " "
                    + text(
                        "Its menu turns Night Shift on or off, everywhere or just in the app in front, for as long as you choose."
                    ) + " " + text("You can change any of this later in Settings, along with the shortcut.")
            )
        }
    }

    /// A footer's button.
    func title(of button: OnboardingFlow.FooterButton) -> String {
        switch button {
        case .continue: text("Continue")
        case .notNow: text("Not Now")
        case .reviewSettings: text("Review Settings")
        case .skipToNightcap: text("Skip to Nightcap")
        case .addRules(let count): count == 0 ? text("Add Rules") : text("Add \(count) Rules")
        case .chooseApp: text("Choose Another App…")
        case .turnOnWebsites: text("Turn On")
        case .allowAccessibility: text("Allow Accessibility…")
        case .waiting: text("Waiting…")
        case .done: text("Done")
        }
    }

    var back: String { text("Back") }

    /// A step's dot, as VoiceOver names it.
    func name(of step: OnboardingFlow.Step) -> String {
        switch step {
        case .alreadySetUp: nightShift
        case .strength: text("Strength")
        case .schedule: text("Schedule")
        case .apps: text("Apps")
        case .websites: text("Websites")
        case .lastThings: text("Notifications and login")
        case .ready: text("Ready")
        }
    }

    /// The current step's dot, as VoiceOver names it: "Apps, current step".
    func current(_ step: String) -> String {
        text("\(step), current step")
    }

    /// A step's mark in a checklist, as VoiceOver names it.
    func step(current: Bool) -> String {
        current ? text("Current step") : text("To do")
    }

    // MARK: Night Shift's steps

    var schedule: String { text("Schedule") }

    /// How warm a strength looks, in words, since System Settings shows no number either.
    enum Warmth {
        case aLittle, warm, very

        init(_ strength: UInt8) {
            self = strength < 34 ? .aLittle : strength < 67 ? .warm : .very
        }
    }

    func warmth(_ strength: UInt8) -> String {
        switch Warmth(strength) {
        case .aLittle: text("A little warm")
        case .warm: text("Warm")
        case .very: text("Very warm")
        }
    }

    /// When Night Shift comes on: "Sunset to sunrise", or custom hours in the user's clock.
    func schedule(_ schedule: Schedule) -> String {
        switch schedule {
        case .off: text("By hand")
        case .sunsetToSunrise: text("Sunset to sunrise")
        case .custom(let from, let to): text("\(time(from)) to \(time(to))")
        }
    }

    var skippingLeavesThem: String {
        text("Skipping leaves them exactly as they are.") + " "
            + text("You can change them any time in System Settings › Displays.")
    }

    var lessWarm: String { text("Less Warm") }
    var moreWarm: String { text("More Warm") }

    /// Under the strength's slider: whether the screen shows the strength it's dragged to.
    func strengthPreview(showing: Bool) -> String {
        showing ? text("Your screen is showing this strength now.") : text("Drag the slider to try it on your screen.")
    }

    /// What the strength step says Night Shift is, each a title over a line.
    var nightShiftFacts: [(title: String, text: String)] {
        [
            (text("Part of macOS"), text("In System Settings › Displays, and in Control Center.")),
            (
                text("Colour, not brightness"),
                text("It warms the colours.") + " " + text("Brightness stays where it is.")
            ),
            (text("By hand or on a schedule"), text("You choose which on the next step.")),
        ]
    }

    var sunsetToSunriseChoice: String { text("On a schedule: sunset to sunrise") }
    var followsYourLocation: String { text("Follows your location") }
    var customHoursChoice: String { text("On a schedule: custom hours") }
    /// Between custom hours' two times.
    var customHoursTo: String { text("to") }
    var byHandChoice: String { text("By hand") }
    var onlyWhenYouTurnItOn: String { text("Only when you turn it on") }
    var eitherWay: String {
        text("Either way, you can turn it on or off any time from Control Center or Nightcap’s menu.")
    }

    // MARK: Nightcap's steps

    /// What a suggested app is for.
    var design: String { text("Design") }
    var photo: String { text("Photo") }
    var videoEditing: String { text("Video editing") }
    var video: String { text("Video") }
    /// An app the user chose for the grid.
    var chosenByYou: String { text("Chosen by you") }

    var foundNoApps: String {
        text(
            "Nightcap looks for apps like Figma, Photoshop, Final Cut Pro and DaVinci Resolve, and found none on this Mac."
        )
            + " " + text("Choose one yourself, or add one later from the menu while it’s in front.")
    }

    var chooseApp: String { text("Choose App…") }
    var choose: String { text("Choose") }
    var chooseAnAppThatNeedsTrueColour: String { text("Choose an app that needs true colour.") }

    /// What the websites step says website checking does, each a few words in bold, then the
    /// rest.
    var websiteFacts: [(symbol: String, lead: String, rest: String)] {
        [
            (
                "globe", text("Safari, Chrome, Firefox and more."),
                text("Also Edge, Arc, Brave, Dia, Zen and Orion, in the front tab.")
            ),
            ("accessibility", text("Through Accessibility."), text("macOS asks you to allow it, once.")),
            (
                "lock", text("Only the site’s name."),
                text("Like youtube.com; the rest of the address is dropped, and the icon comes from the site.")
            ),
        ]
    }

    /// The Accessibility page's checklist, in order.
    var accessibilitySteps: [String] {
        [
            text("Click Allow Accessibility, then Open System Settings when macOS asks."),
            text("Switch Nightcap on under Privacy & Security › Accessibility."),
            text("That’s all.") + " " + text("This step moves on by itself the moment Nightcap is allowed."),
        ]
    }

    var tellMeWhenARuleIsSnoozed: String { text("Tell me when a rule is snoozed") }

    /// What notifications tell, and where they're switched off if macOS turned them down.
    func whatNotificationsTell(switchedOff: Bool) -> String {
        let detail = text(
            "If you turn Night Shift on in Control Center while a rule holds it off, Nightcap snoozes the rule until the schedule’s next change, and says so in a notification."
        )
        return switchedOff
            ? detail + " " + text("Notifications are switched off for Nightcap in System Settings.") : detail
    }

    var openNotificationSettings: String { text("Open Notification Settings…") }

    var openAtLoginDetail: String {
        text("So your rules apply from the start.") + " " + text("Nightcap lives in the menu bar, with no Dock icon.")
    }

    // MARK: Ready

    var apps: String { text("Apps") }
    var needsAccessibility: String { text("Needs Accessibility") }

    /// A choice Ready sums up, on or off.
    func onOrOff(_ on: Bool) -> String {
        on ? text("On") : text("Off")
    }

    /// The rules given here for apps: "None", "1 rule", "2 rules".
    func appRules(_ count: Int) -> String {
        count == 0 ? text("None") : text("\(count) rules")
    }

    /// The rules given here for sites: "1 site", "2 sites".
    func siteRules(_ count: Int) -> String {
        count == 0 ? text("None") : text("\(count) sites")
    }
}
