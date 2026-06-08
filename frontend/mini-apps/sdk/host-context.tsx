import { createContext, useContext } from "react";
import type { ReactNode } from "react";
import type { OqtoHost } from "./host";

const OqtoHostContext = createContext<OqtoHost | null>(null);

export interface OqtoHostProviderProps {
	host: OqtoHost;
	children: ReactNode;
}

export function OqtoHostProvider({ host, children }: OqtoHostProviderProps) {
	return (
		<OqtoHostContext.Provider value={host}>{children}</OqtoHostContext.Provider>
	);
}

/** Access the host from inside a mounted mini-app. Throws if unprovided. */
export function useOqtoHost(): OqtoHost {
	const host = useContext(OqtoHostContext);
	if (!host) {
		throw new Error(
			"useOqtoHost must be used within an OqtoHostProvider (mount the app via OqtoAppShell).",
		);
	}
	return host;
}
