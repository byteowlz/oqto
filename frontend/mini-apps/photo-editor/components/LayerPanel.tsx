import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import { Slider } from "@/components/ui/slider";
import { EyeIcon, EyeOffIcon, ImageIcon } from "lucide-react";
import type { ImageInfo, ImageLayer } from "../types";

export interface LayerPanelProps {
	image: ImageInfo | null;
	layer: ImageLayer;
	onToggleVisible: () => void;
	onOpacityChange: (opacity: number) => void;
}

export function LayerPanel({
	image,
	layer,
	onToggleVisible,
	onOpacityChange,
}: LayerPanelProps) {
	return (
		<section className="flex flex-col gap-3">
			<h3 className="text-xs font-bold uppercase tracking-wide text-muted-foreground">
				Layers
			</h3>
			{image ? (
				<div className="flex flex-col gap-3 border border-border p-2">
					<div className="flex items-center gap-2">
						<ImageIcon className="size-4 text-muted-foreground" />
						<span className="min-w-0 flex-1 truncate text-sm">
							{image.name}
						</span>
						<Button
							variant="ghost"
							size="icon"
							onClick={onToggleVisible}
							aria-pressed={layer.visible}
							title={layer.visible ? "Hide layer" : "Show layer"}
						>
							{layer.visible ? (
								<EyeIcon className="size-4" />
							) : (
								<EyeOffIcon className="size-4 text-muted-foreground" />
							)}
						</Button>
					</div>
					<div className="flex flex-col gap-1.5">
						<div className="flex items-center justify-between">
							<Label className="text-xs text-muted-foreground">Opacity</Label>
							<span className="text-xs tabular-nums text-muted-foreground">
								{Math.round(layer.opacity * 100)}%
							</span>
						</div>
						<Slider
							value={[layer.opacity]}
							min={0}
							max={1}
							step={0.01}
							onValueChange={(values) =>
								onOpacityChange(values[0] ?? layer.opacity)
							}
						/>
					</div>
					<p className="text-xs text-muted-foreground">
						{image.width} x {image.height}
					</p>
				</div>
			) : (
				<p className="text-xs text-muted-foreground">No image loaded.</p>
			)}
		</section>
	);
}
