import type { HTMLAttributes, ReactNode } from "react";
import { cx } from "../lib/cx";

/** One of the page's sections, ruled off from the one above. */
export function Section({ className, ...rest }: HTMLAttributes<HTMLElement>) {
	return (
		<section
			className={cx("border-t border-line py-18", className)}
			data-lens-section
			{...rest}
		/>
	);
}

export function SectionText({
	className,
	children,
}: {
	className?: string;
	children: ReactNode;
}) {
	return (
		<p
			className={cx(
				"max-w-[540px] text-[17px] leading-[1.6] text-ink-muted",
				className,
			)}
		>
			{children}
		</p>
	);
}

/** A section's headline beside its text, the text dropping under it where they don't fit. */
export function SplitHeading({
	title,
	children,
}: {
	title: string;
	children: ReactNode;
}) {
	return (
		<div className="flex flex-wrap items-start gap-x-12 gap-y-3">
			<h2 className="flex-[1_1_280px]">{title}</h2>
			<SectionText className="flex-[1_1_420px]">{children}</SectionText>
		</div>
	);
}
