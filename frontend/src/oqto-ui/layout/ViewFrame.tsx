import type { ReactNode } from "react";

type ViewFrameProps = {
	viewId: string;
	title: string;
	meta: string;
	children: ReactNode;
};

export function ViewFrame({ viewId, title, meta, children }: ViewFrameProps) {
	return (
		<section
			className="oui-view"
			data-view-id={viewId}
			aria-labelledby={`${viewId}-title`}
		>
			<header className="oui-view-header">
				<h2 id={`${viewId}-title`}>{title}</h2>
				<span translate="no">{meta}</span>
			</header>
			<div className="oui-view-content">{children}</div>
		</section>
	);
}
