import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import {
	type PreviewRenderer,
	ResourcePreviewHost,
} from "../components/viewers/resource-preview-host";

describe("ResourcePreviewHost", () => {
	it("lets an injected renderer override the standard file renderer", () => {
		const modelRenderer: PreviewRenderer = {
			id: "test.model-3d",
			priority: 100,
			matches: (resource) => resource.uri.endsWith(".glb"),
			render: (resource) => <div>3D: {resource.label}</div>,
		};
		render(
			<ResourcePreviewHost
				resource={{
					kind: "workspace-file",
					uri: "/api/files/assembly.glb",
					label: "assembly.glb",
				}}
				renderers={[modelRenderer]}
			/>,
		);
		expect(screen.getByText("3D: assembly.glb")).toBeInTheDocument();
	});

	it.each([
		"notes.txt",
		"README.md",
		"photo.png",
		"manual.pdf",
		"clip.mp4",
		"table.csv",
		"data.json",
		"config.yaml",
		"feed.xml",
		"paper.typ",
	])("routes common resource %s through the standard file renderer", (name) => {
		const fetchSpy = vi
			.spyOn(globalThis, "fetch")
			.mockResolvedValue(new Response("preview", { status: 200 }));
		try {
			const view = render(
				<ResourcePreviewHost
					resource={{
						kind: "workspace-file",
						uri: `http://localhost/api/files/${name}`,
						label: name,
					}}
				/>,
			);
			expect(
				view.container.querySelector(
					'[data-resource-preview-renderer="oqto.standard-file"]',
				),
			).toBeInTheDocument();
			view.unmount();
		} finally {
			fetchSpy.mockRestore();
		}
	});
});
