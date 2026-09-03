import { useSearchParams } from "react-router-dom";
import { browserLayoutStorage } from "../platform/layout-storage";
import { liveOqtoUiPlatform } from "../platform/live-platform";
import { CompositorShell } from "./CompositorShell";
import { OqtoUiShell } from "./OqtoUiShell";
import { applyNavigation } from "./routing";

/**
 * The single shipped OqtoUI entry, backed by the live platform adapter.
 * `?compositor=1` composes the same live data through the ADR-0041
 * compositor (classic preset, device-local persisted layout) while the
 * hardcoded shell remains the default during the migration.
 */
export default function OqtoUiRoute() {
	const [searchParams, setSearchParams] = useSearchParams();
	const onNavigate = (next: Parameters<typeof applyNavigation>[1]) => {
		setSearchParams((current) => applyNavigation(current, next));
	};
	if (searchParams.get("compositor") === "1") {
		return (
			<CompositorShell
				platform={liveOqtoUiPlatform}
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
			platform={liveOqtoUiPlatform}
			workDirectoryId={searchParams.get("workDirectory")}
			sessionId={searchParams.get("session")}
			mobileView={searchParams.get("view") === "files" ? "files" : "chat"}
			schemeId={searchParams.get("scheme")}
			workAreaTab={searchParams.get("tab") ?? "chat"}
			onNavigate={onNavigate}
		/>
	);
}
