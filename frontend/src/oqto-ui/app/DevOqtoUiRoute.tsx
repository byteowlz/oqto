import { useSearchParams } from "react-router-dom";
import { scriptedOqtoUiPlatform } from "../dev/scripted-platform";
import { OqtoUiShell } from "./OqtoUiShell";
import { applyNavigation } from "./routing";

export default function DevOqtoUiRoute() {
	const [searchParams, setSearchParams] = useSearchParams();
	return (
		<OqtoUiShell
			platform={scriptedOqtoUiPlatform}
			platformId="scripted"
			workDirectoryId={searchParams.get("workDirectory")}
			sessionId={searchParams.get("session")}
			mobileView={searchParams.get("view") === "files" ? "files" : "chat"}
			schemeId={searchParams.get("scheme") ?? "oqto-dark"}
			workAreaTab={searchParams.get("tab") ?? "chat"}
			onNavigate={(next) => {
				setSearchParams((current) => applyNavigation(current, next));
			}}
		/>
	);
}
