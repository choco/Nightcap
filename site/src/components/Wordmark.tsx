/** The app icon beside "Nightcap" in Caprasimo, linking to `href`. `compact`, as in the nav, keeps
 *  to a smaller icon alone on phones, leaving the row to the Night Shift switch. */
export function Wordmark({
	href,
	compact,
}: {
	href: string;
	compact?: boolean;
}) {
	return (
		<a
			className="flex items-center gap-3 font-display text-[30px] leading-none text-ink no-underline hover:text-ink"
			href={href}
		>
			<img
				className={compact ? "max-sm:size-9" : undefined}
				src="/icon.svg"
				alt="Nightcap"
				width="44"
				height="44"
			/>
			<span
				className={compact ? "max-sm:hidden" : undefined}
				aria-hidden="true"
			>
				Nightcap
			</span>
		</a>
	);
}
