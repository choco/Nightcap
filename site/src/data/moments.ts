import {
	type AppName,
	FIGMA_ALWAYS,
	FIGMA_RULE,
	FOLLOWING,
	type Moon,
	SUNRISE,
} from "./menus";

/** One moment of an evening: what it does to the screen and to the menu's Night Shift item and
 *  front app's row. */
interface Moment {
	time: string;
	title: string;
	warm: boolean;
	body: string;
	moon: Moon;
	reason: string[];
	app: AppName;
	item: string;
	note?: string[];
}

export const MOMENTS: Moment[] = [
	{
		time: "18:36",
		title: "Sunset",
		warm: true,
		body: "The schedule starts Night Shift, and Nightcap leaves it alone while Mail is in front.",
		moon: "on",
		reason: FOLLOWING,
		app: "mail",
		item: "Turn Off in Mail",
	},
	{
		time: "21:15",
		title: "You open Figma",
		warm: false,
		body: "Figma has a rule, so Nightcap switches Night Shift off. The colours on screen are the colours in the file.",
		moon: "forced",
		reason: FIGMA_RULE,
		app: "figma",
		item: "Turn On in Figma",
		note: FIGMA_ALWAYS,
	},
	{
		time: "22:40",
		title: "Back to Mail",
		warm: true,
		body: "Mail has no rule. Night Shift comes straight back, warm as before.",
		moon: "on",
		reason: FOLLOWING,
		app: "mail",
		item: "Turn Off in Mail",
	},
	{
		time: "23:30",
		title: "You want it warm",
		warm: true,
		body: `You choose “Turn On in Figma” › “Until ${SUNRISE}”, or turn Night Shift on in Control Center. Either way, the Figma rule snoozes until sunrise.`,
		moon: "on",
		reason: [`On in Figma until ${SUNRISE}`],
		app: "figma",
		item: "Turn Off in Figma",
		note: [`On until ${SUNRISE}, then`, "your config’s rule"],
	},
	{
		time: SUNRISE,
		title: "Sunrise",
		warm: false,
		body: "The schedule ends Night Shift, and the snooze ends with it. Tonight, Figma is true colour again.",
		moon: "off",
		reason: FIGMA_RULE,
		app: "figma",
		item: "Turn On in Figma",
		note: FIGMA_ALWAYS,
	},
];
