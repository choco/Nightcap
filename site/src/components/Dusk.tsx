import type { CSSProperties, ReactNode } from "react";
import { cx } from "../lib/cx";
import { DownloadButton } from "./Download";
import styles from "./Dusk.module.css";
import glow from "./Glow.module.css";

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

const SPARKLE =
	"M12 3c.9 4.7 2 6.2 8.6 9-6.6 2.8-7.7 4.3-8.6 9-.9-4.7-2-6.2-8.6-9 6.6-2.8 7.7-4.3 8.6-9z";

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
				<path d="M20 14.6A8.6 8.6 0 1 1 9.4 4.1a6.7 6.7 0 0 0 10.6 10.5z" />
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
				<path d="M12 3.3l2.6 5.5 6 .8-4.4 4.1 1.1 6-5.3-2.9-5.3 2.9 1.1-6-4.4-4.1 6-.8z" />
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
				<p className="font-display text-[40px] leading-[1.05] text-balance max-sm:text-[34px]">
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
