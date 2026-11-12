// The Nightcap components' types, from the site's source (pnpm --dir site bundle).
import type { HTMLAttributes, ReactNode } from "react";

// components/Button
export interface ButtonProps {
    href: string;
    primary?: boolean;
    large?: boolean;
    children: ReactNode;
}
/** A capsule link that springs on hover: `primary`, in moon with a gold edge, for the one action a
 *  view is about, and outlined for the rest. `large` is for the download button in the hero. */
export declare function Button({ href, primary, large, children }: ButtonProps): import("react").JSX.Element;

// data/menus
export type Moon = "on" | "forced" | "off";
export type AppName = "mail" | "figma" | "safari";
export type Glyph = "timer" | "globe" | "gear" | "power";
export declare const APPS: Record<AppName, {
    name: string;
    letter: string;
    tile: string;
}>;
export declare const APP_NAMES: AppName[];
/** The Night Shift item, everywhere's switch: checked while it's on everywhere, which a
 *  forced-off moon still means, and what decides the screen, warm only while the moon is on. */
export interface NightShift {
    moon: Moon;
    reason: string[];
}
export type MenuEntry = ({
    kind: "nightShift";
} & NightShift) | {
    kind: "strength";
    value: number;
} | {
    kind: "separator";
} | {
    kind: "item";
    title: string;
    image: {
        app: AppName;
    } | {
        glyph: Glyph;
    };
    subtitle?: string[];
    key?: string;
    submenu?: boolean;
    indented?: boolean;
};
export declare const SUNRISE = "07:52";
export declare const KEYS = "\u2303\u2325\u2318N";
export declare const nightShift: ({ moon, reason }: NightShift) => MenuEntry;
export declare const separator: MenuEntry;
export declare const FOLLOWING: string[];
export declare const FIGMA_RULE: string[];
export declare const FIGMA_ALWAYS: string[];
/** The Night Shift item with each app in front. */
export declare const NIGHT_SHIFT: Record<AppName, NightShift>;
export declare const MENUS: Record<AppName, MenuEntry[]>;
/** The front app's row, alone. */
export declare const placeRow: (title: string, app: AppName, subtitle?: string[]) => MenuEntry;

// components/Menu
export type MenuProps = HTMLAttributes<HTMLDivElement> & {
    entries: MenuEntry[];
    look: "mac" | "night";
};
/** Nightcap's menu, with the rows the app would show: in macOS's own look (`mac`), sized in em of
 *  the text its parent sets, 13px at the real menu's size, or as a night panel in the site's
 *  colours (`night`). */
export declare function Menu({ entries, look, className, ...rest }: MenuProps): import("react").JSX.Element;

// components/Scene
/** Nightcap in the menu bar with its menu open, while a rule keeps Night Shift off in Figma. */
export declare function Scene(): import("react").JSX.Element;

// components/Terminal
export type TerminalProps = HTMLAttributes<HTMLElement> & {
    children: ReactNode;
};
/** A terminal session with the `nightcap` command: a night panel in either theme, glowing gold.
 *  Each line is a `Command` or its `Output`, and long ones wrap, as a terminal window's would. */
export declare function Terminal({ className, children, ...figure }: TerminalProps): import("react").JSX.Element;
/** A command line: the prompt, `nightcap` and what follows it. */
export declare const Command: ({ children }: {
    children: ReactNode;
}) => import("react").JSX.Element;
/** What a command printed. */
export declare const Output: ({ children }: {
    children: ReactNode;
}) => import("react").JSX.Element;
/** An argument, in gold. It stays whole: a line breaks between arguments, never at a flag's
 *  hyphens. */
export declare const Arg: ({ children }: {
    children: ReactNode;
}) => import("react").JSX.Element;
/** Night Shift's state in output: "off" warm, "on" in the check mark's green. */
export declare const State: ({ on }: {
    on?: boolean;
}) => import("react").JSX.Element;

// bundle

