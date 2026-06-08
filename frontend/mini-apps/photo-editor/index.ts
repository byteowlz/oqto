import { defineOqtoApp } from "@/mini-apps/sdk";
import { ImageIcon } from "lucide-react";
import { PhotoEditorApp } from "./PhotoEditorApp";

export const photoEditorApp = defineOqtoApp({
	id: "photo-editor",
	title: "Photo Editor",
	description: "GPU image adjustments and crop, powered by PixiJS",
	icon: ImageIcon,
	component: PhotoEditorApp,
	requestedCapabilities: ["files", "notifications", "theme"],
});
