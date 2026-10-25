import { type RefObject, useEffect, useState } from "react";

/** Whether the element is on screen, or within half a screen of it, so a demo that runs only
 *  while it can be seen is already running when it scrolls in. */
export function useInView(ref: RefObject<Element | null>): boolean {
	const [inView, setInView] = useState(false);
	useEffect(() => {
		const element = ref.current;
		if (!element) throw new Error("useInView has no element");
		const observer = new IntersectionObserver(
			([entry]) => setInView(entry.isIntersecting),
			{ rootMargin: "50% 0px" },
		);
		observer.observe(element);
		return () => observer.disconnect();
	}, [ref]);
	return inView;
}
