/** Matches phones: narrower than the theme's sm breakpoint (main.css). */
export function phoneQuery(): MediaQueryList {
	const sm = getComputedStyle(document.documentElement).getPropertyValue(
		"--breakpoint-sm",
	);
	if (!sm) throw new Error("the page has no --breakpoint-sm");
	return window.matchMedia(`(width < ${sm})`);
}
