"use client";

import { UIControlProvider } from "@/components/contexts/ui-control-context";
import { SessionScreen } from "@/features/sessions/SessionScreen";

export function SessionsApp() {
	return (
		<UIControlProvider>
			<SessionScreen />
		</UIControlProvider>
	);
}

export default SessionsApp;
