import type { CSSProperties, HTMLAttributes, ReactNode } from "react";
import { APPS, KEYS, type MenuEntry } from "../data/menus";
import { cx } from "../lib/cx";
import { Chevron, GlyphIcon, MoonIcon, Sun } from "./icons";
import styles from "./Menu.module.css";

type Props = HTMLAttributes<HTMLDivElement> & {
	entries: MenuEntry[];
	look: "mac" | "night";
};

export function Menu({ entries, look, className, ...rest }: Props) {
	return (
		<div className={cx(styles[look], className)} {...rest}>
			{entries.map((entry, index) => (
				<Entry key={index} entry={entry} />
			))}
		</div>
	);
}

function Entry({ entry }: { entry: MenuEntry }) {
	switch (entry.kind) {
		case "separator":
			return <div className={styles.separator} />;
		case "strength":
			return (
				<div className={styles.slider}>
					<span className={cx(styles.sun, styles.less)}>
						<Sun />
					</span>
					<span
						className={styles.track}
						style={{ "--value": entry.value } as CSSProperties}
					>
						<span className={styles.fill} />
						<span className={styles.knob} />
					</span>
					<span className={cx(styles.sun, styles.more)}>
						<Sun large />
					</span>
				</div>
			);
		case "nightShift":
			return (
				<Item
					checked={entry.moon !== "off"}
					image={<MoonIcon moon={entry.moon} />}
					title="Night Shift"
					subtitle={entry.reason}
					end={KEYS}
				/>
			);
		case "item": {
			const { image } = entry;
			const picture =
				"app" in image ? (
					<span
						className={styles.app}
						style={{ "--tile": APPS[image.app].tile } as CSSProperties}
					>
						{APPS[image.app].letter}
					</span>
				) : (
					<GlyphIcon glyph={image.glyph} />
				);
			return (
				<Item
					image={picture}
					title={entry.title}
					subtitle={entry.subtitle}
					end={entry.submenu ? <Chevron /> : entry.key}
					indented={entry.indented}
				/>
			);
		}
	}
}

function Item(props: {
	checked?: boolean;
	image: ReactNode;
	title: string;
	subtitle?: string[];
	end?: ReactNode;
	indented?: boolean;
}) {
	return (
		<div className={cx(styles.item, props.indented && styles.indented)}>
			<span className={styles.check}>{props.checked ? "✓" : ""}</span>
			<span className={styles.image}>{props.image}</span>
			<span className={styles.text}>
				<span>{props.title}</span>
				{props.subtitle && (
					<span className={styles.subtitle}>{props.subtitle.join("\n")}</span>
				)}
			</span>
			<span className={styles.key}>{props.end}</span>
		</div>
	);
}
