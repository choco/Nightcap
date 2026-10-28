import { cx } from "../lib/cx";
import styles from "./Tint.module.css";

export function Tint({ on }: { on: boolean }) {
	return (
		<div className={cx(styles.tint, on && styles.on)} aria-hidden="true">
			<span />
			<span />
		</div>
	);
}
