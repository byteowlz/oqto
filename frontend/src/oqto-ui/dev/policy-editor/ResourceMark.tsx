import type { IconName, Resource } from "./resources";

const PATHS: Record<IconName, string> = {
	code: "m8 7-5 5 5 5m8-10 5 5-5 5m-3-13-2 16",
	book: "M3 4h7l2 2 2-2h7v15h-7l-2 2-2-2H3ZM12 6v15",
	memory: "M12 3c-5 0-7 4-7 9s2 9 7 9 7-4 7-9-2-9-7-9Zm-4 6h8m-8 6h8M12 3v18",
	team: "M8 12a4 4 0 1 0 0-8 4 4 0 0 0 0 8Zm8-7a3 3 0 0 1 0 6M2 21v-3c0-5 12-5 12 0v3m3-7c4 0 5 2 5 4v3",
	flask: "M9 3h6M10 3v6l-6 10a1 1 0 0 0 1 2h14a1 1 0 0 0 1-2L14 9V3M7 15h10",
	globe:
		"M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18Zm-9 9h18M12 3c3 3 3 15 0 18M12 3c-3 3-3 15 0 18",
	shield: "M12 3 4 6v6c0 5 3.5 8 8 9 4.5-1 8-4 8-9V6Z",
	phone: "M7 2h10v20H7ZM11 18h2",
	key: "M21 2l-9.6 9.6m-2.4 8.4a5 5 0 1 1-7-7 5 5 0 0 1 7 7Zm0 0L15 8m-2 2 2.5 2.5",
	terminal: "M4 17l6-5-6-5M12 19h8",
	database:
		"M12 3c4.4 0 8 1 8 3v12c0 2-3.6 3-8 3s-8-1-8-3V6c0-2 3.6-3 8-3Zm-8 3c0 2 3.6 3 8 3s8-1 8-3M4 12c0 2 3.6 3 8 3s8-1 8-3",
};
interface ResourceMarkProps {
	resource: Resource;
}
/** Configured icon when one exists, otherwise the workdir letter tile. */
export function ResourceMark({ resource }: ResourceMarkProps) {
	const letter = Array.from(resource.name.trim())[0]?.toLocaleUpperCase();
	return (
		<span
			className="resource-mark"
			data-kind={resource.kind}
			aria-hidden="true"
		>
			{resource.icon ? (
				<svg
					aria-hidden="true"
					width="16"
					height="16"
					viewBox="0 0 24 24"
					fill="none"
					stroke="currentColor"
					strokeWidth="1.5"
				>
					<path d={PATHS[resource.icon]} />
				</svg>
			) : (
				(letter ?? "?")
			)}
		</span>
	);
}
