import { ChevronDown, ChevronUp } from "lucide-react";
import { Fragment, useId, useState } from "react";
import { useTranslation } from "react-i18next";
import type { SessionTask } from "../platform/contracts";

type TaskProgressProps = {
	tasks: SessionTask[];
	placement: "desktop" | "mobile";
};

export function TaskProgress({ tasks, placement }: TaskProgressProps) {
	const { t } = useTranslation();
	const [expanded, setExpanded] = useState(false);
	const detailsId = useId();
	if (tasks.length === 0) return null;

	const completed = tasks.filter((task) => task.status === "completed").length;
	const active = tasks.find((task) => task.status === "active");
	const summary = t("oqtoUi.taskProgress.summary", {
		completed,
		total: tasks.length,
	});

	return (
		<section
			className="wb-task-progress"
			data-expanded={expanded}
			data-placement={placement}
			aria-label={summary}
		>
			<div className="wb-task-progress__rail">
				<span className="wb-task-progress__label">
					{t("oqtoUi.taskProgress.label")}
				</span>
				{active ? (
					<span className="wb-task-progress__active">{t(active.titleKey)}</span>
				) : null}
				<div className="wb-task-progress__points" aria-label={summary}>
					{tasks.map((task, index) => (
						<Fragment key={task.id}>
							{index > 0 ? (
								<span
									className="wb-task-progress__connector"
									aria-hidden="true"
								/>
							) : null}
							<button
								className="wb-task-progress__point"
								data-status={task.status}
								type="button"
								aria-label={`${t(task.titleKey)}: ${t(`oqtoUi.taskProgress.status.${task.status}`)}`}
								onClick={() => setExpanded(true)}
							>
								<span className="wb-task-progress__dot" aria-hidden="true" />
								<span className="wb-task-progress__tooltip" role="tooltip">
									<strong>{t(task.titleKey)}</strong>
									<small>
										{t(`oqtoUi.taskProgress.status.${task.status}`)}
									</small>
								</span>
							</button>
						</Fragment>
					))}
				</div>
				<span className="wb-task-progress__count">
					{completed}/{tasks.length}
				</span>
				<button
					className="wb-task-progress__toggle"
					type="button"
					aria-controls={detailsId}
					aria-expanded={expanded}
					aria-label={t(
						expanded
							? "oqtoUi.taskProgress.closePlan"
							: "oqtoUi.taskProgress.openPlan",
					)}
					onClick={() => setExpanded((current) => !current)}
				>
					{expanded ? (
						<ChevronDown aria-hidden="true" />
					) : (
						<ChevronUp aria-hidden="true" />
					)}
				</button>
			</div>

			{expanded ? (
				<section
					className="wb-task-progress__details"
					id={detailsId}
					aria-label={t("oqtoUi.taskProgress.label")}
				>
					<header>
						<strong>{t("oqtoUi.taskProgress.label")}</strong>
						<span>{summary}</span>
						<button
							type="button"
							aria-label={t("oqtoUi.taskProgress.closePlan")}
							onClick={() => setExpanded(false)}
						>
							<ChevronDown aria-hidden="true" />
						</button>
					</header>
					<ol>
						{tasks.map((task) => (
							<li data-status={task.status} key={task.id}>
								<span aria-hidden="true" />
								<strong>{t(task.titleKey)}</strong>
								<small>{t(`oqtoUi.taskProgress.status.${task.status}`)}</small>
							</li>
						))}
					</ol>
				</section>
			) : null}
		</section>
	);
}
