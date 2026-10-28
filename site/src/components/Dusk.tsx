import type { CSSProperties, ReactNode } from "react";
import { cx } from "../lib/cx";
import { DownloadButton } from "./Download";
import styles from "./Dusk.module.css";
import glow from "./Glow.module.css";
import { MOON, SPARKLE, STAR } from "./outlines";

function Star({
	style,
	size,
	motion,
	strokeWidth,
	children,
}: {
	style: CSSProperties;
	size: number;
	motion: string;
	strokeWidth: number;
	children: ReactNode;
}) {
	return (
		<svg
			className={cx(styles.star, motion, "motion-reduce:animate-none")}
			style={style}
			viewBox="0 0 24 24"
			width={size}
			height={size}
			fill="none"
			stroke="currentColor"
			strokeWidth={strokeWidth}
			strokeLinecap="round"
			strokeLinejoin="round"
			aria-hidden="true"
		>
			{children}
		</svg>
	);
}

export function Dusk() {
	return (
		<section
			className={cx(
				styles.dusk,
				"mb-18 flex flex-wrap items-center gap-x-10 gap-y-6 rounded-[12px] p-[clamp(28px,5vw,48px)]",
			)}
			data-lens-section
			data-lens-area
			data-dusk
		>
			<span className={styles.sky} aria-hidden="true" />
			<span className={cx(styles.sun, glow.glow)} aria-hidden="true" />
			<Star
				style={{ right: "9%", top: 18 }}
				size={22}
				motion="animate-sway"
				strokeWidth={1.5}
			>
				<path d={MOON} />
			</Star>
			<Star
				style={{ right: "30%", top: 22 }}
				size={14}
				motion="animate-twinkle"
				strokeWidth={1.6}
			>
				<path d={SPARKLE} />
			</Star>
			<Star
				style={{ right: "44%", top: 12, animationDelay: "1.4s" }}
				size={10}
				motion="animate-twinkle"
				strokeWidth={1.8}
			>
				<path d={STAR} />
			</Star>
			<Star
				style={{ right: "18%", top: 46, animationDelay: "2.6s" }}
				size={9}
				motion="animate-twinkle"
				strokeWidth={2}
			>
				<path d={SPARKLE} />
			</Star>
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
