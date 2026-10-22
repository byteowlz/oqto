import { useTranslation } from "react-i18next";
import type { OperationView } from "./access";
import type { GrantMode } from "./resources";

const MODES: readonly GrantMode[] = ["allow", "ask", "deny"];
const LABEL: Record<GrantMode, string> = {
	allow: "Allow",
	ask: "Ask",
	deny: "Deny",
};
interface OperationRowProps {
	operation: OperationView;
	resourceName: string;
	editable: boolean;
	onPropose: (operation: string, mode: GrantMode) => void;
}
/** One operation, edited in place on its node. */
export function OperationRow({
	operation,
	resourceName,
	editable,
	onPropose,
}: OperationRowProps) {
	const { t } = useTranslation();
	const effective = operation.proposed ?? operation.current;
	return (
		<fieldset
			className="resource-op"
			data-changed={Boolean(operation.proposed)}
			data-locked={Boolean(operation.lockKey)}
			disabled={!editable || Boolean(operation.lockKey)}
			aria-label={t("{{operation}} for {{resource}}", {
				operation: t(operation.labelKey),
				resource: resourceName,
			})}
		>
			<span className="resource-op-label">{t(operation.labelKey)}</span>
			{operation.lockKey ? (
				<span className="resource-op-lock" title={t(operation.lockKey)}>
					{t("Locked")}
				</span>
			) : (
				<span className="resource-op-modes">
					{MODES.map((mode) => (
						<button
							key={mode}
							type="button"
							data-mode={mode}
							data-current={operation.current === mode}
							aria-pressed={effective === mode}
							onClick={() => onPropose(operation.id, mode)}
						>
							{t(LABEL[mode])}
						</button>
					))}
				</span>
			)}
		</fieldset>
	);
}

interface AccessChipsProps {
	operations: readonly OperationView[];
}
/** Compact read-only summary shown on collapsed nodes. */
export function AccessChips({ operations }: AccessChipsProps) {
	const { t } = useTranslation();
	return (
		<span className="resource-chips" aria-hidden="true">
			{operations.map((operation) => (
				<span
					key={operation.id}
					data-mode={operation.proposed ?? operation.current}
					data-changed={Boolean(operation.proposed)}
					data-locked={Boolean(operation.lockKey)}
				>
					{t(operation.shortKey)}
				</span>
			))}
		</span>
	);
}
