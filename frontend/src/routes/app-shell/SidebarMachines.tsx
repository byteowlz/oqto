import { useCurrentUser } from "@/hooks/use-auth";
import { controlPlaneApiUrl, getAuthHeaders } from "@/lib/control-plane-client";
import { parseRunnerTargets } from "../../oqto-ui/platform/runner-targets";
import { RunnerTargets } from "../../oqto-ui/sessions/RunnerTargets";

/** Original-shell adapter for the same Account-authorized machine inventory. */
export function SidebarMachines() {
	const { data: user } = useCurrentUser();
	if (!user) return null;
	return (
		<RunnerTargets
			source={{
				id: `original:${controlPlaneApiUrl("/api/runner-targets")}:${user.id}`,
				list: async () => {
					const response = await fetch(
						controlPlaneApiUrl("/api/runner-targets"),
						{
							credentials: "include",
							headers: { ...getAuthHeaders(), Accept: "application/json" },
						},
					);
					if (!response.ok) throw new Error(`HTTP ${response.status}`);
					return parseRunnerTargets(await response.json());
				},
			}}
		/>
	);
}
