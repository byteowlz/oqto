import { useMountEffect } from "@/hooks/use-mount-effect";
import { useId, useMemo, useState, useSyncExternalStore } from "react";
import { useTranslation } from "react-i18next";
import type { Answer, Question } from "../platform/selection-contract";
import type { SelectionController } from "../platform/selection-host";
import type { SelectionHost } from "../platform/selection-service";
import "./selection.css";

type SelectionViewProps = { host: SelectionHost; sessionId: string };
type RenderedSelectionProps = {
	controller: SelectionController;
	reviewId: string;
};
function RenderedSelection({ controller, reviewId }: RenderedSelectionProps) {
	useMountEffect(() => {
		controller.rendered(reviewId);
	});
	return null;
}
export function SelectionView({ host, sessionId }: SelectionViewProps) {
	const viewId = useId();
	const { t } = useTranslation();
	const controller = useMemo(() => host.session(sessionId), [host, sessionId]);
	const state = useSyncExternalStore(
		controller.subscribe,
		controller.snapshot,
		controller.snapshot,
	);
	const [search, setSearch] = useState("");
	const [group, setGroup] = useState("");
	const [page, setPage] = useState(0);
	const [confirmClose, setConfirmClose] = useState(false);
	const [confirmReload, setConfirmReload] = useState(false);
	const record = state.record;
	if (!record)
		return state.error ? (
			<section
				className="oqto-selection"
				aria-label={t("oqtoUi.selection.unavailable")}
			>
				<p role="alert">
					{t("oqtoUi.selection.error", { detail: state.error })}
				</p>
				<p>{t("oqtoUi.selection.schemaHelp")}</p>
				<button type="button" onClick={() => void controller.reload()}>
					{t("oqtoUi.selection.reload")}
				</button>
			</section>
		) : null;
	const blocked =
		state.busy || state.phase !== "ready" || record.state !== "draft";
	const update = (question: Question, patch: Partial<Answer>) => {
		const answer = state.answers[question.id] ?? {
			answered: false,
			selectedIds: [],
		};
		controller.edit({
			...state.answers,
			[question.id]: { ...answer, ...patch },
		});
	};
	const submit = async () => {
		await controller.save(true);
		const invalidId = Object.keys(controller.snapshot().fieldErrors)[0];
		if (invalidId) {
			setSearch("");
			setGroup("");
			setPage(
				Math.floor(
					record.spec.questions.findIndex(
						(question) => question.id === invalidId,
					) / 25,
				),
			);
		}
	};
	const matching = record.spec.questions.filter(
		(question) =>
			(!group || question.groupId === group) &&
			`${question.title} ${question.description ?? ""}`
				.toLocaleLowerCase()
				.includes(search.toLocaleLowerCase()),
	);
	const currentPage = Math.min(
		page,
		Math.max(0, Math.ceil(matching.length / 25) - 1),
	);
	const visible = matching.slice(currentPage * 25, (currentPage + 1) * 25);
	return (
		<section
			className="oqto-selection"
			aria-label={record.spec.title}
			onKeyDown={(event) => {
				if ((event.ctrlKey || event.metaKey) && event.key === "Enter") {
					event.preventDefault();
					void submit();
				}
			}}
		>
			{state.phase === "ready" && record.state === "draft" ? (
				<RenderedSelection
					key={`${record.id}:${state.presentationEpoch}`}
					controller={controller}
					reviewId={record.id}
				/>
			) : null}
			<header>
				<h2>{record.spec.title}</h2>
				<p>{record.spec.description}</p>
			</header>
			<output aria-live="polite">
				{t(`oqtoUi.selection.${record.state}`)} ·{" "}
				{t(`oqtoUi.selection.${state.phase}`)}
				{state.dirty ? ` · ${t("oqtoUi.selection.unsaved")}` : ""}
				{state.busy ? ` · ${t("oqtoUi.selection.working")}` : ""}
			</output>
			{record.spec.mode === "review" ? (
				<div className="oqto-selection-filters">
					<label>
						{t("oqtoUi.selection.search")}
						<input
							type="search"
							value={search}
							onChange={(event) => {
								setSearch(event.target.value);
								setPage(0);
							}}
						/>
					</label>
					<label>
						{t("oqtoUi.selection.group")}
						<select
							value={group}
							onChange={(event) => {
								setGroup(event.target.value);
								setPage(0);
							}}
						>
							<option value="">{t("oqtoUi.selection.all")}</option>
							{record.spec.groups.map((item) => (
								<option key={item.id} value={item.id}>
									{item.title}
								</option>
							))}
						</select>
					</label>
				</div>
			) : null}
			{matching.length > 25 ? (
				<nav
					className="oqto-selection-actions"
					aria-label={t("oqtoUi.selection.pages")}
				>
					<button
						type="button"
						disabled={currentPage === 0}
						onClick={() => setPage(currentPage - 1)}
					>
						{t("oqtoUi.selection.previous")}
					</button>
					<output>
						{t("oqtoUi.selection.range", {
							start: currentPage * 25 + 1,
							end: Math.min((currentPage + 1) * 25, matching.length),
							total: matching.length,
						})}
					</output>
					<button
						type="button"
						disabled={(currentPage + 1) * 25 >= matching.length}
						onClick={() => setPage(currentPage + 1)}
					>
						{t("oqtoUi.selection.next")}
					</button>
				</nav>
			) : null}
			<div className="oqto-selection-items">
				{visible.length === 0 ? <p>{t("oqtoUi.selection.noResults")}</p> : null}
				{visible.map((question) => {
					const answer = state.answers[question.id] ?? {
						answered: false,
						selectedIds: [],
					};
					const groupTitle = record.spec.groups.find(
						(item) => item.id === question.groupId,
					)?.title;
					return (
						<fieldset
							key={question.id}
							disabled={blocked}
							tabIndex={-1}
							aria-describedby={
								state.fieldErrors[question.id]
									? `${viewId}-${question.id}-error`
									: undefined
							}
							ref={(node) => {
								if (node && Object.keys(state.fieldErrors)[0] === question.id)
									node.focus();
							}}
						>
							<legend>
								{groupTitle ? `${groupTitle} · ` : ""}
								{question.header ? `${question.header} · ` : ""}
								{question.title}
								{question.required
									? ` (${t("oqtoUi.selection.required")})`
									: ""}
							</legend>
							{question.description ? <p>{question.description}</p> : null}
							{state.fieldErrors[question.id] ? (
								<p id={`${viewId}-${question.id}-error`} aria-live="polite">
									{t("oqtoUi.selection.error", {
										detail: state.fieldErrors[question.id],
									})}
								</p>
							) : null}
							{question.options?.map((option) => (
								<label key={option.id} className="oqto-selection-choice">
									<input
										type={question.kind === "multiple" ? "checkbox" : "radio"}
										name={`${viewId}-${record.id}-${question.id}`}
										checked={answer.selectedIds.includes(option.id)}
										onChange={(event) => {
											const selectedIds =
												question.kind === "multiple"
													? event.target.checked
														? [...answer.selectedIds, option.id]
														: answer.selectedIds.filter(
																(id) => id !== option.id,
															)
													: [option.id];
											update(question, {
												answered:
													selectedIds.length > 0 ||
													(question.kind === "multiple" &&
														Boolean(answer.text?.trim())),
												disposition: undefined,
												text:
													question.kind === "multiple"
														? answer.text
														: undefined,
												selectedIds,
											});
										}}
									/>
									<span>
										{option.label}
										{option.recommended
											? ` · ${t("oqtoUi.selection.recommended")}`
											: ""}
										{option.description ? (
											<small>{option.description}</small>
										) : null}
									</span>
								</label>
							))}
							{question.kind === "text" || question.allowOther ? (
								<div>
									<label htmlFor={`${viewId}-${question.id}-text`}>
										{t(
											question.kind === "text"
												? "oqtoUi.selection.text"
												: "oqtoUi.selection.other",
										)}
									</label>
									{question.multiline ? (
										<textarea
											id={`${viewId}-${question.id}-text`}
											value={answer.text ?? ""}
											maxLength={question.maxLength}
											onChange={(event) =>
												update(question, {
													text: event.target.value,
													selectedIds:
														question.kind === "multiple"
															? answer.selectedIds
															: [],
													answered: true,
													disposition: undefined,
												})
											}
										/>
									) : (
										<input
											type="text"
											id={`${viewId}-${question.id}-text`}
											value={answer.text ?? ""}
											maxLength={question.maxLength}
											onChange={(event) =>
												update(question, {
													text: event.target.value,
													selectedIds:
														question.kind === "multiple"
															? answer.selectedIds
															: [],
													answered: true,
													disposition: undefined,
												})
											}
										/>
									)}
								</div>
							) : null}
							{question.note ? (
								<div>
									<label htmlFor={`${viewId}-${question.id}-note`}>
										{question.note.label ?? t("oqtoUi.selection.notes")}
									</label>
									{question.note.multiline ? (
										<textarea
											id={`${viewId}-${question.id}-note`}
											value={answer.note ?? ""}
											maxLength={question.note.maxLength}
											onChange={(event) =>
												update(question, { note: event.target.value })
											}
										/>
									) : (
										<input
											type="text"
											id={`${viewId}-${question.id}-note`}
											value={answer.note ?? ""}
											maxLength={question.note.maxLength}
											onChange={(event) =>
												update(question, { note: event.target.value })
											}
										/>
									)}
								</div>
							) : null}
							<div className="oqto-selection-actions">
								{question.kind === "multiple" ? (
									<button
										type="button"
										onClick={() =>
											update(question, {
												answered: true,
												disposition: undefined,
												selectedIds: [],
												text: undefined,
											})
										}
									>
										{t("oqtoUi.selection.confirmNone")}
									</button>
								) : null}
								{(["skipped", "unsure"] as const).map((disposition) => (
									<button
										type="button"
										key={disposition}
										aria-pressed={answer.disposition === disposition}
										onClick={() =>
											update(question, {
												answered: false,
												selectedIds: [],
												text: undefined,
												disposition,
											})
										}
									>
										{t(`oqtoUi.selection.${disposition}`)}
									</button>
								))}
								<button
									type="button"
									onClick={() =>
										update(question, {
											answered: false,
											disposition: undefined,
											selectedIds: [],
											text: undefined,
										})
									}
								>
									{t("oqtoUi.selection.reset")}
								</button>
							</div>
							<p>
								{answer.disposition
									? t(`oqtoUi.selection.${answer.disposition}`)
									: t(
											answer.answered
												? "oqtoUi.selection.answered"
												: "oqtoUi.selection.unanswered",
										)}
							</p>
						</fieldset>
					);
				})}
			</div>
			{state.error ? (
				<p role="alert">
					{t("oqtoUi.selection.error", { detail: state.error })}
				</p>
			) : null}
			<footer className="oqto-selection-actions">
				<button
					type="button"
					disabled={blocked}
					onClick={() => void controller.save()}
				>
					{t("oqtoUi.selection.save")}
				</button>
				<button type="button" disabled={blocked} onClick={() => void submit()}>
					{t("oqtoUi.selection.submit")}
				</button>
				<button
					type="button"
					disabled={state.busy || state.phase !== "ready"}
					onClick={() =>
						state.dirty ? setConfirmReload(true) : void controller.reload()
					}
				>
					{t("oqtoUi.selection.reload")}
				</button>
				<button
					type="button"
					disabled={blocked}
					onClick={() => setConfirmClose(true)}
				>
					{t("oqtoUi.selection.close")}
				</button>
			</footer>
			{confirmClose || confirmReload ? (
				<div role="alert" className="oqto-selection-confirm">
					<p>
						{t(
							confirmClose
								? "oqtoUi.selection.confirmClose"
								: "oqtoUi.selection.confirmReload",
						)}
					</p>
					<button
						type="button"
						onClick={() => {
							if (confirmClose) void controller.close();
							else void controller.reload();
							setConfirmClose(false);
							setConfirmReload(false);
						}}
					>
						{t("oqtoUi.selection.confirm")}
					</button>
					<button
						type="button"
						onClick={() => {
							setConfirmClose(false);
							setConfirmReload(false);
						}}
					>
						{t("oqtoUi.selection.keep")}
					</button>
				</div>
			) : null}
		</section>
	);
}
