import type { CSSProperties } from "react";
import { cx } from "../lib/cx";
import { DownloadButton } from "./Download";
import styles from "./Dusk.module.css";
import glow from "./Glow.module.css";
import { Outline } from "./icons";
import { MOON, SPARKLE, STAR } from "./outlines";

// The banner's sky: the moon and three stars, each swaying or twinkling.
const SKY: {
	d: string;
	at: CSSProperties;
	size: number;
	motion: "animate-sway" | "animate-twinkle";
	strokeWidth: number;
}[] = [
	{
		d: MOON,
		at: { right: "9%", top: 18 },
		size: 22,
		motion: "animate-sway",
		strokeWidth: 1.5,
	},
	{
		d: SPARKLE,
		at: { right: "30%", top: 22 },
		size: 14,
		motion: "animate-twinkle",
		strokeWidth: 1.6,
	},
	{
		d: STAR,
		at: { right: "44%", top: 12, animationDelay: "1.4s" },
		size: 10,
		motion: "animate-twinkle",
		strokeWidth: 1.8,
	},
	{
		d: SPARKLE,
		at: { right: "18%", top: 46, animationDelay: "2.6s" },
		size: 9,
		motion: "animate-twinkle",
		strokeWidth: 2,
	},
];

export function Dusk() {
	return (
		<section
			className={cx(
				styles.dusk,
				"mb-18 flex flex-wrap items-center gap-x-10 gap-y-6 rounded-md p-[clamp(28px,5vw,48px)]",
			)}
			data-lens-section
			data-lens-area
			data-dusk
		>
			<span className={styles.sky} aria-hidden="true" />
			<span className={cx(styles.sun, glow.glow)} aria-hidden="true" />
			{SKY.map(({ d, at, size, motion, strokeWidth }) => (
				<Outline
					key={d + size}
					className={cx(styles.star, motion, "motion-reduce:animate-none")}
					style={at}
					width={size}
					height={size}
					strokeWidth={strokeWidth}
				>
					<path d={d} />
				</Outline>
			))}
			<div className="flex min-w-0 flex-[999_1_300px] flex-col gap-2.5">
				<p className="font-display text-[40px] leading-[1.05] text-balance max-sm:text-[length:clamp(32px,9.5vw,34px)]">
					Set it up before sunset.
				</p>
				<p className="text-[15px] leading-[1.55] text-balance text-starlight-muted">
					Free and open source, for macOS 15 or later on Apple silicon.
				</p>
			</div>
			<DownloadButton plain />
		</section>
	);
}
