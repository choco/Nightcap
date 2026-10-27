import { type CSSProperties, useEffect, useRef, useState } from "react";
import { APPS, type AppName, MENUS } from "../data/menus";
import { cx } from "../lib/cx";
import { useInView } from "../lib/inView";
import { Clock } from "./Clock";
import styles from "./Desk.module.css";
import { MoonIcon } from "./icons";
import { Menu } from "./Menu";
import { Poster } from "./Poster";

// Until the reader picks an app, the desk shows each in turn, while it can be seen.
const CYCLE: AppName[] = ["figma", "mail", "safari", "mail"];

/** What the menu's Night Shift row says with the app in front. */
function nightShiftSays(app: AppName) {
	const row = MENUS[app].find((entry) => entry.kind === "nightShift");
	if (!row) throw new Error(`${app}'s menu has no Night Shift row`);
	return `Night Shift: ${row.reason.join(" ")}`;
}

/** The live Mac: the Nightcap menu as the app builds it, for whatever is in front. Figma has a
 *  config rule and youtube.com an ad-hoc one; Mail has none, so only Mail is warm. */
export function Desk({ shiftOn }: { shiftOn: boolean }) {
	const [step, setStep] = useState(0);
	const [picked, setPicked] = useState<AppName | null>(null);
	const desk = useRef<HTMLElement>(null);
	const inView = useInView(desk);
	useEffect(() => {
		if (picked || !inView) return;
		const timer = setInterval(
			() => setStep((current) => (current + 1) % CYCLE.length),
			3200,
		);
		return () => clearInterval(timer);
	}, [picked, inView]);
	const front = picked ?? CYCLE[step];
	const warm = front === "mail";
	const windowClass = (app: AppName) =>
		cx(styles.window, styles[app], "animate-swap motion-reduce:animate-none");

	return (
		<figure
			ref={desk}
			className={styles.desk}
			aria-label="A Mac running Nightcap: Night Shift is off while Figma is in front, and on in Mail"
		>
			{/* The screen is drawn, and hidden from assistive technology, all but the dock's buttons. */}
			<div className={styles.screen}>
				<div className={styles.menubar} aria-hidden="true">
					<span className={styles.appName}>{APPS[front].name}</span>
					<span className={styles.label}>File</span>
					<span className={styles.label}>Edit</span>
					<span className={styles.spacer} />
					<span className={styles.moon} aria-hidden="true">
						<MoonIcon moon={warm ? "on" : "forced"} />
					</span>
					<Clock className={styles.time} />
				</div>

				<div className={styles.windowSlot} aria-hidden="true">
					<div className={windowClass("mail")} hidden={front !== "mail"}>
						<div className={styles.windowBar}>Inbox</div>
						<div className={styles.mailRow}>
							<strong>Ada</strong>
							<span>Dinner on Friday?</span>
						</div>
						<div className={styles.mailRow}>
							<strong>Leo</strong>
							<span>Photos from Sunday, finally</span>
						</div>
						<div className={styles.mailRow}>
							<strong>Mia</strong>
							<span>Re: the poster colours</span>
						</div>
					</div>
					<div className={windowClass("figma")} hidden={front !== "figma"}>
						<div className={styles.windowBar}>Poster v3</div>
						<div className={styles.canvas}>
							<Poster />
						</div>
					</div>
					<div className={windowClass("safari")} hidden={front !== "safari"}>
						<div className={styles.windowBar}>
							<span className={styles.address}>youtube.com</span>
						</div>
						<div className={styles.video}>
							<span className={styles.play}>
								<svg
									aria-hidden="true"
									viewBox="0 0 24 24"
									width="18"
									height="18"
								>
									<path d="M8 5v14l11-7z" fill="#eef1fa" />
								</svg>
							</span>
						</div>
					</div>
				</div>

				<Menu
					className={styles.menuInline}
					entries={MENUS[front]}
					look="night"
					aria-hidden="true"
				/>

				<div className={styles.dock}>
					{(Object.keys(APPS) as AppName[]).map((app) => (
						<button
							key={app}
							className={styles.dockApp}
							type="button"
							aria-label={`Bring ${APPS[app].name} to the front`}
							aria-pressed={front === app}
							style={{ "--tile": APPS[app].tile } as CSSProperties}
							onClick={() => setPicked(app)}
						>
							{APPS[app].letter}
						</button>
					))}
				</div>

				<div
					className={cx(styles.tint, warm && shiftOn && styles.on)}
					aria-hidden="true"
				>
					<span />
					<span />
				</div>
			</div>
			<div className={styles.menusBelow}>
				{(Object.keys(APPS) as AppName[]).map((app) => (
					<Menu
						key={app}
						className={cx(styles.menuBelow, app !== front && styles.unseen)}
						entries={MENUS[app]}
						look="night"
					/>
				))}
			</div>
			{/* Says what a pick does to Night Shift, in the menu's words. The cycle says nothing, or
			    it would speak every 3.2 s. */}
			<p className="sr-only" aria-live="polite">
				{picked && nightShiftSays(picked)}
			</p>
		</figure>
	);
}
