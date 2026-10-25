import { type ReactNode, useEffect, useRef, useState } from "react";
import { cx } from "../lib/cx";
import { Mini } from "./Mini";
import { Section, SplitHeading } from "./Section";

export type Peek = "peeking" | "returning" | null;

type Move = "tap" | "hold";

interface Props {
	shiftOn: boolean;
	onShift: () => void;
	onPeek: (peek: (current: Peek) => Peek) => void;
}

/** The shortcut: one set of keys under two cards, telling a tap from a hold as the app does.
 *  Until pressed, the keys act it out on the cards: a tap, a tap back, then a hold. Pressed, the
 *  press lifts the page's warmth at once, and a release within 0.3 s was a tap, which switches
 *  the page's Night Shift as the nav's switch does. The card for the move that happened lights. */
export function Shortcut({ shiftOn, onShift, onPeek }: Props) {
	const [tapOn, setTapOn] = useState(true);
	const [holding, setHolding] = useState(false);
	const [lit, setLit] = useState<Move | null>(null);
	const [down, setDown] = useState(false);
	const [hint, setHint] = useState(
		"Try the shortcut: tap the keys, then hold them.",
	);
	const timers = useRef<ReturnType<typeof setTimeout>[]>([]);
	const demoing = useRef(false);
	const pressedAt = useRef<number | null>(null);
	const peeking = useRef(false);
	const lastMove = useRef<Move | null>(null);

	useEffect(() => {
		if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) return;
		demoing.current = true;
		const at = (ms: number, run: () => void) =>
			timers.current.push(setTimeout(run, ms));
		const demo = () => {
			at(0, () => setDown(true));
			at(140, () => {
				setDown(false);
				setLit("tap");
				setTapOn(false);
			});
			at(1700, () => setDown(true));
			at(1840, () => {
				setDown(false);
				setTapOn(true);
			});
			at(3300, () => {
				setDown(true);
				setLit("hold");
				setHolding(true);
			});
			at(5500, () => {
				setDown(false);
				setHolding(false);
			});
			at(6800, () => {
				setLit(null);
				timers.current = [];
				demo();
			});
		};
		demo();
		return () => timers.current.forEach(clearTimeout);
	}, []);

	function press() {
		if (pressedAt.current !== null) return;
		if (demoing.current) {
			demoing.current = false;
			timers.current.forEach(clearTimeout);
			setLit(null);
			setTapOn(shiftOn);
		}
		pressedAt.current = performance.now();
		setDown(true);
		if (!shiftOn) return;
		setHolding(true);
		peeking.current = true;
		onPeek(() => "peeking");
		setHint("True colour while you hold. Let go and the warmth comes back.");
	}

	function release() {
		if (pressedAt.current === null) return;
		const held = performance.now() - pressedAt.current;
		pressedAt.current = null;
		setDown(false);
		setHolding(false);
		const peeked = peeking.current;
		peeking.current = false;
		onPeek(() => null);
		if (held < 300) {
			const on = !shiftOn;
			onShift();
			setTapOn(on);
			setLit("tap");
			setHint(
				on
					? "A tap: Night Shift is back on. Now hold the keys."
					: "A tap: Night Shift is off, page and all. Tap again to turn it back on.",
			);
			lastMove.current = "tap";
		} else if (peeked) {
			onPeek(() => "returning");
			setTimeout(
				() => onPeek((current) => (current === "returning" ? null : current)),
				300,
			);
			setLit("hold");
			setHint(
				lastMove.current === "tap"
					? "A hold: a peek, and Night Shift stayed on."
					: "A hold: a peek. Now tap the keys instead.",
			);
			lastMove.current = "hold";
		} else {
			setHint(
				"Night Shift is off, so there was nothing to peek past. Tap to turn it back on.",
			);
		}
	}

	return (
		<Section id="shortcut" className="flex flex-col gap-8">
			<SplitHeading title="One shortcut, two moves">
				Set a shortcut in Settings, <kbd>⌃⌥⌘N</kbd> to start with, and it works
				from any app. A tap switches Night Shift. A hold is a peek: true colour
				until you let go, then the warmth comes straight back.
			</SplitHeading>
			<div className="grid grid-cols-[repeat(auto-fit,minmax(280px,1fr))] gap-6">
				<MoveCard
					lit={lit === "tap"}
					mini={<Mini warm={tapOn} moon={tapOn ? "on" : "off"} />}
					title="Tap"
					glyph={
						<>
							<rect x="5" y="10" width="14" height="10.5" rx="3" />
							<path d="M12 3v3.4M6.6 4.6l1.6 2.6M17.4 4.6l-1.6 2.6" />
						</>
					}
				>
					Night Shift turns off, and stays off until you tap again.
				</MoveCard>
				<MoveCard
					lit={lit === "hold"}
					mini={<Mini warm={!holding} moon="on" />}
					title="Hold"
					glyph={
						<>
							<path d="M12 2.5a9.5 9.5 0 1 1-9.5 9.5" />
							<rect x="8" y="8" width="8" height="8" rx="2" />
						</>
					}
				>
					True colour while you hold. Night Shift stays on, and the moon says
					so.
				</MoveCard>
			</div>
			<div className="flex flex-col items-center gap-3.5">
				<button
					className="cursor-pointer touch-none rounded-[18px] border border-line bg-surface px-[18px] py-3.5 shadow-lift select-none"
					type="button"
					aria-label="Control Option Command N: tap to turn this page's Night Shift off or on, hold to see it in true colour"
					onPointerDown={(event) => {
						event.preventDefault();
						event.currentTarget.setPointerCapture(event.pointerId);
						press();
					}}
					onPointerUp={release}
					onPointerCancel={release}
					onKeyDown={(event) => {
						if ((event.key === " " || event.key === "Enter") && !event.repeat) {
							event.preventDefault();
							press();
						}
					}}
					onKeyUp={(event) => {
						if (event.key === " " || event.key === "Enter") release();
					}}
					// A key let go after the focus moved on never reaches the keys.
					onBlur={release}
					onClick={(event) => event.preventDefault()}
				>
					<span className="inline-flex gap-2.5" aria-hidden="true">
						{["⌃", "⌥", "⌘", "N"].map((key) => (
							<span
								key={key}
								className={cx(
									"box-border inline-flex h-11 min-w-11 items-center justify-center rounded-[10px] border border-line-strong bg-surface-raised px-2.5 font-text text-[19px] text-ink transition-[translate,box-shadow] duration-[70ms] motion-reduce:transition-none max-sm:h-[38px] max-sm:min-w-[38px] max-sm:text-[17px]",
									down
										? "translate-y-[3px] shadow-[0_0_0_var(--line-strong)]"
										: "shadow-[0_3px_0_var(--line-strong)]",
								)}
							>
								{key}
							</span>
						))}
					</span>
				</button>
				<p
					className="min-h-[1.6em] text-center text-[15px] text-ink-muted"
					aria-live="polite"
				>
					{hint}
				</p>
			</div>
		</Section>
	);
}

function MoveCard({
	lit,
	mini,
	title,
	glyph,
	children,
}: {
	lit: boolean;
	mini: ReactNode;
	title: string;
	glyph: ReactNode;
	children: ReactNode;
}) {
	return (
		<figure
			className={cx(
				"m-0 flex flex-col gap-4 rounded-md border border-line bg-surface p-4 transition-shadow duration-200 ease-in-out motion-reduce:transition-none",
				lit && "shadow-[0_0_0_2px_var(--moon-text),var(--shadow-lift)]",
			)}
		>
			{mini}
			<figcaption className="flex items-center gap-3.5">
				<span
					className={cx(
						"grid size-[46px] flex-none place-items-center rounded-sm border bg-surface-raised transition-[color,border-color] duration-200 ease-in-out motion-reduce:transition-none",
						lit ? "border-moon-text text-moon-text" : "border-line text-ink",
					)}
					aria-hidden="true"
				>
					<svg
						aria-hidden="true"
						className="size-[26px]"
						viewBox="0 0 24 24"
						fill="none"
						stroke="currentColor"
						strokeWidth="2"
						strokeLinecap="round"
						strokeLinejoin="round"
					>
						{glyph}
					</svg>
				</span>
				<span className="text-[15px] leading-[1.5] text-ink-muted">
					<strong className="block text-[16px] text-ink">{title}</strong>
					{children}
				</span>
			</figcaption>
		</figure>
	);
}
