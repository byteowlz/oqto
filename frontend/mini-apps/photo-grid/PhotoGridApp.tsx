import { Button } from "@/components/ui/button";
import { useMountEffect } from "@/hooks/use-mount-effect";
import { useOqtoHost } from "@/mini-apps/sdk";
import { DownloadIcon, LayoutGridIcon } from "lucide-react";
import { useMemo, useRef, useState } from "react";
import { GridCanvas } from "./components/GridCanvas";
import { GridSizePicker } from "./components/GridSizePicker";
import { PhotoTray } from "./components/PhotoTray";
import { composeGrid, poolById } from "./lib/cover";
import { usePhotoGridState } from "./state/usePhotoGridState";
import type { PoolImage } from "./types";

const EXPORT_MAX_DIM = 2200;

export function PhotoGridApp() {
	const host = useOqtoHost();
	const [state, dispatch] = usePhotoGridState();
	const [phase, setPhase] = useState<"setup" | "compose">("setup");
	const [selectedIndex, setSelectedIndex] = useState<number | null>(null);

	const gridRef = useRef<HTMLDivElement | null>(null);
	const imageElsRef = useRef<Map<string, HTMLImageElement>>(new Map());
	const objectUrlsRef = useRef<Set<string>>(new Set());

	useMountEffect(() => () => {
		for (const url of objectUrlsRef.current) URL.revokeObjectURL(url);
		objectUrlsRef.current.clear();
		imageElsRef.current.clear();
	});

	const imagesById = useMemo(() => poolById(state.pool), [state.pool]);
	const assignedIds = useMemo(
		() =>
			new Set(
				state.tiles
					.map((t) => t.imageId)
					.filter((id): id is string => id !== null),
			),
		[state.tiles],
	);

	const handleUpload = async () => {
		const refs = await host.files.pickMultiple({ accept: "image/*" });
		if (refs.length === 0) return;
		const loaded = await Promise.all(
			refs.map(async (ref): Promise<PoolImage | null> => {
				try {
					const blob = await host.files.read(ref);
					const url = URL.createObjectURL(blob);
					objectUrlsRef.current.add(url);
					const img = new Image();
					img.src = url;
					await img.decode();
					imageElsRef.current.set(ref.id, img);
					return {
						id: ref.id,
						name: ref.name,
						url,
						width: img.naturalWidth,
						height: img.naturalHeight,
					};
				} catch {
					return null;
				}
			}),
		);
		const images = loaded.filter((p): p is PoolImage => p !== null);
		if (images.length === 0) {
			host.notifications.notify("Could not load images", "error");
			return;
		}
		dispatch({ type: "addPoolImages", images });
		host.notifications.notify(`Added ${images.length} photo(s)`, "success");
	};

	const handleRemovePoolImage = (id: string) => {
		const image = state.pool.find((p) => p.id === id);
		if (image) {
			URL.revokeObjectURL(image.url);
			objectUrlsRef.current.delete(image.url);
			imageElsRef.current.delete(id);
		}
		dispatch({ type: "removePoolImage", id });
	};

	const handlePickImage = (imageId: string) => {
		if (selectedIndex !== null) {
			dispatch({ type: "assignTile", index: selectedIndex, imageId });
			return;
		}
		const firstEmpty = state.tiles.findIndex((t) => t.imageId === null);
		if (firstEmpty >= 0) {
			dispatch({ type: "assignTile", index: firstEmpty, imageId });
		}
	};

	const handleSetSize = (rows: number, cols: number) => {
		dispatch({ type: "setSpec", rows, cols });
		setSelectedIndex(null);
	};

	const handleExport = async () => {
		const grid = gridRef.current;
		if (!grid) return;
		const rect = grid.getBoundingClientRect();
		if (rect.width === 0 || rect.height === 0) return;
		const scale = EXPORT_MAX_DIM / Math.max(rect.width, rect.height);
		const background =
			getComputedStyle(document.body).backgroundColor || "#000";
		const blob = await composeGrid(
			state,
			imageElsRef.current,
			rect.width * scale,
			rect.height * scale,
			state.gap * scale,
			background,
		);
		if (!blob) {
			host.notifications.notify("Export failed", "error");
			return;
		}
		await host.files.write("grid.png", blob);
		host.notifications.notify("Exported grid", "success");
	};

	if (phase === "setup") {
		return (
			<div className="flex h-full items-center justify-center p-6">
				<div className="flex w-full max-w-md flex-col gap-6 border border-border bg-card p-6">
					<div className="flex flex-col gap-1">
						<h2 className="text-base font-bold">New photo grid</h2>
						<p className="text-xs text-muted-foreground">
							Choose a grid size to start. You can change it later.
						</p>
					</div>
					<GridSizePicker
						rows={state.spec.rows}
						cols={state.spec.cols}
						onChange={handleSetSize}
					/>
					<div
						className="mx-auto grid aspect-square w-48 gap-1"
						style={{
							gridTemplateColumns: `repeat(${state.spec.cols}, 1fr)`,
							gridTemplateRows: `repeat(${state.spec.rows}, 1fr)`,
						}}
					>
						{Array.from({ length: state.spec.rows * state.spec.cols }).map(
							(_, i) => {
								// biome-ignore lint/suspicious/noArrayIndexKey: preview cells are positional
								return <div key={i} className="bg-muted/50" />;
							},
						)}
					</div>
					<Button onClick={() => setPhase("compose")}>
						<LayoutGridIcon className="size-4" />
						Create grid
					</Button>
				</div>
			</div>
		);
	}

	return (
		<div className="flex h-full flex-col">
			<div className="flex flex-wrap items-center justify-between gap-3 border-b border-border px-3 py-2">
				<div className="flex items-center gap-4">
					<GridSizePicker
						rows={state.spec.rows}
						cols={state.spec.cols}
						onChange={handleSetSize}
					/>
					<span className="hidden text-xs text-muted-foreground sm:inline">
						Drag the lines to resize. Drag inside a tile to reframe.
					</span>
				</div>
				<Button size="sm" onClick={handleExport}>
					<DownloadIcon className="size-4" />
					Export
				</Button>
			</div>
			<div className="flex min-h-0 flex-1 flex-col md:flex-row">
				<div className="min-h-[42vh] min-w-0 flex-1 md:min-h-0">
					<GridCanvas
						state={state}
						imagesById={imagesById}
						selectedIndex={selectedIndex}
						gridRef={gridRef}
						onSelectTile={setSelectedIndex}
						onAssignDrop={(index, imageId) =>
							dispatch({ type: "assignTile", index, imageId })
						}
						onPanChange={(index, posX, posY) =>
							dispatch({ type: "setTilePos", index, posX, posY })
						}
						onClear={(index) => dispatch({ type: "clearTile", index })}
						onColSizes={(sizes) => dispatch({ type: "setColSizes", sizes })}
						onRowSizes={(sizes) => dispatch({ type: "setRowSizes", sizes })}
						onMargins={(margins) => dispatch({ type: "setMargins", margins })}
					/>
				</div>
				<aside className="max-h-[40vh] w-full shrink-0 overflow-hidden border-t border-border p-4 md:max-h-none md:w-72 md:border-t-0 md:border-l">
					<PhotoTray
						pool={state.pool}
						assignedIds={assignedIds}
						canAssign={
							selectedIndex !== null ||
							state.tiles.some((t) => t.imageId === null)
						}
						onUpload={handleUpload}
						onRemove={handleRemovePoolImage}
						onPickImage={handlePickImage}
					/>
				</aside>
			</div>
		</div>
	);
}
