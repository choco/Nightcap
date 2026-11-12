import type { ReactNode } from "react";
import { cx } from "../lib/cx";

export interface ButtonProps {
	href: string;
	primary?: boolean;
	large?: boolean;
	children: ReactNode;
}

/** A capsule link that springs on hover: `primary`, in moon with a gold edge, for the one action a
 *  view is about, and outlined for the rest. `large` is for the download button in the hero. */
export function Button({ href, primary, large, children }: ButtonProps) {
	return (
		<a
			className={cx(
				"inline-flex items-center gap-2 rounded-full border font-semibold no-underline transition-transform duration-[420ms] ease-spring hover:scale-[1.045] active:scale-[.97] motion-reduce:transition-none",
				large ? "h-13 px-8 text-[17px]" : "h-10 px-6 text-[15px]",
				primary
					? "border-moon-text bg-moon text-on-moon hover:text-on-moon"
					: "border-line-strong text-ink hover:text-ink",
			)}
			href={href}
		>
			{children}
		</a>
	);
}
