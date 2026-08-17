import { liveOqtoUiPlatform } from "../platform/live-platform";
import { OqtoUiShell } from "./OqtoUiShell";

export default function OqtoUiRoute() {
	return <OqtoUiShell platform={liveOqtoUiPlatform} mode="live" />;
}
