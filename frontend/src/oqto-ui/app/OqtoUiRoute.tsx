import { useSearchParams } from "react-router-dom";
import { liveOqtoUiPlatform } from "../platform/live-platform";
import { OqtoUiShell } from "./OqtoUiShell";
import { applyNavigation } from "./routing";

/** The single shipped OqtoUI entry, backed by the live platform adapter. */
export default function OqtoUiRoute() {
	const [searchParams, setSearchParams] = useSearchParams();
	return (
		<OqtoUiShell
			platform={liveOqtoUiPlatform}
			workDirectoryId={searchParams.get("workDirectory")}
			sessionId={searchParams.get("session")}
			mobileView={searchParams.get("view") === "files" ? "files" : "chat"}
			schemeId={searchParams.get("scheme")}
			workAreaTab={searchParams.get("tab") ?? "chat"}
			onNavigate={(next) => {
				setSearchParams((current) => applyNavigation(current, next));
			}}
		/>
	);
}
