import { useEffect, useRef, useState } from "react";
import { Cli } from "./components/Cli";
import { Doodles, useDoodles } from "./components/Doodles";
import { useRelease } from "./components/Download";
import { Dusk } from "./components/Dusk";
import { Evening } from "./components/Evening";
import { Footer } from "./components/Footer";
import { Hero } from "./components/Hero";
import { How } from "./components/How";
import { Lens, useLens } from "./components/Lens";
import { type Peek, Shortcut } from "./components/Shortcut";

/** The page, under its own Night Shift, which the nav's switch and the shortcut's tap turn off
 *  and the shortcut's hold peeks past. */
export function App() {
	const page = useRef<HTMLDivElement>(null);
	const [shiftOn, setShiftOn] = useState(true);
	const [peek, setPeek] = useState<Peek>(null);
	const release = useRelease();
	const doodles = useDoodles(page);
	useLens(page);

	// The end of the page in view, for the body's background (main.css).
	useEffect(() => {
		const mark = () =>
			document.body.classList.toggle(
				"at-end",
				window.scrollY + window.innerHeight / 2 >
					document.documentElement.scrollHeight / 2,
			);
		mark();
		window.addEventListener("scroll", mark, { passive: true });
		window.addEventListener("resize", mark);
		return () => {
			window.removeEventListener("scroll", mark);
			window.removeEventListener("resize", mark);
		};
	}, []);

	const toggleShift = () => setShiftOn((on) => !on);
	return (
		<div
			ref={page}
			className="relative min-h-screen overflow-hidden bg-ground"
			data-shift={shiftOn ? "on" : "off"}
			data-peek={peek ?? undefined}
		>
			<Doodles doodles={doodles.page} />
			<Hero
				doodles={doodles.hero}
				release={release}
				shiftOn={shiftOn}
				onShift={toggleShift}
			/>
			{/* The sections' column: 896px inside 1rem gutters, a step wider than the nav's. */}
			<main className="relative mx-auto max-w-[calc(864px+4rem)] px-4">
				<How shiftOn={shiftOn} />
				<Shortcut shiftOn={shiftOn} onShift={toggleShift} onPeek={setPeek} />
				<Evening />
				<Cli />
				<Dusk release={release} />
				<Footer />
			</main>
			<Lens />
		</div>
	);
}
