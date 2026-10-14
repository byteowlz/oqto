// Controlled Playwright entrypoint: production styles, scheme engine, and
// shared Button. This is a CSS/component fixture, not an authenticated app E2E.
import React from "react";
import { createRoot } from "react-dom/client";

import { Button } from "../../components/ui/button";
import { applyOqtoUiScheme } from "../../src/oqto-ui/platform/base24-theme";
import "../../src/styles/globals.css";
import "../../src/oqto-ui/app/shell.css";

const root = document.getElementById("root");
if (!root) throw new Error("missing fixture root");

applyOqtoUiScheme(document.documentElement, "oqto-dark");
createRoot(root).render(
	<main className="p-5" style={{ minHeight: "100vh" }}>
		<h1>Oqto control baseline fixture</h1>
		<div
			className="flex flex-wrap items-center gap-4 py-5"
			data-fixture="shared-button"
		>
			<Button id="look-shared-default">Primary control</Button>
			<Button id="look-shared-outline" variant="outline">
				Outline control
			</Button>
			<Button id="look-shared-secondary" variant="secondary" size="sm">
				Secondary small
			</Button>
		</div>
		<div
			className="flex flex-wrap items-center gap-4 py-5"
			data-fixture="additional-shared-button-variants-and-sizes"
		>
			<Button id="look-shared-default-sm" size="sm">
				Primary small
			</Button>
			<Button
				id="look-shared-default-icon"
				size="icon"
				aria-label="Primary icon"
			>
				<svg aria-hidden="true" viewBox="0 0 16 16" fill="none">
					<path d="M2 8h12M8 2v12" stroke="currentColor" />
				</svg>
			</Button>
			<Button id="look-shared-outline-sm" variant="outline" size="sm">
				Outline small
			</Button>
			<Button
				id="look-shared-outline-icon"
				variant="outline"
				size="icon"
				aria-label="Outline icon"
			>
				<svg aria-hidden="true" viewBox="0 0 16 16" fill="none">
					<path d="M2 8h12M8 2v12" stroke="currentColor" />
				</svg>
			</Button>
			<Button id="look-shared-secondary-default" variant="secondary">
				Secondary control
			</Button>
			<Button
				id="look-shared-secondary-icon"
				variant="secondary"
				size="icon"
				aria-label="Secondary icon"
			>
				<svg aria-hidden="true" viewBox="0 0 16 16" fill="none">
					<path d="M2 8h12M8 2v12" stroke="currentColor" />
				</svg>
			</Button>
			<Button id="look-shared-ghost-default" variant="ghost">
				Ghost control
			</Button>
			<Button id="look-shared-ghost-sm" variant="ghost" size="sm">
				Ghost small
			</Button>
			<Button
				id="look-shared-ghost-icon"
				variant="ghost"
				size="icon"
				aria-label="Ghost icon"
			>
				<svg aria-hidden="true" viewBox="0 0 16 16" fill="none">
					<path d="M2 8h12M8 2v12" stroke="currentColor" />
				</svg>
			</Button>
			<Button id="look-shared-destructive-default" variant="destructive">
				Destructive control
			</Button>
			<Button id="look-shared-destructive-sm" variant="destructive" size="sm">
				Destructive small
			</Button>
			<Button
				id="look-shared-destructive-icon"
				variant="destructive"
				size="icon"
				aria-label="Destructive icon"
			>
				<svg aria-hidden="true" viewBox="0 0 16 16" fill="none">
					<path d="M2 8h12M8 2v12" stroke="currentColor" />
				</svg>
			</Button>
		</div>
		<div
			className="wb-shell"
			style={{ height: "100px", width: "300px" }}
			data-fixture="workbench-control"
		>
			<div className="wb-sidebar__logo" style={{ width: "300px" }}>
				<button
					type="button"
					id="look-workbench-icon"
					className="wb-icon-button"
					aria-label="Workbench control"
				>
					<svg aria-hidden="true" viewBox="0 0 16 16" fill="none">
						<path d="M2 8h12M8 2v12" stroke="currentColor" />
					</svg>
				</button>
			</div>
		</div>
	</main>,
);
