import { useQuery } from "@tanstack/react-query";
import { Monitor } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { RunnerTargetsPort } from "../platform/runner-targets";
import "./runner-targets.css";

type RunnerTargetsProps = { source: RunnerTargetsPort };

export function RunnerTargets({ source }: RunnerTargetsProps) {
	const { t } = useTranslation();
	const query = useQuery({
		queryKey: ["oqto-runner-targets", source.id],
		queryFn: () => source.list(),
		refetchInterval: 10_000,
		staleTime: 0,
		// Never retain another Account's roster after logout/unmount.
		gcTime: 0,
		retry: false,
	});
	if (query.isPending) return null;
	if (!query.isError && !query.data?.length) return null;
	return (
		<details className="wb-runner-targets" open>
			<summary>{t("oqtoUi.runnerTargets.title")}</summary>
			{query.isError ? (
				<output>{t("oqtoUi.runnerTargets.refreshFailed")}</output>
			) : (
				<ul aria-label={t("oqtoUi.runnerTargets.title")}>
					{query.data?.map((target) => (
						<li key={target.id} data-connection={target.connection}>
							<Monitor aria-hidden="true" />
							<div className="wb-runner-targets__name">
								<strong>{target.label}</strong>
								<span>{t("oqtoUi.runnerTargets.connectionOnly")}</span>
							</div>
							<span
								className="wb-runner-targets__status"
								title={t("oqtoUi.runnerTargets.checkedAt", {
									time: new Date(target.checkedAt).toLocaleTimeString(),
								})}
							>
								{t(`oqtoUi.runnerTargets.${target.connection}`)}
							</span>
						</li>
					))}
				</ul>
			)}
		</details>
	);
}
