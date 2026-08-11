import { useSearchParams } from "react-router-dom";
import { WorkbenchLab } from "../surfaces/shell/WorkbenchLab";

const DEFAULT_WORK_DIRECTORY = "oqto";
const DEFAULT_SESSION = "frontend-rebuild";

export default function WorkbenchLabRoute() {
	const [searchParams, setSearchParams] = useSearchParams();
	const workDirectoryId =
		searchParams.get("workDirectory") ?? DEFAULT_WORK_DIRECTORY;
	const sessionId = searchParams.get("session") ?? DEFAULT_SESSION;
	const mobileView = searchParams.get("view") === "files" ? "files" : "chat";
	const schemeId = searchParams.get("scheme") ?? "oqto-dark";
	const workAreaTab = searchParams.get("tab") ?? "chat";

	return (
		<WorkbenchLab
			workDirectoryId={workDirectoryId}
			sessionId={sessionId}
			mobileView={mobileView}
			schemeId={schemeId}
			workAreaTab={workAreaTab}
			onNavigate={(next) => {
				setSearchParams((current) => {
					const updated = new URLSearchParams(current);
					if (next.workDirectoryId) {
						updated.set("workDirectory", next.workDirectoryId);
					}
					if (next.sessionId) updated.set("session", next.sessionId);
					if (next.mobileView) updated.set("view", next.mobileView);
					if (next.schemeId) updated.set("scheme", next.schemeId);
					if (next.workAreaTab) updated.set("tab", next.workAreaTab);
					return updated;
				});
			}}
		/>
	);
}
