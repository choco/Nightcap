import type { Doodle } from "../lib/doodles";
import { Celestial } from "./Celestial";
import { Doodles } from "./Doodles";
import { DownloadButton } from "./Download";
import styles from "./Hero.module.css";
import { Nav } from "./Nav";
import { Scene } from "./Scene";

interface Props {
	doodles: Doodle[];
	shiftOn: boolean;
	onShift: () => void;
}

export function Hero({ doodles, shiftOn, onShift }: Props) {
	return (
		<section className={styles.hero} data-lens-section data-hero>
			<Doodles doodles={doodles} />
			<Celestial />
			{/* The hero's column is wider than the sections', and on screens wider than phones it
          fills the first screen up to 940px tall, with the content centred in it. */}
			<div className="relative mx-auto max-w-[calc(clamp(1080px,68vw,1280px)+4rem)] px-4 md:flex md:min-h-[min(100svh,940px)] md:flex-col">
				<Nav shiftOn={shiftOn} onShift={onShift} />
				<header
					id="top"
					className="@container grid grid-cols-[minmax(0,1.12fr)_minmax(0,1fr)] content-center items-center gap-x-[clamp(24px,3.5vw,48px)] gap-y-10 pt-4 pb-22 max-md:flex max-md:flex-col max-md:items-stretch max-md:pt-12 max-md:pb-20 md:flex-[1_0_auto]"
				>
					<div
						className="flex min-w-0 flex-col gap-6 max-md:flex-none"
						data-doodle-clear="26"
					>
						<span className="text-[12px] font-[650] tracking-[0.08em] text-ink-muted uppercase">
							For Mac · free and open source
						</span>
						{/* The headline scales with the column, so beside the scene it always keeps its two
                lines. */}
						<h1 className="font-display text-[length:clamp(28px,5.8cqi,76px)] leading-none font-normal tracking-[-0.01em] text-balance max-md:text-[length:clamp(32px,10.2vw,44px)]">
							<span className="inline-block animate-rise motion-reduce:animate-none">
								Night Shift,
							</span>{" "}
							<span className="inline-block animate-rise [animation-delay:140ms] motion-reduce:animate-none">
								made yours.
							</span>
						</h1>
						<p className="max-w-[46ch] animate-fade-in text-[19px] leading-[1.6] motion-reduce:animate-none max-sm:text-[17px]">
							Nightcap brings Night Shift to your menu bar. Set how warm it
							gets, switch it or peek past it with a shortcut, and keep it off
							while the apps and websites you choose are in front.
						</p>
						<div className="flex flex-wrap items-center gap-x-4 gap-y-2">
							<DownloadButton showVersion />
							<span className="text-[14px] text-ink-muted">
								macOS 15 or later, Apple silicon
							</span>
						</div>
					</div>
					<Scene />
				</header>
			</div>
			<a
				className="absolute bottom-[22px] left-1/2 z-1 inline-flex -translate-x-1/2 flex-col items-center gap-0.5 text-[13px] font-semibold text-ink-muted no-underline hover:text-ink max-md:hidden"
				href="#how"
				data-doodle-clear="20"
			>
				How it works
				<svg
					className="animate-nudge motion-reduce:animate-none"
					viewBox="0 0 24 24"
					width="18"
					height="18"
					fill="none"
					stroke="currentColor"
					strokeWidth="2"
					strokeLinecap="round"
					strokeLinejoin="round"
					aria-hidden="true"
				>
					<path d="m6 9 6 6 6-6" />
				</svg>
			</a>
		</section>
	);
}
