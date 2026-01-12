/**
 * A2UI Divider Component
 */

import { Separator } from "@/components/ui/separator";
import type { DividerComponent } from "@/lib/a2ui/types";

interface A2UIDividerProps {
	props: Record<string, unknown>;
}

export function A2UIDivider({ props }: A2UIDividerProps) {
	const dividerProps = props as unknown as DividerComponent;
	const axis = dividerProps.axis || "horizontal";

	return (
		<Separator
			orientation={axis === "vertical" ? "vertical" : "horizontal"}
			className={axis === "vertical" ? "h-full" : "w-full my-2"}
		/>
	);
}
