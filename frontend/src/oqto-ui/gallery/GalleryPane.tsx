import { useTranslation } from "react-i18next";
import type { GalleryResource } from "../platform/contracts";

type GalleryPaneProps = {
	resources: GalleryResource[];
};

export function GalleryPane({ resources }: GalleryPaneProps) {
	const { t } = useTranslation();
	if (resources.length === 0) {
		return (
			<section className="wb-gallery" aria-label={t("oqtoUi.gallery.label")}>
				<p className="wb-gallery__empty">{t("oqtoUi.gallery.empty")}</p>
			</section>
		);
	}
	return (
		<section className="wb-gallery" aria-label={t("oqtoUi.gallery.label")}>
			<ul className="wb-gallery__grid">
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
		</section>
	);
}
