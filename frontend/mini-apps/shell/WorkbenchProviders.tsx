import { Toaster } from "@/components/ui/sonner";
import { TooltipProvider } from "@/components/ui/tooltip";
import { getQueryClient } from "@/lib/query-client";
import { QueryClientProvider } from "@tanstack/react-query";
import type { ReactNode } from "react";

/**
 * The minimal provider tree a standalone mini-app needs -- deliberately NOT the
 * full oqto Providers (no auth, no i18n bootstrap, no router). Theme/mode is
 * owned by OqtoAppShell via the base24 engine, not next-themes.
 */
export function WorkbenchProviders({ children }: { children: ReactNode }) {
	const queryClient = getQueryClient();
	return (
		<QueryClientProvider client={queryClient}>
			<TooltipProvider delayDuration={0}>
				{children}
				<Toaster position="bottom-right" />
			</TooltipProvider>
		</QueryClientProvider>
	);
}
