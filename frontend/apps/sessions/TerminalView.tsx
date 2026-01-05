"use client";

import { GhosttyTerminal } from "@/components/terminal/ghostty-terminal";
import {
	controlPlaneDirectBaseUrl,
	terminalWorkspaceProxyPath,
} from "@/lib/control-plane-client";
import { toAbsoluteWsUrl } from "@/lib/url";
import { useTheme } from "next-themes";
import { useMemo } from "react";

interface TerminalViewProps {
	workspacePath?: string | null;
}

export function TerminalView({ workspacePath }: TerminalViewProps) {
	const { resolvedTheme } = useTheme();

	const wsUrl = useMemo(() => {
		if (!workspacePath) return "";
		const directBase = controlPlaneDirectBaseUrl();
		const proxyPath = terminalWorkspaceProxyPath(workspacePath);
		if (directBase) {
			return toAbsoluteWsUrl(`${directBase}${proxyPath}`);
		}
		return toAbsoluteWsUrl(`/api${proxyPath}`);
	}, [workspacePath]);

	// Don't render terminal if no session selected
	if (!workspacePath) {
		return (
			<div className="h-full bg-black/70 rounded p-4 text-sm font-mono text-red-300">
				Select a chat to attach to the terminal.
			</div>
		);
	}

	// Pass theme to terminal so it can include it in its session key
	return (
		<div className="h-full">
			<GhosttyTerminal
				key={`${workspacePath}-${resolvedTheme}`}
				wsUrl={wsUrl}
				className="border border-border"
				theme={resolvedTheme}
			/>
		</div>
	);
}
