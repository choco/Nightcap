import {
	memo,
	type ReactNode,
	type RefObject,
	useEffect,
	useState,
} from "react";
import { cx } from "../lib/cx";
import {
	type Box,
	type Doodle,
	type DoodleKind,
	type DoodleMotion,
	placeDoodles,
} from "../lib/doodles";
import { phoneQuery } from "../lib/phone";
import { Outline } from "./icons";
import { MOON, SPARKLE, STAR } from "./outlines";

const PATHS: Record<DoodleKind, ReactNode> = {
	star: <path d={STAR} />,
	sparkle: <path d={SPARKLE} />,
	dot: <circle cx="12" cy="12" r="9" fill="currentColor" stroke="none" />,
	moon: <path d={MOON} />,
	sun: (
		<>
			<circle cx="12" cy="12" r="4.4" />
			<path d="M12 2.6v2.6M12 18.8v2.6M2.6 12h2.6M18.8 12h2.6M5.4 5.4l1.9 1.9M16.7 16.7l1.9 1.9M5.4 18.6l1.9-1.9M16.7 7.3l1.9-1.9" />
		</>
	),
};

const MOTIONS: Record<DoodleMotion, string> = {
	twinkle: "animate-twinkle",
	drift: "animate-drift",
	sway: "animate-sway",
	spin: "animate-spin",
};

/** A layer of doodles behind the content. */
export const Doodles = memo(function Doodles({
	doodles,
}: {
	doodles: Doodle[];
}) {
	return (
		<div
			className="pointer-events-none absolute inset-0 z-0"
			aria-hidden="true"
		>
			{doodles.map((d) => (
				<span
					key={`${d.kind}-${d.x}-${d.y}`}
					className="absolute"
					style={{
						left: `${d.x.toFixed(2)}%`,
						top: `${Math.round(d.y)}px`,
						width: `${d.size}px`,
						height: `${d.size}px`,
						transform: `rotate(${d.rot}deg)`,
						color: `var(${d.gold ? "--doodle-gold" : "--doodle"})`,
					}}
				>
					<span
						className={cx(
							"block size-full",
							MOTIONS[d.motion],
							"motion-reduce:animate-none",
						)}
						style={{
							animationDelay: `${d.delay.toFixed(2)}s`,
							animationDuration: `${d.duration.toFixed(1)}s`,
						}}
					>
						<Outline className="block size-full" strokeWidth="1.5">
							{PATHS[d.kind]}
						</Outline>
					</span>
				</span>
			))}
		</div>
	);
});

const box = (r: DOMRect, origin: DOMRect, margin: number): Box => ({
	left: r.left - origin.left - margin,
	right: r.right - origin.left + margin,
	top: r.top - origin.top - margin,
	bottom: r.bottom - origin.top + margin,
});

/** The boxes marked `data-doodle-clear` among `nodes`, each with the margin it gives. */
const cleared = (nodes: HTMLElement[], origin: DOMRect) =>
	nodes.map((node) =>
		box(node.getBoundingClientRect(), origin, Number(node.dataset.doodleClear)),
	);

const same = (a: Doodle[], b: Doodle[]) =>
	a.length === b.length && a.every((d, i) => d === b[i]);

/** The doodles for the hero's layer and the page's, clear of the boxes marked
 *  `data-doodle-clear` with the margin to keep: the hero's of those in the hero, the page's of the
 *  rest. They're placed once the page is laid out, and again when its width changes or the web
 *  fonts reflow the hero; when only the page's height changes, the ones that would now touch
 *  content or fall past its end are left out. On phones, only those marked to keep are drawn. */
export function useDoodles(page: RefObject<HTMLElement | null>) {
	const [doodles, setDoodles] = useState<{ hero: Doodle[]; page: Doodle[] }>({
		hero: [],
		page: [],
	});
	useEffect(() => {
		const root = page.current;
		if (!root) throw new Error("the doodles have no page");
		const hero = root.querySelector<HTMLElement>("[data-hero]");
		if (!hero) throw new Error("the doodles have no hero");
		const marked = [
			...root.querySelectorAll<HTMLElement>("[data-doodle-clear]"),
		];
		const heroMarked = marked.filter((node) => hero.contains(node));
		const pageMarked = marked.filter((node) => !hero.contains(node));
		const phone = phoneQuery();
		// The doodles as last placed, and the hero they were placed around.
		let placed: { around: string; doodles: Doodle[] } | null = null;
		const draw = () => {
			const heroBox = hero.getBoundingClientRect();
			const heroHeight = hero.offsetHeight;
			const pageHeight = root.offsetHeight;
			const clear = cleared(heroMarked, heroBox);
			const around = JSON.stringify([heroBox.width, heroHeight, clear]);
			if (placed?.around !== around)
				placed = {
					around,
					doodles: placeDoodles(pageHeight, {
						width: heroBox.width,
						height: heroHeight,
						clear,
					}),
				};
			const pageBox = root.getBoundingClientRect();
			const content = cleared(pageMarked, pageBox);
			const offContent = (d: Doodle) => {
				const x = (d.x / 100) * pageBox.width;
				return !content.some(
					(r) =>
						x + d.size > r.left &&
						x < r.right &&
						d.y + d.size > r.top &&
						d.y < r.bottom,
				);
			};
			const all = phone.matches
				? placed.doodles.filter((d) => d.keep)
				: placed.doodles;
			const next = {
				hero: all.filter((d) => d.y + d.size < heroHeight),
				page: all.filter(
					(d) =>
						d.y > heroHeight && d.y + d.size < pageHeight - 24 && offContent(d),
				),
			};
			setDoodles((current) =>
				same(current.hero, next.hero) && same(current.page, next.page)
					? current
					: next,
			);
		};
		draw();
		let timer: ReturnType<typeof setTimeout>;
		const redraw = () => {
			clearTimeout(timer);
			timer = setTimeout(draw, 200);
		};
		// The page resizing, or the web fonts reflowing the hero's content.
		const reflow = new ResizeObserver(redraw);
		for (const node of heroMarked) reflow.observe(node);
		reflow.observe(root);
		return () => {
			clearTimeout(timer);
			reflow.disconnect();
		};
	}, [page]);
	return doodles;
}
