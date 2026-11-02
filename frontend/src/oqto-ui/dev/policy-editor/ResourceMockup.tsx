import { useState } from "react";
import { useTranslation } from "react-i18next";
import { ResourceCanvas } from "./ResourceCanvas";
import { ResourceMark } from "./ResourceMark";
import { ReviewBar } from "./ReviewBar";
import { propose } from "./access";
import type { Direction } from "./layout";
import { type Draft, byId, workspaceOf } from "./resources";
import "./resource-mockup.css";

export function ResourceMockup() {
	const { t } = useTranslation();
	const [editing, setEditing] = useState(false);
	const [subject, setSubject] = useState("oqto");
	const [selected, setSelected] = useState("oqto");
	const [expanded, setExpanded] = useState<ReadonlySet<string>>(new Set());
	const [drafts, setDrafts] = useState<readonly Draft[]>([]);
	const [direction, setDirection] = useState<Direction>("horizontal");
	const [notice, setNotice] = useState<string | undefined>();
	const focused = byId(subject);
	const toggle = (id: string) =>
		setExpanded((old) => {
			const next = new Set(old);
			if (next.has(id)) next.delete(id);
			else next.add(id);
			return next;
		});
	const enterEdit = () => {
		setEditing(true);
		setNotice(undefined);
		setExpanded(new Set([subject]));
	};
	const leaveEdit = () => {
		setEditing(false);
		setExpanded(new Set());
	};
	const select = (id: string) => {
		const item = byId(id);
		if (!item) return;
		setSelected(id);
		setNotice(undefined);
		if (!editing) {
			if (item.kind === "workdir") setSubject(id);
			return;
		}
		if (item.kind === "workdir" && id !== subject) {
			setNotice(
				t("Subject is pinned. Return to Navigate to edit another workdir."),
			);
			return;
		}
		if (
			item.kind === "wiki" ||
			item.kind === "memory" ||
			item.kind === "service" ||
			id === subject
		)
			toggle(id);
	};
	if (!focused) return null;
	return (
		<main className="resource-mockup" data-editing={editing}>
			<header className="resource-bar">
				<span className="resource-brand">{t("OQTO · RESOURCE ACCESS")}</span>
				<fieldset className="resource-mode" aria-label={t("Interaction mode")}>
					<button type="button" aria-pressed={!editing} onClick={leaveEdit}>
						{t("Navigate")}
					</button>
					<button type="button" aria-pressed={editing} onClick={enterEdit}>
						{t("Edit access")}
					</button>
				</fieldset>
				<fieldset className="resource-mode" aria-label={t("Layout direction")}>
					<button
						type="button"
						aria-pressed={direction === "horizontal"}
						aria-label={t("Left to right")}
						onClick={() => setDirection("horizontal")}
					>
						{t("→")}
					</button>
					<button
						type="button"
						aria-pressed={direction === "vertical"}
						aria-label={t("Top down")}
						onClick={() => setDirection("vertical")}
					>
						{t("↓")}
					</button>
				</fieldset>
				<div className="resource-subject">
					<ResourceMark resource={focused} />
					<span>
						<small>
							{editing ? t("EDITING ACCESS FOR") : t("FOCUSED WORKDIR")}
						</small>
						<strong translate="no">
							{focused.name}
							<em>{byId(workspaceOf(subject) ?? "")?.name}</em>
						</strong>
					</span>
				</div>
				<p className="resource-hint">
					{notice ??
						(editing
							? t(
									"Click a wiki, memory or service to set access on the node. The subject stays pinned.",
								)
							: t(
									"Click any workdir to focus it. Chips show its access to knowledge and services.",
								))}
				</p>
				<span className="resource-synthetic">
					{t("Synthetic data · no authority")}
				</span>
			</header>
			<ResourceCanvas
				state={{ editing, subject, selected, expanded, drafts, direction }}
				actions={{
					onSelect: select,
					onEdit: enterEdit,
					onPropose: (resource, operation, mode) =>
						setDrafts((old) =>
							propose(old, { subject, resource, operation, mode }),
						),
				}}
			/>
			<ReviewBar
				subject={subject}
				selected={selected}
				drafts={drafts}
				onDiscard={() =>
					setDrafts((old) => old.filter((draft) => draft.subject !== subject))
				}
			/>
		</main>
	);
}
