import { useEffect, useState } from "react";
import { cx } from "../lib/cx";

export interface Release {
	version: string;
	url: string;
}

const RELEASES = "https://github.com/choco/Nightcap/releases/latest";

/** The newest release's DMG, as the Pages deploy writes it into download.json. Until it loads, or
 *  where the site has none, which is before the first release and in a local preview, the
 *  download buttons lead to the releases page. */
export function useRelease(): Release | null {
	const [release, setRelease] = useState<Release | null>(null);
	useEffect(() => {
		fetch("download.json")
			.then((response) => {
				if (response.status === 404) return null;
				if (!response.ok) throw new Error(`download.json: ${response.status}`);
				return response.json() as Promise<Release>;
			})
			.then(setRelease);
	}, []);
	return release;
}

export function DownloadButton({
	release,
	plain,
	showVersion,
}: {
	release: Release | null;
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
			href={release?.url ?? RELEASES}
		>
			Download for Mac
			{showVersion && release && (
				<span className="font-mono text-[12px] font-medium opacity-75">
					{release.version}
				</span>
			)}
		</a>
	);
}
