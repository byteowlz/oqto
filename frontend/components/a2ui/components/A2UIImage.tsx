/**
 * A2UI Image Component
 */

import type { ImageComponent } from "@/lib/a2ui/types";
import { resolveBoundValue } from "@/lib/a2ui/types";
import { cn } from "@/lib/utils";

interface A2UIImageProps {
	props: Record<string, unknown>;
	dataModel: Record<string, unknown>;
}

const fitClasses: Record<string, string> = {
	contain: "object-contain",
	cover: "object-cover",
	fill: "object-fill",
	none: "object-none",
	"scale-down": "object-scale-down",
};

const sizeClasses: Record<string, string> = {
	icon: "w-6 h-6",
	avatar: "w-10 h-10 rounded-full",
	smallFeature: "w-24 h-24",
	mediumFeature: "w-48 h-48",
	largeFeature: "w-96 h-96",
	header: "w-full h-48",
};

export function A2UIImage({ props, dataModel }: A2UIImageProps) {
	const imageProps = props as unknown as ImageComponent;
	const url = resolveBoundValue(imageProps.url, dataModel, "");
	const fit = imageProps.fit || "contain";
	const usageHint = imageProps.usageHint;

	if (!url) {
		return (
			<div className="bg-muted flex items-center justify-center text-muted-foreground text-sm w-24 h-24 rounded">
				No image
			</div>
		);
	}

	const className = cn(
		fitClasses[fit],
		usageHint ? sizeClasses[usageHint] : "max-w-full h-auto",
	);

	return <img src={url} alt="" className={className} loading="lazy" />;
}
