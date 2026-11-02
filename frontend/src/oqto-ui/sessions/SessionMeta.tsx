import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import type {
	ModelOption,
	OqtoUiPlatform,
	SessionOverview,
} from "../platform/contracts";

type SessionMetaProps = {
	session: SessionOverview;
	models: ModelOption[];
	platform?: Pick<OqtoUiPlatform, "id" | "selectModel">;
};

/**
 * Context-aware statusbar items for the selected Session: model (click opens
 * the picker) and context readout (hover/focus opens details).
 */
export function SessionMeta({ session, models, platform }: SessionMetaProps) {
	const { t } = useTranslation();
	const queryClient = useQueryClient();
	const [menuOpen, setMenuOpen] = useState(false);
	const selection = useMutation({
		mutationFn: async (modelId: string) => {
			if (!platform?.selectModel) return;
			await platform.selectModel(session.id, modelId);
		},
		onSuccess: async () => {
			if (!platform?.selectModel) return;
			setMenuOpen(false);
			await Promise.all([
				queryClient.invalidateQueries({ queryKey: ["oqto-ui", platform.id] }),
				queryClient.invalidateQueries({
					queryKey: ["oqto-ui-compositor", platform.id],
				}),
			]);
		},
	});
	const switchingDisabled = !platform?.selectModel || selection.isPending;
	const activeModel =
		models.find((model) => model.id === session.model)?.name ?? session.model;

	return (
		<>
			<span className="wb-statusbar__meta">
				<button
					type="button"
					disabled={switchingDisabled}
					aria-busy={selection.isPending}
					aria-expanded={menuOpen}
					aria-label={t("oqtoUi.chat.model")}
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
								disabled={switchingDisabled || model.id === session.model}
								aria-pressed={model.id === session.model}
								onClick={() => selection.mutate(model.id)}
							>
								{model.name}
							</button>
						))}
					</span>
				) : null}
				{selection.error ? (
					<span role="alert">{selection.error.message}</span>
				) : null}
			</span>
			{session.context ? (
				<span className="wb-statusbar__meta">
					<button type="button" aria-label={t("oqtoUi.chat.contextUsage")}>
						{t("oqtoUi.chat.contextReadout", {
							tokens: session.context.tokens,
							percent: session.context.percent,
						})}
					</button>
					{!menuOpen ? (
						<dl className="wb-composer__pop wb-composer__context-details">
							<dt>{t("oqtoUi.chat.contextModel")}</dt>
							<dd>{activeModel}</dd>
							<dt>{t("oqtoUi.chat.contextWindow")}</dt>
							<dd>{session.context.window}</dd>
							<dt>{t("oqtoUi.chat.contextUsed")}</dt>
							<dd>
								{t("oqtoUi.chat.contextReadout", {
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
