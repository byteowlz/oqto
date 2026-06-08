import { Button } from "@/components/ui/button";
import { useMountEffect } from "@/hooks/use-mount-effect";
import { createMockHost } from "@/mini-apps/sdk";
import { OqtoHostProvider } from "@/mini-apps/sdk";
import type { OqtoApp, OqtoHost } from "@/mini-apps/sdk";
import {
	type Base24Scheme,
	type ThemeMode,
	applyIdentityTokens,
	applyScheme,
	builtInSchemeList,
	builtInSchemes,
	defaultSchemeForMode,
} from "@/mini-apps/theming";
import { ChevronLeftIcon } from "lucide-react";
import { useCallback, useMemo, useRef, useState } from "react";
import { SchemePicker } from "./SchemePicker";
import { WorkbenchProviders } from "./WorkbenchProviders";

export interface OqtoAppShellProps {
	app: OqtoApp;
	/** Host to inject. Defaults to a fresh mock host wired to the shell theme. */
	host?: OqtoHost;
	initialSchemeId?: string;
	/** Show the scheme/mode chrome (default true for the workbench). */
	showChrome?: boolean;
	/** When provided, renders a back control in the header (workbench launcher). */
	onExit?: () => void;
}

function isOqtoScheme(scheme: Base24Scheme): boolean {
	return scheme.id.startsWith("oqto-");
}

/**
 * Standalone shell that mounts a single mini-app with the oqto design system
 * applied: it owns the active base24 scheme + light/dark mode, injects a host,
 * and renders the minimal provider tree. The light/dark class and all semantic
 * CSS vars are driven by the base24 engine (no next-themes here).
 */
export function OqtoAppShell({
	app,
	host,
	initialSchemeId = "oqto-dark",
	showChrome = true,
	onExit,
}: OqtoAppShellProps) {
	const initialScheme =
		builtInSchemes[initialSchemeId] ?? defaultSchemeForMode("dark");

	const [schemeId, setSchemeId] = useState(initialScheme.id);
	const [mode, setMode] = useState<ThemeMode>(initialScheme.mode);

	const schemeRef = useRef<Base24Scheme>(initialScheme);
	const modeRef = useRef<ThemeMode>(initialScheme.mode);

	const applySelection = useCallback(
		(nextScheme: Base24Scheme, nextMode: ThemeMode) => {
			schemeRef.current = nextScheme;
			modeRef.current = nextMode;
			setSchemeId(nextScheme.id);
			setMode(nextMode);
			applyScheme(nextScheme, nextMode);
		},
		[],
	);

	const handleSetMode = useCallback(
		(nextMode: ThemeMode) => {
			const current = schemeRef.current;
			const nextScheme = isOqtoScheme(current)
				? defaultSchemeForMode(nextMode)
				: current;
			applySelection(nextScheme, nextMode);
		},
		[applySelection],
	);

	const handleToggleMode = useCallback(() => {
		handleSetMode(modeRef.current === "dark" ? "light" : "dark");
	}, [handleSetMode]);

	const handleSelectScheme = useCallback(
		(id: string) => {
			const next = builtInSchemes[id];
			if (next) applySelection(next, next.mode);
		},
		[applySelection],
	);

	const mockHost = useMemo<OqtoHost>(
		() =>
			createMockHost({
				kvNamespace: app.id,
				getMode: () => modeRef.current,
				getScheme: () => schemeRef.current,
				setMode: handleSetMode,
			}),
		[app.id, handleSetMode],
	);

	const activeHost = host ?? mockHost;

	useMountEffect(() => {
		applyIdentityTokens(document.documentElement);
		applyScheme(schemeRef.current, modeRef.current);
	});

	const AppComponent = app.component;

	return (
		<WorkbenchProviders>
			<div className="flex h-dvh flex-col bg-background text-foreground">
				{showChrome ? (
					<header className="flex items-center justify-between gap-2 border-b border-border px-3 py-2 sm:px-4">
						<div className="flex min-w-0 items-center gap-2">
							{onExit ? (
								<Button
									variant="ghost"
									size="icon"
									onClick={onExit}
									aria-label="Back to apps"
									title="Back to apps"
								>
									<ChevronLeftIcon className="size-4" />
								</Button>
							) : null}
							{app.icon ? <app.icon className="size-4 shrink-0" /> : null}
							<span className="truncate text-sm font-bold">{app.title}</span>
						</div>
						<SchemePicker
							schemes={builtInSchemeList}
							schemeId={schemeId}
							mode={mode}
							onSelectScheme={handleSelectScheme}
							onToggleMode={handleToggleMode}
						/>
					</header>
				) : null}
				<main className="min-h-0 flex-1 overflow-hidden">
					<OqtoHostProvider host={activeHost}>
						<AppComponent />
					</OqtoHostProvider>
				</main>
			</div>
		</WorkbenchProviders>
	);
}
