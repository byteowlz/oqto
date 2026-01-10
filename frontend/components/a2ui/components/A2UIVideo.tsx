/**
 * A2UI Video Component
 */

import { resolveBoundValue } from "@/lib/a2ui/types";

interface A2UIVideoProps {
	props: Record<string, unknown>;
	dataModel: Record<string, unknown>;
}

export function A2UIVideo({ props, dataModel }: A2UIVideoProps) {
	const url = resolveBoundValue(
		props.url as { literalString?: string; path?: string },
		dataModel,
		"",
	);

	if (!url) {
		return (
			<div className="text-muted-foreground text-sm p-4 border border-dashed rounded">
				No video URL provided
			</div>
		);
	}

	return (
		<video src={url} controls className="w-full max-w-full rounded" playsInline>
			<track kind="captions" />
			Your browser does not support the video tag.
		</video>
	);
}
