import { photoEditorApp } from "@/mini-apps/photo-editor";
import { photoGridApp } from "@/mini-apps/photo-grid";
import type { OqtoApp } from "@/mini-apps/sdk";

/**
 * Apps available in the standalone workbench. This is workbench-only and is
 * deliberately NOT the oqto sidebar app registry (@/lib/app-registry) -- a
 * standalone mini-app must not depend on the running oqto shell.
 */
export const standaloneApps: ReadonlyArray<OqtoApp> = [
	photoEditorApp,
	photoGridApp,
];
