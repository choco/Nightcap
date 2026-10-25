import { cx } from "../lib/cx";

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
		<a
			className={cx(
				"inline-flex h-[54px] items-center gap-2.5 rounded-full border bg-moon px-7 text-[17px] font-semibold text-on-moon no-underline hover:text-on-moon",
				plain ? "border-transparent" : "border-primary-edge",
				"transition-transform duration-[420ms] ease-spring hover:scale-[1.045] active:scale-[.97] motion-reduce:transition-none",
			)}
			href={url || RELEASES}
		>
			Download for Mac
			{showVersion && version && (
				<span className="font-mono text-[12px] font-medium opacity-75">
					{version}
				</span>
			)}
		</a>
	);
}
