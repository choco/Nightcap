import { useEffect, useState } from "react";

const weekday = new Intl.DateTimeFormat(undefined, { weekday: "short" });
const time = new Intl.DateTimeFormat(undefined, {
	hour: "numeric",
	minute: "2-digit",
});

/** A menu bar's clock, at the visitor's own time in their locale's format: the weekday and the
 *  time, or with `timeOnly` the time. The prerendered page shows a fixed evening until it
 *  hydrates. */
export function Clock({
	className,
	timeOnly,
}: {
	className?: string;
	timeOnly?: boolean;
}) {
	const [now, setNow] = useState<Date | null>(null);
	useEffect(() => {
		let timer: ReturnType<typeof setTimeout>;
		const tick = () => {
			const date = new Date();
			setNow(date);
			timer = setTimeout(tick, 60000 - (date.getTime() % 60000));
		};
		tick();
		return () => clearTimeout(timer);
	}, []);
	const shown = now ? time.format(now) : "23:41";
	return (
		<span className={className}>
			{timeOnly ? shown : `${now ? weekday.format(now) : "Wed"} ${shown}`}
		</span>
	);
}
