import type { HTMLAttributes, ReactNode } from "react";
import { cx } from "../lib/cx";

export type TerminalProps = HTMLAttributes<HTMLElement> & {
	children: ReactNode;
};

/** A terminal session with the `nightcap` command: a night panel in either theme, glowing gold.
 *  Each line is a `Command` or its `Output`, and long ones wrap, as a terminal window's would. */
export function Terminal({ className, children, ...figure }: TerminalProps) {
	return (
		<figure
			className={cx(
				"night overflow-x-auto rounded-md bg-night-deep p-6 font-mono text-code text-ink shadow-glow inset-ring inset-ring-starlight/8 max-sm:p-[18px] max-sm:text-[11px] max-sm:[font-variation-settings:'wdth'_87.5]",
				className,
			)}
			{...figure}
		>
			{children}
		</figure>
	);
}

/** A command line: the prompt, `nightcap` and what follows it. */
export const Command = ({ children }: { children: ReactNode }) => (
	<div className="wrap-anywhere whitespace-pre-wrap">
		<span className="text-moon">$ </span>
		<span className="font-semibold">nightcap</span>
		{children}
	</div>
);

/** What a command printed. */
export const Output = ({ children }: { children: ReactNode }) => (
	<div className="wrap-anywhere whitespace-pre-wrap text-ink-muted">
		{children}
	</div>
);

/** An argument, in gold. It stays whole: a line breaks between arguments, never at a flag's
 *  hyphens. */
export const Arg = ({ children }: { children: ReactNode }) => (
	<span className="whitespace-nowrap text-moon">{children}</span>
);

/** Night Shift's state in output: "off" warm, "on" in the check mark's green. */
export const State = ({ on }: { on?: boolean }) =>
	on ? (
		<span className="text-ok">on</span>
	) : (
		<span className="text-ember-text">off</span>
	);
