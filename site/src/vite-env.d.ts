interface ImportMetaEnv {
	/** The newest release's version and the address of its DMG (or the zip of a release that has
	 *  none), set by the Pages deploy. */
	readonly VITE_RELEASE_VERSION?: string;
	readonly VITE_RELEASE_URL?: string;
}
