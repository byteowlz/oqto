import { useTranslation } from "react-i18next";
import type { WorkbenchStatus } from "../../modules/lab/model";

type StatusMarkProps = {
	status: WorkbenchStatus;
	compact?: boolean;
};

export function StatusMark({ status, compact = false }: StatusMarkProps) {
	const { t } = useTranslation();
	return (
		<span className={`wb-status wb-status--${status}`} data-compact={compact}>
			<span className="wb-status__glyph" aria-hidden="true" />
			{compact ? null : <span>{t(`workbench.status.${status}`)}</span>}
		</span>
	);
}
