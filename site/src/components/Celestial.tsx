import { cx } from "../lib/cx";
import styles from "./Celestial.module.css";

const ambient = "motion-reduce:animate-none";

export function Celestial() {
	return (
		<div className={styles.celestial} aria-hidden="true">
			<div className={styles.moon}>
				<div
					className={cx(styles.ring, styles.near, "animate-spin", ambient)}
				/>
				<div
					className={cx(styles.ring, styles.far, "animate-spin-back", ambient)}
				/>
				<div className={cx(styles.glow, "animate-glow", ambient)} />
				<div className={styles.disc} />
			</div>
			<div className={styles.sun}>
				<div className={cx(styles.rays, "animate-spin", ambient)}>
					<svg
						viewBox="0 0 100 100"
						fill="none"
						stroke="var(--doodle-gold)"
						strokeWidth="0.7"
						strokeLinecap="round"
					>
						<path d="M50 2v9M50 89v9M2 50h9M89 50h9M16 16l6.4 6.4M77.6 77.6l6.4 6.4M16 84l6.4-6.4M77.6 22.4l6.4-6.4M31.6 5.6l3.4 8.3M65 86.1l3.4 8.3M5.6 31.6l8.3 3.4M86.1 65l8.3 3.4M5.6 68.4l8.3-3.4M86.1 35l8.3-3.4M31.6 94.4l3.4-8.3M65 13.9l3.4-8.3" />
					</svg>
				</div>
				<div className={cx(styles.disc, "animate-glow", ambient)} />
			</div>
		</div>
	);
}
