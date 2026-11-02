import { useState } from "react";
import { useTranslation } from "react-i18next";
import { type Draft, byId, operationsFor } from "./resources";

interface ReviewBarProps {
	subject: string;
	selected: string;
	drafts: readonly Draft[];
	onDiscard: () => void;
}
const MODE: Record<Draft["mode"], string> = {
	allow: "Allow",
	ask: "Ask",
	deny: "Deny",
};
export function ReviewBar({
	subject,
	selected,
	drafts,
	onDiscard,
}: ReviewBarProps) {
	const { t } = useTranslation();
	const [open, setOpen] = useState(false);
	const mine = drafts.filter((draft) => draft.subject === subject);
	const others = new Set(
		drafts
			.filter((draft) => draft.subject !== subject)
			.map((draft) => draft.subject),
	);
	const item = byId(selected);
	const name = byId(subject)?.name ?? subject;
	return (
		<footer className="resource-footer">
			{item && (
				<dl className="resource-detail">
					<dt>{t("SELECTED")}</dt>
					<dd translate="no">{item.name}</dd>
					<dt>{t("IDENTITY")}</dt>
					<dd>
						<code>{item.id}</code>
					</dd>
					<dt>{t("YOUR ACCESS")}</dt>
					<dd>{t(item.accessKey)}</dd>
					<dt>{t("BINDING")}</dt>
					<dd>{t(item.binding)}</dd>
				</dl>
			)}
			<div className="resource-draft">
				<strong>
					{mine.length
						? t("{{count}} proposed for {{name}}", { count: mine.length, name })
						: t("No proposed changes")}
				</strong>
				{others.size > 0 && (
					<small>
						{t("+ drafts for {{count}} other workdirs", { count: others.size })}
					</small>
				)}
			</div>
			<button
				type="button"
				disabled={!mine.length}
				aria-expanded={open}
				onClick={() => setOpen(!open)}
			>
				{t("Review")}
			</button>
			<button
				type="button"
				disabled={!mine.length}
				onClick={() => {
					onDiscard();
					setOpen(false);
				}}
			>
				{t("Discard")}
			</button>
			<button type="button" disabled>
				{t("Apply unavailable")}
			</button>
			{open && mine.length > 0 && (
				<section
					className="resource-review"
					aria-label={t("Proposed change review")}
				>
					<h2>
						{t("Proposed access for {{name}} — not authorization", { name })}
					</h2>
					<ul>
						{mine.map((draft) => {
							const target = byId(draft.resource);
							const operation =
								target &&
								operationsFor(target.kind).find(
									(op) => op.id === draft.operation,
								);
							return (
								<li
									key={`${draft.resource}:${draft.operation}`}
									data-mode={draft.mode}
								>
									<span translate="no">{target?.name}</span>
									<span>
										{operation ? t(operation.labelKey) : draft.operation}
									</span>
									<strong>{t(MODE[draft.mode])}</strong>
								</li>
							);
						})}
					</ul>
					<p>
						{t(
							"Each provider validates delegation and acknowledges adoption. Nothing is saved in this mockup.",
						)}
					</p>
				</section>
			)}
		</footer>
	);
}
