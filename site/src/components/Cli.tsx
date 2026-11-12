import { Section, SectionText } from "./Section";
import { Arg, Command, Output, State, Terminal } from "./Terminal";

export function Cli() {
	return (
		<Section id="cli" className="flex flex-wrap items-center gap-x-12 gap-y-8">
			<div
				className="flex flex-[1_1_300px] flex-col gap-3.5"
				data-doodle-clear="12"
			>
				<h2>Made to be scripted</h2>
				<SectionText>
					Everything in the menu is also a <code>nightcap</code> command. Bind
					it to a key with skhd, Raycast or Karabiner, run it from a script, or
					read the status as JSON.
				</SectionText>
				<p className="text-small text-ink-muted">
					Rules, and Night Shift's strength and schedule, can also live in a
					plain config file, next to the rest of your setup.
				</p>
			</div>
			<Terminal
				className="min-w-0 flex-[999_1_490px]"
				data-lens-area
				data-doodle-clear="12"
				aria-label="A terminal session with the nightcap command"
			>
				<Command>
					{" "}
					off <Arg>--for 1h30m</Arg>
				</Command>
				<Output>
					Night Shift <State /> until 01:00
				</Output>
				<Command>
					{" "}
					rules add <Arg>domain:figma.com</Arg>
				</Command>
				<Output>
					Added domain:figma.com, which turns Night Shift off. Night Shift{" "}
					<State /> until 01:00.
				</Output>
				<Command>
					{" "}
					rules add <Arg>app:com.apple.iBooksX</Arg> <Arg>--on</Arg>
				</Command>
				<Output>
					Added app:com.apple.iBooksX, which turns Night Shift on. Night Shift{" "}
					<State /> until 01:00.
				</Output>
				<Command> reset</Command>
				<Output>Night Shift following the schedule</Output>
				<Command>
					{" "}
					status <Arg>--json</Arg> | jq -r <Arg>.front.decision.state</Arg>
				</Command>
				<Output>
					<State on />
				</Output>
			</Terminal>
		</Section>
	);
}
