import {
	type CSSProperties,
	type ReactNode,
	useRef,
	useState,
	useSyncExternalStore,
} from "react";
import { useTranslation } from "react-i18next";
import { ModeBar } from "./ModeBar";
import { WorkdirMark } from "./WorkdirMark";
import { boundaryLayout } from "./graph-layout";
import type { PolicyEditor, PolicyNode } from "./model";
import "./policy-editor.css";

export interface PolicyTreeProps {
	editor: PolicyEditor;
	/** Resolve the host's existing project logo/monogram; never fetch paths here. */
	renderWorkdirIcon?: (identity: string) => ReactNode;
	onEditingChange?: (editing: boolean) => void;
}
export function PolicyTree({
	editor,
	renderWorkdirIcon,
	onEditingChange,
}: PolicyTreeProps) {
	const { t } = useTranslation();
	const state = useSyncExternalStore(
		editor.subscribe,
		editor.getSnapshot,
		editor.getSnapshot,
	);
	const [expanded, setExpanded] = useState<ReadonlySet<string>>(
		() => new Set(state.current.nodes.map((node) => node.id)),
	);
	const [selected, setSelected] = useState<string | undefined>(
		() => state.current.nodes.find((node) => node.workDirectoryId)?.id,
	);
	const refs = useRef(new Map<string, HTMLLIElement>());
	const viewport = useRef<HTMLDivElement>(null);
	const [zoom, setZoom] = useState(0.9);
	const [editing, setEditing] = useState(false);
	const nodes = state.current.nodes;
	const boundWorkdir = nodes.find((node) => node.workDirectoryId);
	const [focusedId, setFocusedId] = useState<string | undefined>(
		() => boundWorkdir?.id,
	);
	const focusedWorkdir = nodes.find((node) => node.id === focusedId);
	const canEdit = Boolean(
		focusedWorkdir && focusedWorkdir.id === boundWorkdir?.id,
	);
	const changeMode = (next: boolean) => {
		if (next && !canEdit) return;
		setEditing(next);
		onEditingChange?.(next);
	};
	const rows: { node: PolicyNode; depth: number }[] = [];
	const visit = (parent: string | null, depth: number, seen: Set<string>) => {
		for (const node of nodes.filter((n) => n.parentId === parent)) {
			if (seen.has(node.id)) continue;
			rows.push({ node, depth });
			if (expanded.has(node.id))
				visit(node.id, depth + 1, new Set([...seen, node.id]));
		}
	};
	visit(null, 1, new Set());
	const graph = boundaryLayout(nodes, expanded);
	const chosen = nodes.find((n) => n.id === selected) ?? rows[0]?.node;
	const focus = (id: string | undefined) => {
		if (id) {
			setSelected(id);
			refs.current.get(id)?.focus();
		}
	};
	const toggle = (id: string, open: boolean) =>
		setExpanded((old) => {
			const next = new Set(old);
			if (open) next.add(id);
			else next.delete(id);
			return next;
		});
	return (
		<section className="policy-editor" aria-label={t("Permission boundaries")}>
			<header>
				<span className="policy-eyebrow">{t("WORKSPACE / PERMISSIONS")}</span>
				<h2>{t("Permission boundaries")}</h2>
				{state.current.projection === "containment_only" && (
					<p>
						{t(
							"Containment projection only. Guard auto/ask/deny ceilings are not composed; this is not complete permission authority.",
						)}
					</p>
				)}
				<p>
					{t("Current revision")}: <code>{state.current.revision}</code> ·{" "}
					{t("Draft")}: {state.draftRevision}
				</p>
			</header>
			<ModeBar
				editing={editing}
				name={focusedWorkdir?.name}
				dirty={state.edits.length > 0}
				canEdit={canEdit}
				onChange={changeMode}
			/>
			<div className="policy-columns">
				<div className="policy-graph-viewport">
					<div className="policy-graph-caption">
						{t("Home → workspace → files")}{" "}
						<span>{t("Select a boundary to inspect or propose a change")}</span>
					</div>
					<div className="policy-graph-tools">
						<button
							type="button"
							onClick={() =>
								setZoom(
									Math.min(
										1,
										(viewport.current?.clientWidth ?? 900) / graph.width,
										470 / graph.height,
									),
								)
							}
						>
							{t("Fit")}
						</button>
						<button
							type="button"
							aria-label={t("Zoom out")}
							onClick={() => setZoom(Math.max(0.35, zoom - 0.1))}
						>
							−
						</button>
						<button
							type="button"
							aria-label={t("Zoom in")}
							onClick={() => setZoom(Math.min(1.6, zoom + 0.1))}
						>
							+
						</button>
						<span>{Math.round(zoom * 100)}%</span>
					</div>
					<div className="policy-graph-scroll" ref={viewport}>
						<div
							className="policy-graph-stage"
							style={
								{
									"--graph-width": `${graph.width}px`,
									"--graph-height": `${graph.height}px`,
									"--graph-zoom": zoom,
								} as CSSProperties
							}
						>
							<svg
								className="policy-connectors"
								width={graph.width}
								height={graph.height}
								aria-hidden="true"
							>
								{graph.visible.map((node) => {
									const point = graph.positions.get(node.id);
									const parent = node.parentId
										? graph.positions.get(node.parentId)
										: undefined;
									if (!point || !parent) return null;
									const start = parent.x + 194;
									return (
										<path
											key={node.id}
											d={`M ${start} ${parent.y} C ${start + 27} ${parent.y}, ${point.x - 27} ${point.y}, ${point.x} ${point.y}`}
										/>
									);
								})}
							</svg>
							<ul role="tree" aria-label={t("Filesystem permissions")}>
								{rows.map(({ node, depth }, index) => {
									const children = nodes.some((n) => n.parentId === node.id);
									const proposed = state.edits.find(
										(edit) => edit.nodeId === node.id,
									);
									return (
										<li
											key={node.id}
											role="treeitem"
											className="policy-graph-node"
											data-access={node.effective}
											data-workdir={Boolean(node.workDirectoryId)}
											data-focused={node.id === focusedId}
											data-proposed={state.edits.some(
												(edit) => edit.nodeId === node.id,
											)}
											style={
												{
													"--node-x": `${graph.positions.get(node.id)?.x ?? 0}px`,
													"--node-y": `${(graph.positions.get(node.id)?.y ?? 0) - 30}px`,
												} as CSSProperties
											}
											aria-level={depth}
											aria-selected={chosen?.id === node.id}
											aria-expanded={
												children ? expanded.has(node.id) : undefined
											}
											tabIndex={chosen?.id === node.id ? 0 : -1}
											ref={(el) => {
												if (el) refs.current.set(node.id, el);
												else refs.current.delete(node.id);
											}}
											onFocus={() => setSelected(node.id)}
											onClick={() => focus(node.id)}
											onKeyDown={(event) => {
												if (event.target !== event.currentTarget) return;
												const key = event.key;
												if (
													![
														"ArrowDown",
														"ArrowUp",
														"ArrowRight",
														"ArrowLeft",
														"Home",
														"End",
														"Enter",
														" ",
													].includes(key)
												)
													return;
												event.preventDefault();
												if (key === "ArrowDown")
													focus(rows[index + 1]?.node.id);
												if (key === "ArrowUp") focus(rows[index - 1]?.node.id);
												if (key === "Home") focus(rows[0]?.node.id);
												if (key === "End") focus(rows.at(-1)?.node.id);
												if (key === "ArrowRight" && children) {
													if (!expanded.has(node.id)) toggle(node.id, true);
													else focus(rows[index + 1]?.node.id);
												}
												if (key === "ArrowLeft") {
													if (expanded.has(node.id)) toggle(node.id, false);
													else focus(node.parentId ?? undefined);
												}
												if ((key === "Enter" || key === " ") && children)
													toggle(node.id, !expanded.has(node.id));
											}}
										>
											<span className="policy-node-icon" aria-hidden="true">
												{node.workDirectoryId ? (
													<WorkdirMark
														name={node.name}
														icon={renderWorkdirIcon?.(node.workDirectoryId)}
													/>
												) : (
													<svg
														aria-hidden="true"
														viewBox="0 0 24 24"
														width="19"
														height="19"
														fill="none"
														stroke="currentColor"
														strokeWidth="1.4"
													>
														<path d="M3 7V5h6l2 2h10v12H3Z" />
													</svg>
												)}
											</span>
											{children ? (
												<button
													type="button"
													tabIndex={-1}
													aria-label={
														expanded.has(node.id)
															? t("Collapse {{name}}", { name: node.name })
															: t("Expand {{name}}", { name: node.name })
													}
													onClick={(event) => {
														event.stopPropagation();
														focus(node.id);
														toggle(node.id, !expanded.has(node.id));
													}}
												>
													{expanded.has(node.id) ? "▾" : "▸"}
												</button>
											) : null}
											<span className="policy-node-name" translate="no">
												{node.name}
											</span>{" "}
											<small>
												{t(node.effective)}
												{proposed ? ` → ${t(proposed.mode)}` : ""} ·{" "}
												{t(node.inherited ? "inherited" : "explicit")}
												{node.mixed && ` · ${t("mixed")}`}
												{(node.locked || node.lockReason) &&
													` · ${t("Locked")}`}
												{node.containsLockedDescendant &&
													` · ${t("Contains locked descendants")}`}
											</small>
										</li>
									);
								})}
							</ul>
						</div>
					</div>
					<div className="policy-graph-legend">
						<span>{t("Read / write")}</span>
						<span>{t("Read only")}</span>
						<span>{t("Protected")}</span>
						<span>{t("Proposed change")}</span>
					</div>
				</div>
				{chosen && (
					<aside aria-label={t("Selected boundary")}>
						<h3 translate="no">{chosen.path}</h3>
						<p>
							{chosen.origin ? t(chosen.origin) : t("Origin not supplied")} ·{" "}
							{t(chosen.inherited ? "inherited" : "explicit")}
							{chosen.mixed && ` · ${t("mixed")}`}
						</p>
						<p>
							{t("Scope")}: {chosen.scope ?? t("Not supplied")} ·{" "}
							{t("Duration")}: {chosen.duration ?? t("Not supplied")}
						</p>
						{chosen.containsLockedDescendant && (
							<p>
								{t(
									"Contains locked descendants; subtree edits cannot override those locks",
								)}
							</p>
						)}
						<details>
							<summary>{t("Resolver provenance")}</summary>
							{chosen.provenance.decisions.map((decision, index) => (
								<p
									key={`${decision.origin.layer}:${decision.origin.source}:${index}`}
									translate="no"
								>
									{decision.access} · {decision.origin.layer} ·{" "}
									{decision.origin.source} · {decision.matchedPath ?? "—"}
								</p>
							))}
							<h4>{t("Decisive resolver rules")}</h4>
							{chosen.provenance.decisive.map((decision, index) => (
								<p
									key={`${decision.origin.layer}:${decision.origin.source}:${index}`}
									translate="no"
								>
									{decision.access} · {decision.origin.layer} ·{" "}
									{decision.origin.source} · {decision.matchedPath ?? "—"}
								</p>
							))}
						</details>
						{chosen.lockReason && (
							<p>
								{t("Locked")}: {chosen.lockReason}
							</p>
						)}
						<p>
							{t("Current")}: {t(chosen.effective)}
						</p>
						<p>
							{t("Proposed")}:{" "}
							{state.edits.find((e) => e.nodeId === chosen.id)?.mode ??
								t("Unchanged")}
						</p>
						{!editing && chosen.workDirectoryId && (
							<button type="button" onClick={() => setFocusedId(chosen.id)}>
								{t("Focus this workdir")}
							</button>
						)}
						{!editing && (
							<p>
								{t("Navigate mode · enter Edit boundaries to propose changes")}
							</p>
						)}
						{editing && (
							<fieldset
								disabled={
									chosen.locked ||
									Boolean(chosen.lockReason) ||
									state.pending === "apply" ||
									Boolean(state.publishedNotAdopted) ||
									Boolean(state.publicationUncertain)
								}
							>
								<legend>{t("Propose access")}</legend>
								{chosen.allowedChoices
									.filter(
										(mode) =>
											state.current.projection !== "containment_only" ||
											mode !== "ask",
									)
									.map((mode) => (
										<button
											type="button"
											key={mode}
											onClick={() =>
												editor.edit({
													nodeId: chosen.id,
													mode,
													source: "operator",
												})
											}
										>
											{t(mode)}
										</button>
									))}
							</fieldset>
						)}
						{!chosen.allowedChoices.length && <p>{t("No supported edits")}</p>}
					</aside>
				)}
			</div>
			<footer>
				<button
					type="button"
					disabled={
						!editing ||
						!editor.hasAuthority ||
						!state.edits.length ||
						Boolean(state.pending) ||
						Boolean(state.publishedNotAdopted) ||
						Boolean(state.publicationUncertain)
					}
					onClick={() => void editor.preview()}
				>
					{t("Preview effective changes")}
				</button>
				<button
					type="button"
					disabled={!editing || !canEdit || !editor.canApply()}
					onClick={() => void editor.apply()}
				>
					{t("Apply reviewed draft")}
				</button>
				<output aria-live="polite">
					{state.pending
						? t("Awaiting authority")
						: state.applied && !state.edits.length
							? t("Applied acknowledgement received")
							: t("Not applied")}
				</output>
				{!editor.hasAuthority && (
					<p>{t("Apply unavailable: trusted authority is not connected")}</p>
				)}
				{state.preview && (
					<div>
						<h3>{t("Authoritative proposed preview")}</h3>
						<ul>
							{state.preview.diff.map((line) => (
								<li key={line}>{line}</li>
							))}
						</ul>
						{state.preview.nodes.map((node) => (
							<p key={node.id}>
								<span translate="no">{node.path}</span>: {t(node.effective)} ·{" "}
								{t(node.inherited ? "inherited" : "explicit")}{" "}
								{node.mixed && t("mixed")} {node.locked && t("Locked")}
							</p>
						))}
					</div>
				)}
				{state.publicationUncertain && (
					<p role="alert">
						{t(
							"Publication uncertain: effects may have occurred. Reconcile with protected authority; do not retry blindly.",
						)}
					</p>
				)}
				{state.publishedNotAdopted && (
					<p role="alert">
						{t(
							"Published, not adopted: runtime enforcement has not acknowledged this policy. Refresh from trusted authority before editing.",
						)}
					</p>
				)}
				{state.error && <p role="alert">{state.error}</p>}
				<p>
					{t("Skills, wiki and memory: unsupported until enforcement exists")}
				</p>
			</footer>
		</section>
	);
}
