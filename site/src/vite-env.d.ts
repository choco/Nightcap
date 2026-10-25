interface ImportMetaEnv {
	/** The newest release's version and the address of its DMG (or the zip of a release that has
	 *  none), set by the Pages deploy. */
	readonly VITE_RELEASE_VERSION?: string;
	readonly VITE_RELEASE_URL?: string;
}

/** The repo's CHANGELOG.md, rendered at build time by plugins/changelog.ts. */
declare module "virtual:changelog" {
	export const title: string;
	/** The paragraph under the title, as plain text. */
	export const lead: string;
	/** Every version, each heading anchored as `v<version>`. */
	export const html: string;
}
