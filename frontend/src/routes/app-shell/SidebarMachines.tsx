import { useCurrentUser } from "@/hooks/use-auth";
import { controlPlaneApiUrl, getAuthHeaders } from "@/lib/control-plane-client";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import {
	type RunnerTarget,
	parseRunnerTargets,
} from "../../oqto-ui/platform/runner-targets";
import { RunnerTargets } from "../../oqto-ui/sessions/RunnerTargets";
import { MachineProviders } from "./MachineProviders";

/** Original-shell adapter for Account-authorized inventory and explicit login grants. */
export function SidebarMachines() {
	const { data: user } = useCurrentUser();
	if (!user) return null;
	const scope = `original:${controlPlaneApiUrl("/api/runner-targets")}:${user.id}`;
	return <AccountMachines key={scope} scope={scope} />;
}
function AccountMachines({ scope }: { scope: string }) {
	const { t } = useTranslation();
	const [selected, setSelected] = useState<RunnerTarget | null>(null);
	return (
		<>
			<RunnerTargets
				presentation="rows"
				source={{
					id: scope,
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
				actions={(target) =>
					target.providerLogin ? (
						<button
							type="button"
							className="rounded border px-2 py-1 text-xs disabled:opacity-40"
							disabled={target.connection !== "online"}
							onClick={() => setSelected(target)}
							aria-label={t("oqtoUi.providerLogin.buttonLabel", {
								machine: target.label,
							})}
						>
							{t("oqtoUi.providerLogin.button")}
						</button>
					) : null
				}
			/>
			{selected && (
				<MachineProviders
					key={`${scope}:${selected.id}`}
					label={selected.label}
					close={() => setSelected(null)}
					port={{
						call: async (command, signal) => {
							const response = await fetch(
								controlPlaneApiUrl(
									`/api/runner-targets/${encodeURIComponent(selected.id)}/provider-login`,
								),
								{
									method: "POST",
									credentials: "include",
									cache: "no-store",
									signal,
									headers: {
										...getAuthHeaders(),
										"Content-Type": "application/json",
										Accept: "application/json",
									},
									body: JSON.stringify(command),
								},
							);
							if (!response.ok) throw new Error("Provider login unavailable");
							return response.json();
						},
					}}
				/>
			)}
		</>
	);
}
