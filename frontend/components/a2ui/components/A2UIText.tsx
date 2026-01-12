/**
 * A2UI Text Component
 */

import type { TextComponent } from "@/lib/a2ui/types";
import { resolveBoundValue } from "@/lib/a2ui/types";
import { cn } from "@/lib/utils";

interface A2UITextProps {
	props: Record<string, unknown>;
	dataModel: Record<string, unknown>;
}

export function A2UIText({ props, dataModel }: A2UITextProps) {
	const textProps = props as unknown as TextComponent;
	const text = resolveBoundValue(textProps.text, dataModel, "");
	const usageHint = textProps.usageHint;

	const className = cn(
		usageHint === "h1" && "text-3xl font-bold",
		usageHint === "h2" && "text-2xl font-semibold",
		usageHint === "h3" && "text-xl font-semibold",
		usageHint === "h4" && "text-lg font-medium",
		usageHint === "h5" && "text-base font-medium",
		usageHint === "caption" && "text-sm text-muted-foreground",
		usageHint === "body" && "text-base",
		!usageHint && "text-base",
	);

	// Use semantic elements based on usage hint
	switch (usageHint) {
		case "h1":
			return <h1 className={className}>{text}</h1>;
		case "h2":
			return <h2 className={className}>{text}</h2>;
		case "h3":
			return <h3 className={className}>{text}</h3>;
		case "h4":
			return <h4 className={className}>{text}</h4>;
		case "h5":
			return <h5 className={className}>{text}</h5>;
		case "caption":
			return <span className={className}>{text}</span>;
		default:
			return <p className={className}>{text}</p>;
	}
}
