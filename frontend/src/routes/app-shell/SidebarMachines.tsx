import { useCurrentUser } from "@/hooks/use-auth";
import { controlPlaneApiUrl, getAuthHeaders } from "@/lib/control-plane-client";
import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import {
	type RunnerTarget,
	parseRunnerTargets,
} from "../../oqto-ui/platform/runner-targets";
import { MachineChats } from "./MachineChats";
import type { HistoryCommand } from "./MachineHistory";
import { MachineProviders } from "./MachineProviders";

function historyPort(scope: string, targetId: string) {
	return {
		call: async (command: HistoryCommand, signal: AbortSignal) => {
			const response = await fetch(
				controlPlaneApiUrl(
					`/api/runner-targets/${encodeURIComponent(targetId)}/history`,
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
			if (!response.ok) throw new Error("Machine history unavailable");
			return response.json();
		},
		scope,
	};
}

/** Original-shell adapter for Account-authorized inventory and explicit login grants. */
export function SidebarMachines({
	onNewSessionInDirectory,
}: { onNewSessionInDirectory?: (directory: string) => void } = {}) {
	const { data: user } = useCurrentUser();
	if (!user) return null;
	const scope = `original:${controlPlaneApiUrl("/api/runner-targets")}:${user.id}`;
	return (
		<AccountMachines
			key={scope}
			scope={scope}
			onNewSessionInDirectory={onNewSessionInDirectory}
		/>
	);
}
function AccountMachines({
	scope,
	onNewSessionInDirectory,
}: {
	scope: string;
	onNewSessionInDirectory?: (directory: string) => void;
}) {
	const { t } = useTranslation();
	const [selected, setSelected] = useState<RunnerTarget | null>(null);
	const isMobile =
		typeof window !== "undefined" &&
		window.matchMedia("(max-width: 767px)").matches;
	const roster = useQuery({
		queryKey: ["oqto-runner-targets", scope],
		queryFn: async () => {
			const response = await fetch(controlPlaneApiUrl("/api/runner-targets"), {
				credentials: "include",
				headers: { ...getAuthHeaders(), Accept: "application/json" },
			});
			if (!response.ok) throw new Error(`HTTP ${response.status}`);
			return parseRunnerTargets(await response.json());
		},
		refetchInterval: 10_000,
		staleTime: 0,
		// Never retain another Account's roster after logout/unmount.
		gcTime: 0,
		retry: false,
	});
	// An unverifiable roster is withheld rather than shown as a stale status.
	const targets = roster.isError ? [] : (roster.data ?? []);
	return (
		<>
			{targets
				.filter((target) => target.historyRead)
				.map((target) => (
					<MachineChats
						key={`${scope}:${target.id}`}
						scope={`${scope}:${target.id}`}
						label={target.label}
						online={target.connection === "online"}
						isMobile={isMobile}
						onNewSession={
							target.sessionCreation ? onNewSessionInDirectory : undefined
						}
						onOpenProviders={
							target.providerLogin && target.connection === "online"
								? () => setSelected(target)
								: undefined
						}
						providersLabel={t("oqtoUi.providerLogin.buttonLabel", {
							machine: target.label,
						})}
						port={historyPort(`${scope}:${target.id}`, target.id)}
					/>
				))}
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
