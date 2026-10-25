// Doodles: the margins get a jittered, staggered grid (one per cell, so no clumps or gaps), the
// centre a few kept far apart, and the hero's open space a scatter around its content. None
// covers the hero's content or a section's. Kinds come from shuffled decks and never repeat a
// neighbour; each kind's sizes cycle through small, medium and large. Seeded, so the layout is
// the same on every visit.

export type DoodleKind = "star" | "sparkle" | "dot" | "moon" | "sun";
export type DoodleMotion = "twinkle" | "drift" | "sway" | "spin";

export interface Doodle {
	kind: DoodleKind;
	/** Across the page, in percent of its width. */
	x: number;
	/** Down the page, in pixels. */
	y: number;
	size: number;
	/** Shown on phones too. */
	keep: boolean;
	rot: number;
	motion: DoodleMotion;
	delay: number;
	duration: number;
	gold: boolean;
}

export interface Box {
	left: number;
	right: number;
	top: number;
	bottom: number;
}

/** Doodles from the top of the page to `bottom`, clear of the hero's content boxes. */
export function placeDoodles(
	bottom: number,
	hero: { width: number; height: number; clear: Box[] },
): Doodle[] {
	let seed = 20261016;
	const rand = () => {
		seed = (seed + 0x6d2b79f5) | 0;
		let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
		t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
		return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
	};
	const between = (a: number, b: number) => a + rand() * (b - a);
	const shuffle = <T>(xs: T[]) => {
		const a = xs.slice();
		for (let i = a.length - 1; i > 0; i--) {
			const j = Math.floor(rand() * (i + 1));
			[a[i], a[j]] = [a[j], a[i]];
		}
		return a;
	};
	const deckOf: DoodleKind[] = [
		"star",
		"star",
		"sparkle",
		"sparkle",
		"dot",
		"dot",
		"moon",
		"sun",
	];
	let deck: DoodleKind[] = [];
	const drawKind = (avoid: DoodleKind[]) => {
		if (deck.length === 0) deck = shuffle(deckOf);
		let i = deck.findIndex((k) => !avoid.includes(k));
		if (i < 0) i = 0;
		return deck.splice(i, 1)[0];
	};
	const sizes: Record<DoodleKind, [number, number]> = {
		star: [12, 24],
		sparkle: [12, 26],
		dot: [4, 8],
		moon: [18, 32],
		sun: [22, 36],
	};
	const tiers: Partial<Record<DoodleKind, number[]>> = {};
	const sizeOf = (kind: DoodleKind) => {
		let tier = tiers[kind];
		if (!tier?.length) {
			tier = shuffle([0.15, 0.5, 0.85]);
			tiers[kind] = tier;
		}
		const [step] = tier.splice(-1);
		const f = Math.min(1, Math.max(0, step + between(-0.1, 0.1)));
		return Math.round(sizes[kind][0] + f * (sizes[kind][1] - sizes[kind][0]));
	};
	const counts: Partial<Record<DoodleKind, number>> = {};
	let placedCount = 0;
	const doodle = (kind: DoodleKind, x: number, y: number): Doodle => {
		const count = (counts[kind] ?? 0) + 1;
		counts[kind] = count;
		const motion: DoodleMotion =
			kind === "sun"
				? "spin"
				: kind === "moon"
					? "sway"
					: count % 5 === 0
						? "drift"
						: "twinkle";
		const duration =
			motion === "spin"
				? between(45, 80)
				: motion === "sway"
					? between(6, 9)
					: motion === "drift"
						? between(8, 11)
						: between(3, 6);
		const gold = kind === "sun" || kind === "moon" || count % 4 === 0;
		return {
			kind,
			x,
			y,
			size: sizeOf(kind),
			keep: placedCount++ % 4 === 0,
			rot: kind === "sun" ? 0 : Math.round(between(-25, 25)),
			motion,
			delay: between(0, 4),
			duration,
			gold,
		};
	};

	const top = 24;
	const cell = 150;
	const columns = [
		[1, 8],
		[8, 15],
		[85, 92],
		[92, 99],
	];
	const placed: Doodle[] = [];
	const above: (DoodleKind | null)[] = columns.map(() => null);
	for (let row = 0; top + row * cell < bottom; row++) {
		let left: DoodleKind | null = null;
		columns.forEach(([x0, x1], c) => {
			const stagger = c % 2 === 1 ? cell / 2 : 0;
			const y0 = top + row * cell + stagger;
			if (y0 > bottom) return;
			const kind = drawKind(
				[above[c], left].filter((k): k is DoodleKind => k !== null),
			);
			// Of a few spots in the cell, the one farthest from every doodle so far.
			let best = { x: 0, y: 0 };
			let bestGap = -1;
			for (let i = 0; i < 10; i++) {
				const x = between(x0 + 0.6, x1 - 0.6);
				const y = between(y0 + cell * 0.1, y0 + cell * 0.9);
				const gap = placed.reduce(
					(g, d) => Math.min(g, Math.hypot(((d.x - x) / 100) * 1440, d.y - y)),
					Infinity,
				);
				if (gap > bestGap) {
					bestGap = gap;
					best = { x, y };
				}
			}
			placed.push(doodle(kind, best.x, best.y));
			above[c] = kind;
			left = kind;
		});
	}
	const centre: { x: number; y: number }[] = [];
	for (let tries = 0; tries < 400 && centre.length < 14; tries++) {
		const x = between(16, 84);
		const y = between(700, bottom);
		if (
			centre.every((d) => Math.hypot(((d.x - x) / 100) * 1440, d.y - y) > 300)
		)
			centre.push({ x, y });
	}
	for (const d of centre) placed.push(doodle(drawKind([]), d.x, d.y));
	// A doodle at (x, y) spans at most 36px, the largest sun.
	const covers = (x: number, y: number) =>
		hero.clear.some(
			(r) => x + 36 > r.left && x < r.right && y + 36 > r.top && y < r.bottom,
		);
	const open = placed.filter(
		(d) => d.y > hero.height || !covers((d.x / 100) * hero.width, d.y),
	);
	const wanted = Math.round((hero.width * hero.height) / (110 * 110));
	for (let tries = 0, added = 0; tries < 4000 && added < wanted; tries++) {
		const px = between(12, hero.width - 12);
		const y = between(12, hero.height - 12);
		const x = (px / hero.width) * 100;
		if (
			covers(px, y) ||
			open.some((d) => Math.hypot(((d.x - x) / 100) * hero.width, d.y - y) < 92)
		)
			continue;
		open.push(doodle(drawKind([]), x, y));
		added++;
	}
	return open;
}
