import { Button } from "@/components/ui/button";
import { useOqtoHost } from "@/mini-apps/sdk";
import { DownloadIcon, FolderOpenIcon } from "lucide-react";
import { useCallback } from "react";
import { AdjustmentsPanel } from "./components/AdjustmentsPanel";
import { EditorCanvas } from "./components/EditorCanvas";
import { LayerPanel } from "./components/LayerPanel";
import { Toolbar } from "./components/Toolbar";
import { usePixiEditor } from "./hooks/usePixiEditor";
import { cropImageToBlob, selectionToSourceRect } from "./lib/crop";
import { usePhotoEditorState } from "./state/usePhotoEditorState";
import {
	type Adjustments,
	DEFAULT_ADJUSTMENTS,
	type SelectionRect,
} from "./types";

export function PhotoEditorApp() {
	const host = useOqtoHost();
	const [state, dispatch] = usePhotoEditorState();
	const editor = usePixiEditor();

	const hasImage = state.image !== null;

	const handleOpen = useCallback(async () => {
		const ref = await host.files.pick({ accept: "image/*" });
		if (!ref) return;
		const blob = await host.files.read(ref);
		const info = await editor.loadImageFromBlob(blob, ref.name);
		if (!info) {
			host.notifications.notify("Could not load image", "error");
			return;
		}
		dispatch({ type: "setImage", image: info });
		editor.setAdjustments(state.adjustments);
		editor.setLayer(state.layer.visible, state.layer.opacity);
		host.notifications.notify(`Loaded ${info.name}`, "success");
	}, [host, editor, dispatch, state.adjustments, state.layer]);

	const handleAdjustmentChange = useCallback(
		(key: keyof Adjustments, value: number) => {
			const next: Adjustments = { ...state.adjustments, [key]: value };
			dispatch({ type: "setAdjustment", key, value });
			editor.setAdjustments(next);
		},
		[editor, dispatch, state.adjustments],
	);

	const handleResetAdjustments = useCallback(() => {
		dispatch({ type: "resetAdjustments" });
		editor.setAdjustments(DEFAULT_ADJUSTMENTS);
	}, [editor, dispatch]);

	const handleToggleVisible = useCallback(() => {
		const visible = !state.layer.visible;
		dispatch({ type: "setLayerVisible", visible });
		editor.setLayer(visible, state.layer.opacity);
	}, [editor, dispatch, state.layer]);

	const handleOpacityChange = useCallback(
		(opacity: number) => {
			dispatch({ type: "setLayerOpacity", opacity });
			editor.setLayer(state.layer.visible, opacity);
		},
		[editor, dispatch, state.layer.visible],
	);

	const handleApplyCrop = useCallback(
		async (selection: SelectionRect) => {
			const viewport = editor.getViewport();
			const source = editor.getSource();
			if (!viewport || !source) return;
			const srcRect = selectionToSourceRect(selection, viewport);
			const blob = await cropImageToBlob(source, srcRect);
			if (!blob) {
				host.notifications.notify("Crop failed", "error");
				return;
			}
			const name = state.image?.name ?? "cropped.png";
			const info = await editor.loadImageFromBlob(blob, name);
			if (!info) return;
			dispatch({ type: "setImage", image: info });
			dispatch({ type: "setTool", tool: "move" });
			editor.setAdjustments(state.adjustments);
			editor.setLayer(state.layer.visible, state.layer.opacity);
			host.notifications.notify("Cropped", "success");
		},
		[editor, host, dispatch, state.image, state.adjustments, state.layer],
	);

	const handleCancelCrop = useCallback(() => {
		dispatch({ type: "setTool", tool: "move" });
	}, [dispatch]);

	const handleExport = useCallback(async () => {
		const blob = await editor.exportBlob();
		if (!blob) {
			host.notifications.notify("Nothing to export", "error");
			return;
		}
		const base = state.image?.name?.replace(/\.[^.]+$/, "") ?? "image";
		await host.files.write(`${base}-edited.png`, blob);
		host.notifications.notify("Exported PNG", "success");
	}, [editor, host, state.image]);

	return (
		<div className="flex h-full flex-col">
			<div className="flex items-center justify-end gap-2 border-b border-border px-3 py-2">
				<Button variant="outline" size="sm" onClick={handleOpen}>
					<FolderOpenIcon className="size-4" />
					Open
				</Button>
				<Button size="sm" onClick={handleExport} disabled={!hasImage}>
					<DownloadIcon className="size-4" />
					Export
				</Button>
			</div>
			<div className="flex min-h-0 flex-1 flex-col md:flex-row">
				<Toolbar
					tool={state.tool}
					disabled={!hasImage}
					onSelectTool={(tool) => dispatch({ type: "setTool", tool })}
				/>
				<div className="min-h-[40vh] min-w-0 flex-1 md:min-h-0">
					<EditorCanvas
						containerRef={editor.containerRef}
						empty={!hasImage}
						cropping={state.tool === "crop"}
						onRequestImage={handleOpen}
						onApplyCrop={handleApplyCrop}
						onCancelCrop={handleCancelCrop}
					/>
				</div>
				<aside className="max-h-[42vh] w-full shrink-0 overflow-y-auto border-t border-border p-4 md:max-h-none md:w-64 md:border-t-0 md:border-l">
					<div className="flex flex-col gap-6">
						<AdjustmentsPanel
							adjustments={state.adjustments}
							disabled={!hasImage}
							onChange={handleAdjustmentChange}
							onReset={handleResetAdjustments}
						/>
						<LayerPanel
							image={state.image}
							layer={state.layer}
							onToggleVisible={handleToggleVisible}
							onOpacityChange={handleOpacityChange}
						/>
					</div>
				</aside>
			</div>
		</div>
	);
}
