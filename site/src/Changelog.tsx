import { html, lead, title } from "virtual:changelog";
import styles from "./Changelog.module.css";
import { Legal } from "./components/Legal";

/** What changed in each version, from the repo's CHANGELOG.md, which each release also carries
 *  to GitHub and to the update window, and which the app links to after an update. */
export function Changelog() {
	return (
		<Legal title={title} lead={lead}>
			<div
				className={styles.versions}
				// biome-ignore lint/security/noDangerouslySetInnerHtml: the repo's own CHANGELOG.md, rendered at build time.
				dangerouslySetInnerHTML={{ __html: html }}
			/>
			<p className={styles.downloads}>
				Every release, with its downloads, is{" "}
				<a href="https://github.com/choco/Nightcap/releases">on GitHub</a>.
			</p>
		</Legal>
	);
}
