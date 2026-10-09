import type { ReactNode } from "react";
import { FilePreview } from "./file-preview";

/** Host-neutral description of something selected for inspection. */
export type PreviewResource = {
	kind: "workspace-file";
	uri: string;
	label?: string;
	mimeType?: string;
	range?: { startLine?: number; endLine?: number };
};

/**
 * Web renderer registration. Higher priority adapters win; callers inject
 * adapters rather than mutating a process-global registry. A future 3D
 * adapter can match .glb/.gltf without changing Chat, layout, or this host.
 */
export type PreviewRenderer = {
	id: string;
	priority: number;
	matches: (resource: PreviewResource) => boolean;
	render: (resource: PreviewResource) => ReactNode;
};

type ResourcePreviewHostProps = {
	resource: PreviewResource;
	renderers?: readonly PreviewRenderer[];
};

const standardFileRenderer: PreviewRenderer = {
	id: "oqto.standard-file",
	priority: 0,
	matches: (resource) => resource.kind === "workspace-file",
	render: (resource) => (
		<FilePreview
			filename={
				resource.label ?? resource.uri.split("/").at(-1) ?? resource.uri
			}
			contentUrl={resource.uri}
			className="h-full"
		/>
	),
};

/** Selects one renderer for a semantic resource and owns fallback behavior. */
export function ResourcePreviewHost({
	resource,
	renderers = [],
}: ResourcePreviewHostProps) {
	const renderer = [...renderers, standardFileRenderer]
		.sort((left, right) => right.priority - left.priority)
		.find((candidate) => candidate.matches(resource));
	return renderer ? (
		<div className="h-full" data-resource-preview-renderer={renderer.id}>
			{renderer.render(resource)}
		</div>
	) : null;
}
