import { useQuery } from "@tanstack/react-query";
import { Monitor } from "lucide-react";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import type {
	RunnerTarget,
	RunnerTargetsPort,
} from "../platform/runner-targets";
import "./runner-targets.css";

type RunnerTargetsProps = {
	source: RunnerTargetsPort;
	actions?: (target: RunnerTarget) => ReactNode;
	belowTarget?: (target: RunnerTarget) => ReactNode;
	presentation?: "section" | "rows";
};

export function RunnerTargets({
	source,
	actions,
	belowTarget,
	presentation = "section",
}: RunnerTargetsProps) {
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
	// An unverifiable roster is worse than none: showing the previous status
	// would claim a machine is Online when that can no longer be confirmed, and
	// a standalone error block just occupies the sidebar with nothing to act on.
	const targets = query.isError ? [] : (query.data ?? []);
	if (!targets.length) return null;
	const contents = (
		<>
			{
				<ul aria-label={t("oqtoUi.runnerTargets.title")}>
					{targets.map((target) => (
						<li key={target.id} data-connection={target.connection}>
							<Monitor aria-hidden="true" />
							{actions?.(target)}
							<div className="wb-runner-targets__name">
								<strong>{target.label}</strong>
								<span>
									{t(
										target.historyRead
											? "oqtoUi.runnerTargets.historyOnly"
											: "oqtoUi.runnerTargets.connectionOnly",
									)}
								</span>
							</div>
							<span
								className="wb-runner-targets__status"
								title={t("oqtoUi.runnerTargets.checkedAt", {
									time: new Date(target.checkedAt).toLocaleTimeString(),
								})}
							>
								{t(`oqtoUi.runnerTargets.${target.connection}`)}
							</span>
							{belowTarget ? (
								<div className="wb-runner-targets__below">
									{belowTarget(target)}
								</div>
							) : null}
						</li>
					))}
				</ul>
			}
		</>
	);
	if (presentation === "rows") {
		return (
			<div className="wb-runner-targets wb-runner-targets--rows">
				{contents}
			</div>
		);
	}
	return (
		<details className="wb-runner-targets" open>
			<summary>{t("oqtoUi.runnerTargets.title")}</summary>
			{contents}
		</details>
	);
}
