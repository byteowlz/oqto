import { useTranslation } from "react-i18next";

interface ModeBarProps {
	editing: boolean;
	name?: string;
	dirty: boolean;
	canEdit: boolean;
	onChange: (editing: boolean) => void;
}
export function ModeBar({
	editing,
	name,
	dirty,
	canEdit,
	onChange,
}: ModeBarProps) {
	const { t } = useTranslation();
	return (
		<div className="policy-mode-bar">
			<fieldset
				className="policy-mode-switch"
				aria-label={t("Interaction mode")}
			>
				<button
					type="button"
					aria-pressed={!editing}
					onClick={() => onChange(false)}
				>
					{t("Navigate")}
				</button>
				<button
					type="button"
					aria-pressed={editing}
					disabled={!canEdit}
					onClick={() => onChange(true)}
				>
					{t("Edit boundaries")}
				</button>
			</fieldset>
			<p aria-live="polite">
				{editing ? t("Editing boundaries for") : t("Focused workdir")}:{" "}
				<strong translate="no">{name ?? t("Select a workdir")}</strong>
			</p>
			<span>
				{editing
					? t("Focus pinned · select any boundary to inspect")
					: dirty
						? t("Draft preserved · navigation cannot edit permissions")
						: t("Browse workdirs · permissions are read-only")}
			</span>
		</div>
	);
}
