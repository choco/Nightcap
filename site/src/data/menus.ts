// Nightcap's menu as the app builds it, for whatever is in front. A subtitle comes in the lines
// the app breaks it into (StatusMenu.wrapped): whole up to 160pt of 12pt text, otherwise at the
// word that evens out two lines. A site's row names the site on the first.

export type Moon = "on" | "forced" | "off";
export type AppName = "mail" | "figma" | "safari";
export type Glyph = "timer" | "globe" | "gear" | "power";

export const APPS: Record<
	AppName,
	{ name: string; letter: string; tile: string }
> = {
	mail: { name: "Mail", letter: "M", tile: "#3d6fd6" },
	figma: { name: "Figma", letter: "F", tile: "#a259ff" },
	safari: { name: "Safari", letter: "S", tile: "#1d9bf0" },
};

export type MenuEntry =
	// The Night Shift item, everywhere's switch: checked while it's on everywhere, which a
	// forced-off moon still means, and what decides the screen.
	| { kind: "nightShift"; moon: Moon; reason: string[] }
	| { kind: "strength"; value: number }
	| { kind: "separator" }
	| {
			kind: "item";
			title: string;
			image: { app: AppName } | { glyph: Glyph };
			subtitle?: string[];
			key?: string;
			submenu?: boolean;
			indented?: boolean;
	  };

export const SUNRISE = "07:52";
export const KEYS = "⌃⌥⌘N";

type Item = Extract<MenuEntry, { kind: "item" }>;
// A row that opens a submenu of lengths: everywhere's, under the switch, or the front app's or
// site's.
const row = (
	title: string,
	image: Item["image"],
	extra: Partial<Item> = {},
): Item => ({
	kind: "item",
	title,
	image,
	submenu: true,
	...extra,
});
export const nightShift = (moon: Moon, reason: string[]): MenuEntry => ({
	kind: "nightShift",
	moon,
	reason,
});
const strength: MenuEntry = { kind: "strength", value: 0.62 };
export const separator: MenuEntry = { kind: "separator" };
const turnOff = row("Turn Off", { glyph: "timer" });
const tail: MenuEntry[] = [
	{ kind: "item", title: "Settings…", key: "⌘,", image: { glyph: "gear" } },
	{
		kind: "item",
		title: "Quit Nightcap",
		key: "⌘Q",
		image: { glyph: "power" },
	},
];

export const FOLLOWING = ["Following the", `schedule until ${SUNRISE}`];
export const FIGMA_RULE = ["Off in Figma:", "your config’s rule"];
export const FIGMA_ALWAYS = ["Always off: your", "config’s rule"];

export const MENUS: Record<AppName, MenuEntry[]> = {
	mail: [
		nightShift("on", FOLLOWING),
		strength,
		turnOff,
		separator,
		row("Turn Off in Mail", { app: "mail" }),
		separator,
		...tail,
	],
	figma: [
		nightShift("forced", FIGMA_RULE),
		strength,
		turnOff,
		separator,
		row("Turn On in Figma", { app: "figma" }, { subtitle: FIGMA_ALWAYS }),
		separator,
		...tail,
	],
	safari: [
		nightShift("forced", ["Off at youtube.com:", "your rule"]),
		strength,
		turnOff,
		separator,
		row("Turn Off in Safari", { app: "safari" }),
		row(
			"Turn On at This Site",
			{ glyph: "globe" },
			{ indented: true, subtitle: ["youtube.com", "Always off: your rule"] },
		),
		separator,
		...tail,
	],
};

/** The front app's row, alone. */
export const placeRow = (
	title: string,
	app: AppName,
	subtitle?: string[],
): MenuEntry => row(title, { app }, subtitle ? { subtitle } : {});
