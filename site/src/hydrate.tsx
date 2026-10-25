import type { ComponentType } from "react";
import { hydrateRoot } from "react-dom/client";
import "./styles/main.css";

/** Hydrates the page prerender.ts wrote into its HTML's #root.
 *
 *  React listens on its root for every event it handles, animation events too, though nothing
 *  here handles them. Given any listener for animation events, WebKit runs every CSS animation on
 *  the page through its main thread, every frame, to dispatch them: the doodles alone then cost
 *  Safari 15% of a core at rest. So the root turns those listeners away, and `onAnimation*`
 *  handlers don't work here. */
export function hydrate(Page: ComponentType) {
	const root = document.getElementById("root");
	if (!root) throw new Error(`${location.pathname} has no #root`);
	const listen = root.addEventListener.bind(root);
	root.addEventListener = ((
		type: string,
		listener: EventListenerOrEventListenerObject,
		options?: boolean | AddEventListenerOptions,
	) => {
		if (!/^(webkit)?animation/i.test(type)) listen(type, listener, options);
	}) as typeof root.addEventListener;
	hydrateRoot(root, <Page />);
}
