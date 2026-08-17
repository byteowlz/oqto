import { scriptedOqtoUiPlatform } from "../dev/scripted-platform";
import { OqtoUiShell } from "./OqtoUiShell";

export default function DevOqtoUiRoute() {
	return <OqtoUiShell platform={scriptedOqtoUiPlatform} mode="scripted" />;
}
