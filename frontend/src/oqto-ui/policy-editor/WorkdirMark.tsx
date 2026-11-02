import type { ReactNode } from "react";

interface WorkdirMarkProps {
	name: string;
	icon?: ReactNode;
}
/** Presentation only: a missing mark never changes workspace identity. */
export function WorkdirMark({ name, icon }: WorkdirMarkProps) {
	const initial = Array.from(name.trim())[0]?.toLocaleUpperCase() ?? "?";
	return (
		<span className="policy-workdir-mark" aria-hidden="true">
			{icon ?? initial}
		</span>
	);
}
