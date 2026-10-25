import { useRef } from "react";
import { ButtonLink } from "./components/Button";
import { Celestial } from "./components/Celestial";
import { Doodles, useDoodles } from "./components/Doodles";
import { Footer } from "./components/Footer";
import { Wordmark } from "./components/Wordmark";
import styles from "./NotFound.module.css";

/** The page Pages serves for a path that isn't there: the hero's sky, and the way back. */
export function NotFound() {
	const page = useRef<HTMLDivElement>(null);
	const doodles = useDoodles(page);
	return (
		<div ref={page} className="relative overflow-hidden">
			<div className={styles.sky} data-hero>
				<Doodles doodles={doodles.hero} />
				<Celestial />
				<div className="relative mx-auto flex min-h-svh max-w-[calc(864px+4rem)] flex-col px-4">
					<div className="py-6" data-doodle-clear="12">
						<Wordmark href="/" />
					</div>
					<main className="flex flex-1 flex-col justify-center pt-8 pb-16">
						<div
							className="flex max-w-[36rem] flex-col items-start gap-6"
							data-doodle-clear="26"
						>
							<span className="text-[12px] font-[650] tracking-[0.08em] text-ink-muted uppercase">
								404 · Not found
							</span>
							<h1 className="animate-rise font-display text-[length:clamp(40px,7vw,64px)] leading-none font-normal tracking-[-0.005em] text-balance motion-reduce:animate-none">
								Nothing at this address.
							</h1>
							<p className="max-w-[40ch] animate-fade-in text-[19px] leading-[1.6] motion-reduce:animate-none max-sm:text-[17px]">
								The link may be mistyped, or out of date. Everything about
								Nightcap is on its home page.
							</p>
							<div className="flex flex-wrap gap-3">
								<ButtonLink href="/" primary>
									Back to Nightcap
								</ButtonLink>
								<ButtonLink href="https://github.com/choco/Nightcap">
									View on GitHub
								</ButtonLink>
							</div>
						</div>
					</main>
					<div data-doodle-clear="12">
						<Footer />
					</div>
				</div>
			</div>
		</div>
	);
}
