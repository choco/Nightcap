import { Legal } from "./components/Legal";

/** The privacy policy, for the app, its command and this site. Every statement is one the code
 *  bears out: change it with the behaviour it describes. */
export function Privacy() {
	return (
		<Legal
			title="Privacy policy"
			lead="Nightcap collects nothing about you. What it knows about your Mac stays on your Mac."
			updated="2026-10-25"
		>
			<h2>Who this covers</h2>
			<p>
				Nightcap is a free, open-source Mac app made by me, Enrico Ghirardi.
				This policy covers the app, its <code>nightcap</code> command, and this
				website, nightcap.choco.me.
			</p>
			<p>
				Nightcap has no accounts, no analytics, no ads and no crash reporting.
				It sends nothing about you to me or to anyone else. Its source is{" "}
				<a href="https://github.com/choco/Nightcap">on GitHub</a>, so you can
				check every line of this.
			</p>

			<h2>What stays on your Mac</h2>
			<p>Nightcap keeps what it needs in your home folder:</p>
			<ul>
				<li>
					<code>~/.config/nightcap/config.toml</code>, the config file, if you
					write one.
				</li>
				<li>
					<code>~/.local/share/nightcap/rules.toml</code>, the rules you add
					from the menu or with <code>nightcap rules add</code>.
				</li>
				<li>
					<code>~/.local/state/nightcap/state.json</code>, its timers and
					snoozes, and the choices you made in onboarding and Settings.
				</li>
				<li>
					<code>~/Library/Caches/io.github.choco.nightcap/</code>, the icons of
					the sites it shows.
				</li>
				<li>
					<code>~/Library/Preferences/io.github.choco.nightcap.plist</code>,
					where macOS keeps when it last checked for an update.
				</li>
			</ul>
			<p>None of it leaves your Mac.</p>

			<h2>Website checking</h2>
			<p>
				Website checking lets a rule follow a website. It stays off until you
				turn it on, and it needs Accessibility, which macOS asks you to allow.
			</p>
			<p>
				With it on, Nightcap reads the address of the front tab in the browser
				in front, keeps only the site's name, such as <code>figma.com</code>,
				and forgets the rest of the address. It doesn't read the page, its
				title, your other tabs or your history. macOS's log shows the site as{" "}
				<code>&lt;private&gt;</code>.
			</p>

			<h2>What Nightcap asks the internet</h2>
			<p>Nightcap goes online for two things, and nothing else:</p>
			<ul>
				<li>
					<strong>Updates.</strong> A copy that updates itself, installed from
					the DMG or the zip, checks <code>nightcap.choco.me/appcast.xml</code>{" "}
					once a day and downloads a new version from GitHub. The check names
					Nightcap and its version, as any update check does, and sends nothing
					about your Mac. A copy that a package manager such as Nix installed
					never checks.
				</li>
				<li>
					<strong>Site icons.</strong> With website checking on, the menu and
					Settings show sites' own icons. Nightcap asks a site for its icon, the
					site in front or one your rules name, without cookies, and keeps it
					for a month. Onboarding shows the icons of the sites it suggests, the
					same for everyone, such as youtube.com and figma.com. A site sees the
					request come from your Mac, as it does when you visit it.
				</li>
			</ul>
			<p>
				Links in the app, such as Sponsor in Settings, open in your browser.
			</p>

			<h2>Permissions</h2>
			<ul>
				<li>
					<strong>Accessibility</strong>, for website checking alone.
				</li>
				<li>
					<strong>Notifications</strong>, to tell you about a problem that needs
					you, such as an invalid config, and when Night Shift changed outside
					Nightcap and a rule was snoozed.
				</li>
				<li>
					<strong>Open at Login</strong>, only if you choose it.
				</li>
			</ul>
			<p>You can turn each of them off in System Settings.</p>

			<h2>This website</h2>
			<p>
				This site sets no cookies, runs no analytics and loads nothing from
				anyone else: its fonts and images come from the site itself. If you pick
				an appearance in the footer, your browser remembers it in its own
				storage, and it stays there.
			</p>
			<p>
				GitHub serves this site and Nightcap's downloads, and keeps logs of the
				requests it serves, such as your IP address, under the{" "}
				<a href="https://docs.github.com/en/site-policy/privacy-policies/github-general-privacy-statement">
					GitHub General Privacy Statement
				</a>
				.
			</p>

			<h2>Sponsoring</h2>
			<p>
				Sponsoring goes through GitHub Sponsors, under GitHub's terms and
				privacy statement. GitHub shows me your account, and your email address
				if you choose to share it.
			</p>

			<h2>Changes</h2>
			<p>
				If Nightcap starts doing something this policy doesn't describe, the
				policy changes first, with a new date above. Every version is in{" "}
				<a href="https://github.com/choco/Nightcap/commits/main/site/src/Privacy.tsx">
					the site's history
				</a>
				.
			</p>

			<h2>Contact</h2>
			<p>
				Questions about this policy, or about anything Nightcap does with your
				data, go to <a href="mailto:hello@choco.me">hello@choco.me</a>.
			</p>
		</Legal>
	);
}
