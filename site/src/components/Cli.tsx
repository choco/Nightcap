import type { ReactNode } from "react";
import { Section, SectionText } from "./Section";

const Command = ({ children }: { children: ReactNode }) => (
	<div className="wrap-anywhere whitespace-pre-wrap">
		<span className="text-moon">$ </span>
		<span className="font-semibold text-starlight">nightcap</span>
		{children}
	</div>
);
const Output = ({ children }: { children: ReactNode }) => (
	<div className="wrap-anywhere whitespace-pre-wrap text-starlight-muted">
		{children}
	</div>
);
// An argument stays whole: a line breaks between them, never at a flag's hyphens.
const Arg = ({ children }: { children: ReactNode }) => (
	<span className="whitespace-nowrap text-moon">{children}</span>
);
const Off = () => <span className="text-[#ffaa6b]">off</span>;

export function Cli() {
	return (
		<Section id="cli" className="flex flex-wrap items-center gap-x-12 gap-y-8">
			<div className="flex flex-[1_1_300px] flex-col gap-3.5">
				<h2>Made to be scripted</h2>
				<SectionText>
					Everything in the menu is also a <code>nightcap</code> command. Bind
					it to a key with skhd, Raycast or Karabiner, run it from a script, or
					read the status as JSON.
				</SectionText>
				<p className="text-[15px] leading-[1.55] text-ink-muted">
					Rules, and Night Shift's strength and schedule, can also live in a
					plain config file, next to the rest of your setup.
				</p>
			</div>
			{/* Long output wraps, as a terminal window would. */}
			<figure
				className="min-w-0 flex-[999_1_490px] overflow-x-auto rounded-[12px] border border-menu-line bg-night-deep p-6 font-mono text-[13px] leading-[1.6] text-starlight shadow-[0_24px_70px_-18px_rgba(246,213,138,0.32)] max-sm:p-[18px] max-sm:text-[11px] max-sm:[font-variation-settings:'wdth'_87.5]"
				data-lens-area
				aria-label="A terminal session with the nightcap command"
			>
				<Command>
					{" "}
					off <Arg>--for 1h30m</Arg>
				</Command>
				<Output>
					Night Shift <Off /> until 01:00
				</Output>
				<Command>
					{" "}
					rules add <Arg>domain:figma.com</Arg>
				</Command>
				<Output>
					Added domain:figma.com, which turns Night Shift off. Night Shift{" "}
					<Off /> until 01:00.
				</Output>
				<Command>
					{" "}
					rules add <Arg>app:com.apple.iBooksX</Arg> <Arg>--on</Arg>
				</Command>
				<Output>
					Added app:com.apple.iBooksX, which turns Night Shift on. Night Shift{" "}
					<Off /> until 01:00.
				</Output>
				<Command> reset</Command>
				<Output>Night Shift following the schedule</Output>
				<Command>
					{" "}
					status <Arg>--json</Arg> | jq -r <Arg>.decision.state</Arg>
				</Command>
				<Output>
					<span className="text-[#30d158]">on</span>
				</Output>
			</figure>
		</Section>
	);
}
