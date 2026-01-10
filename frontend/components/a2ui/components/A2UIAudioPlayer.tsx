/**
 * A2UI AudioPlayer Component
 */

import { resolveBoundValue } from "@/lib/a2ui/types";

interface A2UIAudioPlayerProps {
	props: Record<string, unknown>;
	dataModel: Record<string, unknown>;
}

export function A2UIAudioPlayer({ props, dataModel }: A2UIAudioPlayerProps) {
	const url = resolveBoundValue(
		props.url as { literalString?: string; path?: string },
		dataModel,
		"",
	);

	const description = resolveBoundValue(
		props.description as { literalString?: string; path?: string } | undefined,
		dataModel,
		"",
	);

	if (!url) {
		return (
			<div className="text-muted-foreground text-sm p-4 border border-dashed rounded">
				No audio URL provided
			</div>
		);
	}

	return (
		<div className="flex flex-col gap-1">
			{description && (
				<span className="text-sm text-muted-foreground">{description}</span>
			)}
			{/* biome-ignore lint/a11y/useMediaCaption: A2UI audio may not have captions */}
			<audio src={url} controls className="w-full">
				Your browser does not support the audio element.
			</audio>
		</div>
	);
}
