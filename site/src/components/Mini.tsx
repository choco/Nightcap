import type { Moon } from "../data/menus";
import { Clock } from "./Clock";
import { MoonIcon } from "./icons";
import styles from "./Mini.module.css";
import { Poster } from "./Poster";
import { Tint } from "./Tint";

export function Mini({ warm, moon }: { warm: boolean; moon: Moon }) {
	return (
		<div className={styles.mini} aria-hidden="true">
			<div className={styles.bar}>
				<span className={styles.app}>Figma</span>
				<span className={styles.spacer} />
				<span className={styles.moon}>
					<MoonIcon moon={moon} />
				</span>
				<Clock timeOnly />
			</div>
			<div className={styles.win}>
				<Poster />
			</div>
			<Tint on={warm} />
		</div>
	);
}
