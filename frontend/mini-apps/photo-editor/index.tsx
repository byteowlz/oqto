import { defineOqtoApp } from "@/mini-apps/sdk";
import { Image } from "lucide-react";

function PhotoEditorApp() {
	return (
		<div className="h-full bg-background p-4 text-sm text-muted-foreground">
			Photo editor workbench app placeholder
		</div>
	);
}

export const photoEditorApp = defineOqtoApp({
	id: "photo-editor",
	title: "Photo Editor",
	description: "Edit images in the oqto mini-app workbench.",
	icon: Image,
	component: PhotoEditorApp,
	requestedCapabilities: ["files"],
});
