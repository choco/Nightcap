/** Class names, leaving out the ones that don't apply. */
export const cx = (...names: (string | false | null | undefined)[]) => names.filter(Boolean).join(' ');
