import { useState } from "react";
import { nightShift, placeRow, separator } from "../data/menus";
import { MOMENTS } from "../data/moments";
import { cx } from "../lib/cx";
import styles from "./Evening.module.css";
import { Menu } from "./Menu";
import { Section, SplitHeading } from "./Section";

/** One evening: what each moment does to the screen and the menu. */
export function Evening() {
	const [index, setIndex] = useState(3);
	const moment = MOMENTS[index];
	return (
		<Section id="evening" className="flex flex-col gap-7">
			<SplitHeading title="Overrule it anytime">
				Snooze a rule from its app's row in the menu, for half an hour or until
				morning. Or turn Night Shift on in Control Center while a rule holds it
				off: Nightcap goes along with you, and snoozes that rule until the
				schedule's next change instead of fighting you.
			</SplitHeading>
			<div className={styles.sky} aria-hidden="true">
				{MOMENTS.map((m, i) => (
					<span
						key={m.time}
						className={cx(styles.marker, i === index && styles.on)}
						style={{ left: `${10 + i * 20}%` }}
					/>
				))}
			</div>
			<fieldset
				className="grid min-w-0 grid-cols-2 gap-2 sm:grid-cols-5"
				aria-label="Moments of one evening"
				data-lens-area
			>
				{MOMENTS.map((m, i) => (
					<button
						key={m.time}
						className="group flex min-h-[76px] cursor-pointer flex-col items-start gap-1 rounded-[12px] max-sm:last:col-span-2 border border-line bg-transparent px-3.5 py-3 text-left font-text text-ink aria-pressed:border-moon-text aria-pressed:bg-surface"
						type="button"
						aria-pressed={i === index}
						onClick={() => setIndex(i)}
					>
						<span className="inline-flex items-center gap-[7px] font-mono text-[12px] text-ink-muted tabular-nums group-aria-pressed:text-moon-text">
							<span
								className={cx(
									"size-2 rounded-full",
									m.warm ? "bg-ember" : "bg-true-colour",
								)}
								role="img"
								aria-label={m.warm ? "Warm screen" : "True colour"}
							/>
							{m.time}
						</span>
						<span className="text-[15px] leading-[1.3] font-[650]">
							{m.title}
						</span>
					</button>
				))}
			</fieldset>
			<div className="flex flex-wrap items-start gap-x-10 gap-y-6">
				<p
					key={index}
					className="max-w-[46ch] flex-[1_1_320px] animate-swap text-[17px] leading-[1.6] motion-reduce:animate-none"
				>
					{moment.body}
				</p>
				<Menu
					className="flex-[0_1_272px]"
					look="night"
					entries={[
						nightShift(moment.moon, moment.reason),
						separator,
						placeRow(moment.item, moment.app, moment.note),
					]}
					role="img"
					aria-label={`Nightcap's menu. Night Shift: ${moment.reason.join(" ")}. ${moment.item}${moment.note ? `: ${moment.note.join(" ")}` : ""}.`}
				/>
			</div>
		</Section>
	);
}
