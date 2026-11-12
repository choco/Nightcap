import type { ReactNode } from "react";
import { cx } from "../lib/cx";

/** A capsule link, as the design system's Button, that springs on hover: `primary`, in moon, for
 *  the one action a view is about, and outlined for the rest. `large` is for the download button
 *  in the hero. */
export function ButtonLink({
	href,
	primary,
	large,
	children,
}: {
	href: string;
	primary?: boolean;
	large?: boolean;
	children: ReactNode;
}) {
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
