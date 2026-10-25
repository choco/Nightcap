import { type ReactNode, useEffect, useState } from "react";
import { cx } from "../lib/cx";
import styles from "./Footer.module.css";

type Theme = "system" | "light" | "dark";

const THEMES: { theme: Theme; label: string; icon: ReactNode }[] = [
	{
		theme: "system",
		label: "Match the system",
		icon: (
			<svg
				aria-hidden="true"
				viewBox="0 0 24 24"
				width="17"
				height="17"
				fill="none"
				stroke="currentColor"
				strokeWidth="1.7"
				strokeLinejoin="round"
			>
				<circle cx="12" cy="12" r="8" />
				<path d="M12 4a8 8 0 0 1 0 16z" fill="currentColor" />
			</svg>
		),
	},
	{
		theme: "light",
		label: "Light",
		icon: (
			<svg
				aria-hidden="true"
				viewBox="0 0 24 24"
				width="17"
				height="17"
				fill="none"
				stroke="currentColor"
				strokeWidth="1.7"
				strokeLinecap="round"
			>
				<circle cx="12" cy="12" r="4.2" />
				<path d="M12 2.8v2.4M12 18.8v2.4M2.8 12h2.4M18.8 12h2.4M5.5 5.5l1.7 1.7M16.8 16.8l1.7 1.7M5.5 18.5l1.7-1.7M16.8 7.2l1.7-1.7" />
			</svg>
		),
	},
	{
		theme: "dark",
		label: "Dark",
		icon: (
			<svg
				aria-hidden="true"
				viewBox="0 0 24 24"
				width="17"
				height="17"
				fill="none"
				stroke="currentColor"
				strokeWidth="1.7"
				strokeLinejoin="round"
			>
				<path d="M20 14.6A8.6 8.6 0 1 1 9.4 4.1a6.7 6.7 0 0 0 10.6 10.5z" />
			</svg>
		),
	},
];

/** The appearance: the system's, unless this switch chose one, remembered on this device.
 *  index.html applies a remembered choice before the first paint. */
function ThemeSwitch() {
	// The server can't know the visitor's choice, so the page hydrates as "system" and then reads
	// the one index.html applied.
	const [choice, setChoice] = useState<Theme>("system");
	useEffect(() => {
		const applied = document.documentElement.dataset.theme;
		if (applied === "light" || applied === "dark") setChoice(applied);
	}, []);
	const choose = (theme: Theme) => {
		setChoice(theme);
		const root = document.documentElement;
		if (theme === "system") delete root.dataset.theme;
		else root.dataset.theme = theme;
		try {
			if (theme === "system") localStorage.removeItem("nightcap-theme");
			else localStorage.setItem("nightcap-theme", theme);
		} catch {
			// Without storage, the choice lasts until the page is left.
		}
	};
	return (
		<fieldset
			className={cx(
				styles.switch,
				"inline-flex gap-0.5 rounded-full p-[3px] shadow-[inset_0_0_0_1px_var(--line-strong)]",
			)}
			aria-label="Appearance"
		>
			{THEMES.map(({ theme, label, icon }) => (
				<button
					key={theme}
					className="inline-flex h-[30px] w-[34px] cursor-pointer items-center justify-center rounded-full border-0 bg-transparent p-0 text-ink-muted hover:text-ink aria-pressed:bg-surface aria-pressed:text-ink"
					type="button"
					aria-label={label}
					title={label}
					aria-pressed={choice === theme}
					onClick={() => choose(theme)}
				>
					{icon}
				</button>
			))}
		</fieldset>
	);
}

export function Footer() {
	return (
		<footer className={cx(styles.footer, "text-[14px] text-ink-muted")}>
			<span className={cx(styles.credit, "inline-flex items-center gap-1.5")}>
				Made with
				<svg
					viewBox="0 0 24 24"
					width="16"
					height="16"
					fill="none"
					stroke="var(--moon-text)"
					strokeWidth="1.8"
					strokeLinejoin="round"
					role="img"
					aria-label="love"
				>
					<path d="M12 20.2s-7.6-4.6-7.6-10.1a4.2 4.2 0 0 1 7.6-2.5 4.2 4.2 0 0 1 7.6 2.5c0 5.5-7.6 10.1-7.6 10.1z" />
				</svg>
				<span>
					by{" "}
					<a className="text-ink-muted" href="https://choco.me">
						choco
					</a>
				</span>
				<svg
					className="ml-1 rotate-16"
					viewBox="0 0 30 44"
					width="24"
					height="35"
					aria-hidden="true"
				>
					<defs>
						<mask id="choco-bite">
							<rect width="30" height="44" fill="#fff" />
							<circle cx="21.4" cy="1.4" r="3.3" />
							<circle cx="25" cy="4.4" r="3.3" />
							<circle cx="27.6" cy="8.6" r="3.3" />
							<path
								d="M15 2v20M4 9.2h22M4 16h22"
								stroke="#000"
								strokeWidth="1.4"
							/>
						</mask>
					</defs>
					<rect
						x="4"
						y="2"
						width="22"
						height="22"
						rx="2"
						fill="var(--choco)"
						mask="url(#choco-bite)"
					/>
					<path
						d="M3 23.4 4.8 20.6 6.6 23.4 8.4 20.6 10.2 23.4 12 20.6 13.8 23.4 15.6 20.6 17.4 23.4 19.2 20.6 21 23.4 22.8 20.6 24.6 23.4 26.4 20.6 27 21.6V26H3z"
						fill="var(--choco-foil)"
					/>
					<rect
						x="3"
						y="24"
						width="24"
						height="18"
						rx="1.6"
						fill="var(--moon)"
					/>
					<rect x="3" y="29.5" width="24" height="6" fill="var(--choco-band)" />
				</svg>
			</span>
			<span className={styles.links}>
				<a className="text-ink-muted" href="https://github.com/choco/Nightcap">
					GitHub
				</a>
				<a className="text-ink-muted" href="https://github.com/sponsors/choco">
					Sponsor
				</a>
				<a
					className="text-ink-muted"
					href="https://github.com/choco/Nightcap/releases"
				>
					Releases
				</a>
				<a className="text-ink-muted" href="/privacy/">
					Privacy
				</a>
				<a className="text-ink-muted" href="/terms/">
					Terms
				</a>
				<a
					className="text-ink-muted"
					href="https://github.com/choco/Nightcap/blob/main/LICENSE"
				>
					MIT licence
				</a>
			</span>
			<span className={styles.year}>© 2026</span>
			<ThemeSwitch />
		</footer>
	);
}
