import { Button } from "@/components/ui/button";
import { ImagePlusIcon } from "lucide-react";
import type { RefObject } from "react";
import type { SelectionRect } from "../types";
import { CropOverlay } from "./CropOverlay";

const CHECKERBOARD =
	"repeating-conic-gradient(var(--muted) 0% 25%, var(--background) 0% 50%) 50% / 16px 16px";

export interface EditorCanvasProps {
	containerRef: RefObject<HTMLDivElement | null>;
	empty: boolean;
	cropping: boolean;
	onRequestImage: () => void;
	onApplyCrop: (selection: SelectionRect) => void;
	onCancelCrop: () => void;
}

export function EditorCanvas({
	containerRef,
	empty,
	cropping,
	onRequestImage,
	onApplyCrop,
	onCancelCrop,
}: EditorCanvasProps) {
	return (
		<div className="relative h-full w-full overflow-hidden">
			<div
				ref={containerRef}
				className="h-full w-full"
				style={{ background: CHECKERBOARD }}
			/>
			{cropping && !empty ? (
				<CropOverlay onApply={onApplyCrop} onCancel={onCancelCrop} />
			) : null}
			{empty ? (
				<div className="absolute inset-0 flex flex-col items-center justify-center gap-4 text-center">
					<ImagePlusIcon className="size-10 text-muted-foreground" />
					<div className="flex flex-col gap-1">
						<p className="text-sm font-bold">No image</p>
						<p className="text-xs text-muted-foreground">
							Open an image to start editing.
						</p>
					</div>
					<Button onClick={onRequestImage}>
						<ImagePlusIcon className="size-4" />
						Open image
					</Button>
				</div>
			) : null}
		</div>
	);
}
