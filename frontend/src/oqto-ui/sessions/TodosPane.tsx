/**
 * The Session's todo list as placeable Content. Same tasks the progress
 * rail summarises, given room to be read: one row per task with its state,
 * and a count of what is left.
 */

import { Check, CircleDashed, CircleDot } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { SessionTask } from "../platform/contracts";

const ICONS = {
	completed: Check,
	active: CircleDot,
	pending: CircleDashed,
};

interface TodosPaneProps {
	readonly tasks: readonly SessionTask[];
}

export function TodosPane({ tasks }: TodosPaneProps) {
	const { t } = useTranslation();
	const open = tasks.filter((task) => task.status !== "completed").length;
	return (
		<section className="wb-todos" aria-label={t("oqtoUi.todos.label")}>
			{tasks.length === 0 ? (
				<p className="wb-todos__empty">{t("oqtoUi.todos.empty")}</p>
			) : (
				<>
					<p className="wb-todos__summary">
						{t("oqtoUi.todos.summary", { open, total: tasks.length })}
					</p>
					<ul className="wb-todos__list">
						{tasks.map((task) => {
							const Icon = ICONS[task.status];
							return (
								<li key={task.id} data-status={task.status}>
									<Icon aria-hidden="true" />
									<span>{t(task.titleKey)}</span>
									<small>
										{t(`oqtoUi.taskProgress.status.${task.status}`)}
									</small>
								</li>
							);
						})}
					</ul>
				</>
			)}
		</section>
	);
}
