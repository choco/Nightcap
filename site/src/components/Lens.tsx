import { type RefObject, useEffect, useRef } from "react";
import { phoneQuery } from "../lib/phone";
import styles from "./Lens.module.css";

// The sway, across and down at different paces, so the circle traces a slow figure.
const SWAY = [
	{ axis: "X", share: "width", duration: 5700 },
	{ axis: "Y", share: "height", duration: 7850 },
] as const;

/** The page's Night Shift tint and the ring around the true-colour circle cut out of it.
 *
 *  The circle follows the pointer, staying under it through scrolling. After 2.5 s still, it
 *  wanders over the lens area (`data-lens-area`) of the section (`data-lens-section`) most in
 *  view, or, on phones, which have no pointer to steer it, over the page's first lens area, the
 *  hero's scene. Following, Lens eases the circle towards the pointer each frame until it
 *  arrives. Wandering, it glides there and sways, 32% of the area either side, as animations the
 *  browser runs by itself, so a page at rest does no work for it. While the page's Night Shift is
 *  off, and on phones while the scene is out of view, there's nothing to see, and Lens rests. */
export function Lens({
	page,
	shiftOn,
}: {
	page: RefObject<HTMLElement | null>;
	shiftOn: boolean;
}) {
	const tint = useRef<HTMLDivElement>(null);
	const ring = useRef<HTMLDivElement>(null);
	const hole = useRef<HTMLDivElement>(null);
	const shift = useRef<(on: boolean) => void>(undefined);

	useEffect(() => {
		const root = page.current;
		const tinted = tint.current;
		const ringed = ring.current;
		const cutout = hole.current;
		if (!root || !tinted || !ringed || !cutout)
			throw new Error("the lens has no page");
		const circles = [tinted, ringed];
		const firstArea = root.querySelector("[data-lens-area]");
		if (!firstArea) throw new Error("the lens has no area");
		const phone = phoneQuery();
		const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)");
		const written = new Map<string, string>();
		const set = (values: Record<string, number>) => {
			for (const [name, number] of Object.entries(values)) {
				const value = `${number.toFixed(1)}px`;
				if (written.get(name) === value) continue;
				written.set(name, value);
				for (const circle of circles) circle.style.setProperty(name, value);
			}
		};
		// Steered, the circles move only as Lens moves them, without a glide (Lens.module.css).
		const steer = (on: boolean) => {
			for (const circle of circles) circle.toggleAttribute("data-steered", on);
		};
		// Where a box's centre is on the page, and its size.
		const centre = (element: Element) => {
			const box = root.getBoundingClientRect();
			const r = element.getBoundingClientRect();
			return {
				x: r.left + r.width / 2 - box.left,
				y: r.top + r.height / 2 - box.top,
				width: r.width,
				height: r.height,
			};
		};
		const where = () => centre(cutout);

		// Whether there's anything to see: the page's Night Shift on, and on phones the scene in view.
		let nightShift = true;
		let sceneSeen = true;
		let sways: Animation[] = [];
		const seen = () => {
			const showing = nightShift && (!phone.matches || sceneSeen);
			for (const sway of sways) {
				if (showing) sway.play();
				else sway.pause();
			}
		};
		const still = () => {
			for (const sway of sways) sway.cancel();
			sways = [];
		};
		const sway = (size: { width: number; height: number }) => {
			still();
			if (reducedMotion.matches) return;
			for (const circle of circles) {
				let element: HTMLElement = circle;
				for (const { axis, share, duration } of SWAY) {
					const child = element.firstElementChild;
					if (!(child instanceof HTMLElement))
						throw new Error("the lens has no sway");
					element = child;
					const reach = size[share] * 0.32;
					sways.push(
						element.animate(
							[
								{ transform: `translate${axis}(${-reach}px)` },
								{ transform: `translate${axis}(${reach}px)` },
							],
							{
								duration,
								// Halfway, where the sway passes the middle, so it starts there.
								delay: -duration / 2,
								iterations: Number.POSITIVE_INFINITY,
								direction: "alternate",
								easing: "cubic-bezier(0.37, 0, 0.63, 1)",
							},
						),
					);
				}
			}
			seen();
		};

		// The pointer while it moves, and for 2.5 s after; and where the circle is while it follows.
		let pointer: { x: number; y: number } | null = null;
		let at = { x: 0, y: 0 };
		let resting: ReturnType<typeof setTimeout> | undefined;
		let frame = 0;
		let last = 0;
		const step = (now: number) => {
			frame = 0;
			if (!pointer) return;
			const box = root.getBoundingClientRect();
			const target = { x: pointer.x - box.left, y: pointer.y - box.top };
			// A tenth of the way each sixtieth of a second, at any frame rate.
			const ease = reducedMotion.matches
				? 1
				: 1 - 0.9 ** (((now - last) / 1000) * 60);
			last = now;
			at = {
				x: at.x + (target.x - at.x) * ease,
				y: at.y + (target.y - at.y) * ease,
			};
			set({ "--lx": at.x, "--ly": at.y });
			if (Math.hypot(target.x - at.x, target.y - at.y) > 0.5)
				frame = requestAnimationFrame(step);
		};
		const chase = () => {
			if (frame) return;
			last = performance.now();
			frame = requestAnimationFrame(step);
		};

		const mostVisibleArea = () => {
			const height = window.innerHeight;
			let best: Element | null = null;
			let bestShare = 0;
			for (const section of root.querySelectorAll("[data-lens-section]")) {
				const r = section.getBoundingClientRect();
				const shown = Math.max(
					0,
					Math.min(r.bottom, height) - Math.max(r.top, 0),
				);
				const share = r.height > 0 ? shown / r.height : 0;
				if (share > bestShare + 0.001) {
					bestShare = share;
					best = section.matches("[data-lens-area]")
						? section
						: section.querySelector("[data-lens-area]");
				}
			}
			return best;
		};
		const areaInView = () => (phone.matches ? firstArea : mostVisibleArea());

		// The area the circle wanders over, once it does, and where it was when the circle got there.
		let area: Element | null = null;
		let spot = { x: 0, y: 0, width: 0, height: 0 };
		const wander = (glide: boolean) => {
			if (!nightShift) return;
			const next = areaInView();
			if (!next) return;
			area = next;
			spot = centre(next);
			steer(true);
			if (glide) {
				// From where the sway has it now, so stilling the sway doesn't move it.
				const from = where();
				still();
				set({ "--lx": from.x, "--ly": from.y });
				getComputedStyle(tinted).transform;
				steer(false);
			}
			set({ "--lx": spot.x, "--ly": spot.y });
			sway(spot);
			if (!glide) {
				getComputedStyle(tinted).transform;
				steer(false);
			}
		};
		const rest = () => {
			clearTimeout(resting);
			cancelAnimationFrame(frame);
			frame = 0;
			pointer = null;
			wander(true);
		};
		const move = (event: PointerEvent) => {
			if (event.pointerType !== "mouse" || !nightShift) return;
			if (!pointer) {
				at = where();
				area = null;
				steer(true);
				still();
				set({ "--lx": at.x, "--ly": at.y });
			}
			pointer = { x: event.clientX, y: event.clientY };
			chase();
			clearTimeout(resting);
			resting = setTimeout(rest, 2500);
		};
		const scroll = () => {
			if (pointer) chase();
			else if (areaInView() !== area) wander(true);
		};

		shift.current = (on) => {
			if (on === nightShift) return;
			nightShift = on;
			if (on) wander(true);
			else {
				clearTimeout(resting);
				cancelAnimationFrame(frame);
				frame = 0;
				pointer = null;
			}
			seen();
		};

		// The first place is taken at once, rather than glided to from the prerendered one.
		wander(false);

		// A page that changes size may have moved the area, or changed which one is in view.
		const resize = new ResizeObserver(() => {
			if (pointer) return chase();
			const next = areaInView();
			if (!next) return;
			const now = centre(next);
			const moved = (["x", "y", "width", "height"] as const).some(
				(key) => Math.abs(now[key] - spot[key]) > 0.5,
			);
			if (next !== area || moved) wander(true);
		});
		resize.observe(root);
		const scene = new IntersectionObserver(([entry]) => {
			sceneSeen = entry.isIntersecting;
			seen();
		});
		scene.observe(firstArea);
		root.addEventListener("pointermove", move);
		root.addEventListener("pointerleave", rest);
		window.addEventListener("scroll", scroll, { passive: true });
		return () => {
			clearTimeout(resting);
			cancelAnimationFrame(frame);
			still();
			resize.disconnect();
			scene.disconnect();
			root.removeEventListener("pointermove", move);
			root.removeEventListener("pointerleave", rest);
			window.removeEventListener("scroll", scroll);
		};
	}, [page]);

	useEffect(() => {
		if (!shift.current) throw new Error("the lens isn't set up");
		shift.current(shiftOn);
	}, [shiftOn]);

	return (
		<>
			<div ref={tint} className={styles.tint} aria-hidden="true">
				<div className={styles.sway}>
					<div className={`${styles.sway} ${styles.pieces}`}>
						<div ref={hole} className={styles.hole} />
						<div className={styles.above} />
						<div className={styles.below} />
						<div className={styles.left} />
						<div className={styles.right} />
					</div>
				</div>
			</div>
			<div ref={ring} className={styles.ring} aria-hidden="true">
				<div className={styles.sway}>
					<div className={styles.sway}>
						<div className={styles.circle} />
					</div>
				</div>
			</div>
		</>
	);
}
