import { useEffect } from "react";

/** Marks the body `at-end` while the page's second half is in view, so the browser's edges past
 *  it take the ground's colour instead of the sky's (main.css). */
export function useBodyEdge() {
	useEffect(() => {
		const mark = () =>
			document.body.classList.toggle(
				"at-end",
				window.scrollY + window.innerHeight / 2 >
					document.documentElement.scrollHeight / 2,
			);
		mark();
		window.addEventListener("scroll", mark, { passive: true });
		window.addEventListener("resize", mark);
		return () => {
			window.removeEventListener("scroll", mark);
			window.removeEventListener("resize", mark);
		};
	}, []);
}
