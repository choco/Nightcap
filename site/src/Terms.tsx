import { Legal } from "./components/Legal";

/** The terms of service, for the app, its command and this site, beside the MIT License, which
 *  governs the code. */
export function Terms() {
	return (
		<Legal
			title="Terms of service"
			lead="Nightcap is free and open source, and comes as it is. These are the terms for using it and this site."
			updated="2026-10-25"
		>
			<h2>The app and its licence</h2>
			<p>
				Nightcap is made by me, Enrico Ghirardi, and given away under the{" "}
				<a href="https://github.com/choco/Nightcap/blob/main/LICENSE">
					MIT License
				</a>
				. These terms cover using the app, its <code>nightcap</code> command and
				this website. The licence covers copying, changing and sharing
				Nightcap's code; where the two differ about the code, the licence wins.
			</p>

			<h2>Using Nightcap</h2>
			<p>
				You can use Nightcap for anything lawful, at home or at work, for free.
				It needs no account, and asks for nothing in return.
			</p>

			<h2>What it changes on your Mac</h2>
			<p>
				Nightcap changes Night Shift and nothing else: whether it's on, and how
				warm it gets and when, as you set them. macOS has no public way to do
				this, so Nightcap uses its private CoreBrightness framework. A macOS
				update can change that framework. Nightcap checks for it when it starts,
				and shows Night Shift as unavailable rather than guessing, but it can't
				promise to work on every future version of macOS.
			</p>
			<p>
				Night Shift changes the colours on your screen. Nightcap isn't a medical
				device, and makes no claim about sleep or eye health.
			</p>

			<h2>No warranty</h2>
			<p>
				Nightcap and this site are provided "as is", without warranty of any
				kind, express or implied, including the warranties of merchantability,
				fitness for a particular purpose and non-infringement. To the extent the
				law allows, I'm not liable for any claim, damages or other liability
				arising from Nightcap or this site, or from using them. This is the MIT
				License's own disclaimer, extended to the site.
			</p>

			<h2>Updates and availability</h2>
			<p>
				I may change Nightcap, release new versions or stop working on it at any
				time. Every update is signed, and a copy that updates itself installs
				one only if its signature checks out.
			</p>

			<h2>Other services</h2>
			<p>
				Nightcap's downloads, source and sponsorships are on GitHub, under{" "}
				<a href="https://docs.github.com/en/site-policy/github-terms/github-terms-of-service">
					GitHub's Terms of Service
				</a>
				. The sites Nightcap asks for their icons are their owners', under their
				own terms.
			</p>

			<h2>Trademarks</h2>
			<p>
				Mac, macOS and Night Shift are trademarks of Apple Inc. Nightcap isn't
				made, endorsed or supported by Apple. The other apps and sites named on
				this site belong to their owners.
			</p>

			<h2>Changes</h2>
			<p>
				If these terms change, the new version goes here with a new date above.
				Every version is in{" "}
				<a href="https://github.com/choco/Nightcap/commits/main/site/src/Terms.tsx">
					the site's history
				</a>
				.
			</p>

			<h2>Contact</h2>
			<p>
				Questions about these terms go to{" "}
				<a href="mailto:hello@choco.me">hello@choco.me</a>.
			</p>
		</Legal>
	);
}
