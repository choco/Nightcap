import { MENUS } from "../data/menus";
import { Clock } from "./Clock";
import { AppleLogo, MoonIcon } from "./icons";
import { Menu } from "./Menu";
import { Poster } from "./Poster";
import styles from "./Scene.module.css";

/** Nightcap in the menu bar with its menu open, while a rule keeps Night Shift off in Figma. */
export function Scene() {
	return (
		<figure
			className={styles.scene}
			data-lens-area
			data-doodle-clear="26"
			role="img"
			aria-label="Nightcap in the Mac menu bar with its menu open: a rule keeps Night Shift off while Figma is in front"
		>
			<div className={styles.bar} aria-hidden="true">
				<AppleLogo className={styles.apple} />
				<span className={styles.app}>Figma</span>
				<span className={styles.menus}>
					<span>File</span>
					<span>Edit</span>
					<span>View</span>
					<span>Object</span>
					<span>Plugins</span>
				</span>
				<span className={styles.spacer} />
				<span className={styles.status}>
					<span className={styles.moon}>
						<MoonIcon moon="forced" />
					</span>
					<Clock className={styles.clock} />
				</span>
			</div>
			<div className={styles.window} aria-hidden="true">
				<div className={styles.titlebar}>
					<span className={styles.lights}>
						<i />
						<i />
						<i />
					</span>
					Poster v3
				</div>
				<div className={styles.canvas}>
					<Poster />
				</div>
			</div>
			<Menu
				className={styles.menu}
				entries={MENUS.figma}
				look="mac"
				aria-hidden="true"
			/>
		</figure>
	);
}
