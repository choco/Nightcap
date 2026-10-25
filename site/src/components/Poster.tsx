import styles from "./Poster.module.css";

export function Poster() {
	return (
		<div className={styles.board}>
			<div className={styles.circle} />
			<div className={styles.square} />
			<div className={styles.bar} />
		</div>
	);
}
