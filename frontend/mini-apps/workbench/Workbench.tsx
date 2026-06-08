import {
	Empty,
	EmptyDescription,
	EmptyHeader,
	EmptyTitle,
} from "@/components/common/empty";
import { OqtoAppShell } from "@/mini-apps/shell";
import { useState } from "react";
import { standaloneApps } from "./registry";

/**
 * The standalone workbench. With a single registered app it mounts directly;
 * with several it shows a launcher. Apps mount inside OqtoAppShell so they get
 * the oqto design system and a mock host, no backend required.
 */
export function Workbench() {
	const [selectedId, setSelectedId] = useState<string | null>(
		standaloneApps.length === 1 ? (standaloneApps[0]?.id ?? null) : null,
	);

	const app = standaloneApps.find((a) => a.id === selectedId) ?? null;

	if (app) {
		return (
			<OqtoAppShell
				app={app}
				onExit={
					standaloneApps.length > 1 ? () => setSelectedId(null) : undefined
				}
			/>
		);
	}

	if (standaloneApps.length === 0) {
		return (
			<div className="flex h-dvh items-center justify-center bg-background text-foreground">
				<Empty>
					<EmptyHeader>
						<EmptyTitle>No mini-apps registered</EmptyTitle>
						<EmptyDescription>
							Add an app to mini-apps/workbench/registry.ts to launch it here.
						</EmptyDescription>
					</EmptyHeader>
				</Empty>
			</div>
		);
	}

	return (
		<div className="flex h-dvh flex-col bg-background text-foreground">
			<header className="border-b border-border px-6 py-4">
				<h1 className="text-sm font-bold">oqto mini-app workbench</h1>
				<p className="text-xs text-muted-foreground">
					Standalone prototypes on the oqto design system.
				</p>
			</header>
			<main className="grid flex-1 content-start gap-4 overflow-y-auto p-6 sm:grid-cols-2 lg:grid-cols-3">
				{standaloneApps.map((entry) => (
					<button
						key={entry.id}
						type="button"
						className="flex flex-col items-start gap-2 border border-border bg-card p-4 text-left transition-colors hover:border-primary"
						onClick={() => setSelectedId(entry.id)}
					>
						<div className="flex items-center gap-2">
							{entry.icon ? <entry.icon className="size-5" /> : null}
							<span className="text-sm font-bold">{entry.title}</span>
						</div>
						{entry.description ? (
							<span className="text-xs text-muted-foreground">
								{entry.description}
							</span>
						) : null}
					</button>
				))}
			</main>
		</div>
	);
}
