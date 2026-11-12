import type { ReactNode } from "react";
import { useBodyEdge } from "../lib/edge";
import { Footer } from "./Footer";
import styles from "./Legal.module.css";
import { Wordmark } from "./Wordmark";

// Fixed, so the prerendered date and the hydrated one match wherever the page is read.
const day = new Intl.DateTimeFormat("en-GB", {
	dateStyle: "long",
	timeZone: "UTC",
});

/** A page of text beside the home page, such as the privacy policy, the terms or the changelog:
 *  the wordmark, the title and its lead under the sky, then the text on the ground, and the
 *  footer. `updated` is the day the text last changed, as YYYY-MM-DD, for a page that says so. */
export function Legal({
	title,
	lead,
	updated,
	children,
}: {
	title: string;
	lead: string;
	updated?: string;
	children: ReactNode;
}) {
	useBodyEdge();
	return (
		<div className="min-h-svh bg-ground">
			<header className="bg-hero-sky">
				<div className="mx-auto max-w-column px-4">
					<div className="py-6">
						<Wordmark href="/" />
					</div>
					<div className="flex flex-col items-start gap-4 pt-10 pb-14 max-sm:pt-6 max-sm:pb-10">
						<h1 className="font-display text-display-l text-balance">
							{title}
						</h1>
						<p className="max-w-[46ch] text-lead max-sm:text-[17px]">{lead}</p>
						{updated && (
							<p className="text-[14px] text-ink-muted">
								Last updated{" "}
								<time dateTime={updated}>{day.format(new Date(updated))}</time>
							</p>
						)}
					</div>
				</div>
			</header>
			<main className="mx-auto max-w-column px-4">
				<article className={styles.text}>{children}</article>
				<Footer />
			</main>
		</div>
	);
}
