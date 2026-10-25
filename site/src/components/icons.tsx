import type { ReactNode } from "react";
import type { Glyph, Moon } from "../data/menus";

const MOON = "M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z";

/** The menu-bar moon: filled while Night Shift is on, an outline while it's off, and an outline
 *  with a "z" while a rule forces it off. */
export function MoonIcon({ moon }: { moon: Moon }) {
	if (moon === "on") {
		return (
			<svg aria-hidden="true" viewBox="0 0 24 24">
				<path d={MOON} fill="currentColor" />
			</svg>
		);
	}
	return (
		<svg
			aria-hidden="true"
			viewBox="0 0 24 24"
			fill="none"
			stroke="currentColor"
		>
			<path d={MOON} strokeWidth="2" strokeLinejoin="round" />
			{moon === "forced" && (
				<path
					d="M15.5 2.5h4l-4 4.5h4"
					strokeWidth="1.6"
					strokeLinecap="round"
					strokeLinejoin="round"
				/>
			)}
		</svg>
	);
}

// The menu's other images, drawn like the moons: a 24 grid, 2px strokes, round joins.
const GLYPHS: Record<Glyph, ReactNode> = {
	timer: (
		<>
			<circle cx="12" cy="13.5" r="7.5" />
			<path d="M12 10v3.5l2.3 2.3M9.5 2.5h5" />
		</>
	),
	globe: (
		<>
			<circle cx="12" cy="12" r="9" />
			<path d="M3 12h18M12 3c2.4 2.6 3.6 5.6 3.6 9s-1.2 6.4-3.6 9c-2.4-2.6-3.6-5.6-3.6-9S9.6 5.6 12 3z" />
		</>
	),
	gear: (
		<>
			<circle cx="12" cy="12" r="3" />
			<path d="M12 2.8v2.4M12 18.8v2.4M2.8 12h2.4M18.8 12h2.4M5.5 5.5l1.7 1.7M16.8 16.8l1.7 1.7M5.5 18.5l1.7-1.7M16.8 7.2l1.7-1.7" />
			<circle cx="12" cy="12" r="6.2" />
		</>
	),
	power: <path d="M12 3v8M6.3 6.8a8 8 0 1 0 11.4 0" />,
};

export function GlyphIcon({ glyph }: { glyph: Glyph }) {
	return (
		<svg
			aria-hidden="true"
			viewBox="0 0 24 24"
			fill="none"
			stroke="currentColor"
			strokeWidth="2"
			strokeLinecap="round"
			strokeLinejoin="round"
		>
			{GLYPHS[glyph]}
		</svg>
	);
}

/** A submenu's chevron, at the menu's text size. */
export function Chevron() {
	return (
		<svg
			aria-hidden="true"
			viewBox="0 0 8 14"
			fill="none"
			stroke="currentColor"
			strokeWidth="1.7"
			strokeLinecap="round"
			strokeLinejoin="round"
		>
			<path d="M2 2l4.5 5L2 12" />
		</svg>
	);
}

/** The slider's ends, as the app draws them: a small four-rayed sun, a large eight-rayed one. */
export function Sun({ large }: { large?: boolean }) {
	return large ? (
		<svg
			aria-hidden="true"
			viewBox="0 0 24 24"
			fill="none"
			stroke="currentColor"
			strokeWidth="2"
			strokeLinecap="round"
		>
			<circle cx="12" cy="12" r="4.2" />
			<path d="M12 2.5v2.5M12 19v2.5M2.5 12H5M19 12h2.5M5.3 5.3l1.8 1.8M16.9 16.9l1.8 1.8M5.3 18.7l1.8-1.8M16.9 7.1l1.8-1.8" />
		</svg>
	) : (
		<svg
			aria-hidden="true"
			viewBox="0 0 24 24"
			fill="none"
			stroke="currentColor"
			strokeWidth="2.4"
			strokeLinecap="round"
		>
			<circle cx="12" cy="12" r="4" />
			<path d="M12 3v2M12 19v2M3 12h2M19 12h2" />
		</svg>
	);
}

export function AppleLogo({ className }: { className?: string }) {
	return (
		<svg aria-hidden="true" className={className} viewBox="0 0 14 17">
			<path
				d="M11.6 9c0-2 1.6-2.9 1.7-3-1-1.4-2.4-1.6-2.9-1.6-1.2-.1-2.4.7-3 .7-.6 0-1.6-.7-2.6-.7C3.4 4.4 2 5.3 1.3 6.7c-1.4 2.5-.4 6.1 1 8.1.7 1 1.5 2 2.5 2 1 0 1.4-.6 2.6-.6 1.2 0 1.5.6 2.6.6 1.1 0 1.8-1 2.4-2 .8-1.1 1.1-2.2 1.1-2.3 0 0-2.1-.8-2.1-3.3zM9.7 3c.5-.7.9-1.6.8-2.5-.8 0-1.8.5-2.3 1.2-.5.6-.9 1.5-.8 2.4.9.1 1.8-.4 2.3-1.1z"
				fill="currentColor"
			/>
		</svg>
	);
}
