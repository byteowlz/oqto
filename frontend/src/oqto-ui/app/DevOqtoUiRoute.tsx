import { useSearchParams } from "react-router-dom";
import { scriptedOqtoUiPlatform } from "../dev/scripted-platform";
import { browserLayoutStorage } from "../platform/layout-storage";
import { CompositorShell } from "./CompositorShell";
import { OqtoUiShell } from "./OqtoUiShell";
import { applyNavigation } from "./routing";

/**
 * Test-only adapter wrapper. This is not registered as an application
 * route. `?compositor=1` composes the same scripted platform through the
 * ADR-0041 compositor instead of the hardcoded shell layout.
 */
export default function DevOqtoUiRoute() {
	const [searchParams, setSearchParams] = useSearchParams();
	const onNavigate = (next: Parameters<typeof applyNavigation>[1]) => {
		setSearchParams((current) => applyNavigation(current, next));
	};
	if (searchParams.get("compositor") === "1") {
		return (
			<CompositorShell
				platform={scriptedOqtoUiPlatform}
				workDirectoryId={searchParams.get("workDirectory")}
				sessionId={searchParams.get("session")}
				schemeId={searchParams.get("scheme")}
				workAreaTab={searchParams.get("tab") ?? "chat"}
				storage={browserLayoutStorage}
				onNavigate={onNavigate}
			/>
		);
	}
	return (
		<OqtoUiShell
			platform={scriptedOqtoUiPlatform}
			workDirectoryId={searchParams.get("workDirectory")}
			sessionId={searchParams.get("session")}
			mobileView={searchParams.get("view") === "files" ? "files" : "chat"}
			schemeId={searchParams.get("scheme")}
			workAreaTab={searchParams.get("tab") ?? "chat"}
			onNavigate={onNavigate}
		/>
	);
}
