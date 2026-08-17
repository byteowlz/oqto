import { useTranslation } from "react-i18next";
import type { GalleryResource } from "../platform/contracts";

type GalleryViewProps = {
	resources: GalleryResource[];
};

const APP_ID = "oqto.app.gallery";
const DRAFT_ID = "draft · 7c42a1";
const SOURCE_ROOT = "apps/gallery/";

export function GalleryView({ resources }: GalleryViewProps) {
	const { t } = useTranslation();
	return (
		<div className="oui-gallery-app">
			<header className="oui-app-binding">
				<div>
					<strong translate="no">{APP_ID}</strong>
					<span>{t("oqtoUi.gallery.agentLocal")}</span>
				</div>
				<code translate="no">{DRAFT_ID}</code>
			</header>
			{resources.length === 0 ? (
				<div className="oui-empty">
					<p>{t("oqtoUi.gallery.empty")}</p>
					<code translate="no">{SOURCE_ROOT}</code>
				</div>
			) : (
				<ul className="oui-gallery-grid" aria-label={t("oqtoUi.gallery.label")}>
					{resources.map((resource) => (
						<li key={resource.id}>
							<button
								type="button"
								aria-label={t("oqtoUi.gallery.open", { name: resource.name })}
							>
								<img
									src={resource.src}
									alt=""
									width={resource.width}
									height={resource.height}
								/>
								<span>{resource.name}</span>
								<small translate="no">{resource.revision}</small>
							</button>
						</li>
					))}
				</ul>
			)}
		</div>
	);
}
