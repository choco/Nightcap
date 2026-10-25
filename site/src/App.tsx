import { useRef, useState } from "react";
import { Cli } from "./components/Cli";
import { Doodles, useDoodles } from "./components/Doodles";
import { Dusk } from "./components/Dusk";
import { Evening } from "./components/Evening";
import { Footer } from "./components/Footer";
import { Hero } from "./components/Hero";
import { How } from "./components/How";
import { Lens } from "./components/Lens";
import { type Peek, Shortcut } from "./components/Shortcut";
import { useBodyEdge } from "./lib/edge";

/** The page, under its own Night Shift, which the nav's switch and the shortcut's tap turn off
 *  and the shortcut's hold peeks past. */
export function App() {
	const page = useRef<HTMLDivElement>(null);
	const [shiftOn, setShiftOn] = useState(true);
	const [peek, setPeek] = useState<Peek>(null);
	const doodles = useDoodles(page);
	useBodyEdge();

	const toggleShift = () => setShiftOn((on) => !on);
	return (
		<div
			ref={page}
			className="relative min-h-screen overflow-hidden bg-ground"
			data-shift={shiftOn ? "on" : "off"}
			data-peek={peek ?? undefined}
		>
			<Doodles doodles={doodles.page} />
			<Hero doodles={doodles.hero} shiftOn={shiftOn} onShift={toggleShift} />
			{/* The sections' column: 896px inside 1rem gutters, a step wider than the nav's. */}
			<main className="relative mx-auto max-w-[calc(864px+4rem)] px-4">
				<How shiftOn={shiftOn} />
				<Shortcut shiftOn={shiftOn} onShift={toggleShift} onPeek={setPeek} />
				<Evening />
				<Cli />
				<Dusk />
				<Footer />
			</main>
			<Lens page={page} />
		</div>
	);
}
