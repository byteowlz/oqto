import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { UploadIcon, XIcon } from "lucide-react";
import type { PoolImage } from "../types";
import { DRAG_MIME } from "./drag";

export interface PhotoTrayProps {
	pool: PoolImage[];
	assignedIds: Set<string>;
	canAssign: boolean;
	onUpload: () => void;
	onRemove: (id: string) => void;
	onPickImage: (id: string) => void;
}

export function PhotoTray({
	pool,
	assignedIds,
	canAssign,
	onUpload,
	onRemove,
	onPickImage,
}: PhotoTrayProps) {
	return (
		<div className="flex h-full flex-col gap-3">
			<div className="flex items-center justify-between">
				<h3 className="text-xs font-bold uppercase tracking-wide text-muted-foreground">
					Photos ({pool.length})
				</h3>
				<Button variant="outline" size="sm" onClick={onUpload}>
					<UploadIcon className="size-4" />
					Upload
				</Button>
			</div>
			{pool.length === 0 ? (
				<p className="text-xs text-muted-foreground">
					Upload photos, then drag them onto tiles (or select a tile and click a
					photo).
				</p>
			) : (
				<div className="grid grid-cols-4 gap-2 overflow-y-auto sm:grid-cols-6 md:grid-cols-3">
					{pool.map((image) => (
						<div
							key={image.id}
							className="group relative aspect-square overflow-hidden border border-border"
						>
							<button
								type="button"
								className={cn(
									"block h-full w-full",
									canAssign && "cursor-pointer",
								)}
								draggable
								onDragStart={(e) => {
									e.dataTransfer.setData(DRAG_MIME, image.id);
									e.dataTransfer.effectAllowed = "copy";
								}}
								onClick={() => onPickImage(image.id)}
								title={image.name}
							>
								<img
									src={image.url}
									alt={image.name}
									className="pointer-events-none h-full w-full object-cover"
									draggable={false}
								/>
							</button>
							{assignedIds.has(image.id) ? (
								<span className="absolute left-1 top-1 size-2 rounded-full bg-primary" />
							) : null}
							<button
								type="button"
								className="absolute right-1 top-1 z-10 hidden border border-border bg-popover p-0.5 text-popover-foreground group-hover:block"
								onClick={() => onRemove(image.id)}
								aria-label={`Remove ${image.name}`}
							>
								<XIcon className="size-3" />
							</button>
						</div>
					))}
				</div>
			)}
		</div>
	);
}
