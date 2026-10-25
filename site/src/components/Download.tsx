import { ButtonLink } from "./Button";

const RELEASES = "https://github.com/choco/Nightcap/releases/latest";

/** The newest release's DMG, which the Pages deploy builds the site with (pages.yml), so the
 *  prerendered page links it. A build without one, as before the first release and everywhere
 *  but the deploy, leads to the releases page. */
const { VITE_RELEASE_VERSION: version, VITE_RELEASE_URL: url } = import.meta
	.env;
if (Boolean(version) !== Boolean(url))
	throw new Error("VITE_RELEASE_VERSION and VITE_RELEASE_URL go together");

export function DownloadButton({
	plain,
	showVersion,
}: {
	plain?: boolean;
	showVersion?: boolean;
}) {
	return (
		<ButtonLink href={url || RELEASES} primary large plain={plain}>
			Download for Mac
			{showVersion && version && (
				<span className="font-mono text-[12px] font-medium opacity-75">
					{version}
				</span>
			)}
		</ButtonLink>
	);
}

/** The newest release's notes, beside the download that names its version, or the whole
 *  changelog in a build without a release. Muted, since the download button is the view's one
 *  gold. */
export function WhatsNew() {
	return (
		<a
			className="text-ink-muted"
			href={version ? `/changelog/#v${version}` : "/changelog/"}
		>
			What’s new
		</a>
	);
}
