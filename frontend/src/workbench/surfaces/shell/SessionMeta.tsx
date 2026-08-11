import { useState } from "react";
import { useTranslation } from "react-i18next";
import type { LabModelOption, LabSession } from "../../modules/lab/model";

type SessionMetaProps = {
	session: LabSession;
	models: LabModelOption[];
};

/**
 * Context-aware statusbar items for the selected Session: model (click opens
 * the picker) and context readout (hover/focus opens details).
 */
export function SessionMeta({ session, models }: SessionMetaProps) {
	const { t } = useTranslation();
	const [modelId, setModelId] = useState(session.model);
	const [menuOpen, setMenuOpen] = useState(false);
	const activeModel =
		models.find((model) => model.id === modelId)?.name ?? modelId;

	return (
		<>
			<span className="wb-statusbar__meta">
				<button
					type="button"
					aria-expanded={menuOpen}
					aria-label={t("workbench.chat.model")}
					onClick={() => setMenuOpen((current) => !current)}
				>
					{activeModel}
				</button>
				{menuOpen ? (
					<span className="wb-composer__pop wb-composer__model-menu">
						{models.map((model) => (
							<button
								key={model.id}
								type="button"
								aria-pressed={model.id === modelId}
								onClick={() => {
									setModelId(model.id);
									setMenuOpen(false);
								}}
							>
								{model.name}
							</button>
						))}
					</span>
				) : null}
			</span>
			{session.context ? (
				<span className="wb-statusbar__meta">
					<button type="button" aria-label={t("workbench.chat.contextUsage")}>
						{t("workbench.chat.contextReadout", {
							tokens: session.context.tokens,
							percent: session.context.percent,
						})}
					</button>
					{!menuOpen ? (
						<dl className="wb-composer__pop wb-composer__context-details">
							<dt>{t("workbench.chat.contextModel")}</dt>
							<dd>{activeModel}</dd>
							<dt>{t("workbench.chat.contextWindow")}</dt>
							<dd>{session.context.window}</dd>
							<dt>{t("workbench.chat.contextUsed")}</dt>
							<dd>
								{t("workbench.chat.contextReadout", {
									tokens: session.context.tokens,
									percent: session.context.percent,
								})}
							</dd>
						</dl>
					) : null}
				</span>
			) : null}
		</>
	);
}
