import { defineOqtoApp } from "@/mini-apps/sdk";
import { LayoutGridIcon } from "lucide-react";
import { PhotoGridApp } from "./PhotoGridApp";

export const photoGridApp = defineOqtoApp({
	id: "photo-grid",
	title: "Photo Grid",
	description: "Arrange photos in a resizable grid with per-tile crop framing",
	icon: LayoutGridIcon,
	component: PhotoGridApp,
	requestedCapabilities: ["files", "notifications", "theme"],
});
