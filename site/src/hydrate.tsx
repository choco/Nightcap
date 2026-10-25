import type { ComponentType } from "react";
import { hydrateRoot } from "react-dom/client";
import "./styles/main.css";

/** Hydrates the page prerender.ts wrote into its HTML's #root. */
export function hydrate(Page: ComponentType) {
	const root = document.getElementById("root");
	if (!root) throw new Error(`${location.pathname} has no #root`);
	hydrateRoot(root, <Page />);
}
