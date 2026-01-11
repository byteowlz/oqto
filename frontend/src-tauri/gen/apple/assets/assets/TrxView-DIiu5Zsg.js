import {
	K as A,
	i as G,
	G as K,
	L,
	H as Q,
	x as R,
	y as S,
	M as W,
	l as X,
	N as Y,
	P as Z,
	j as e,
	X as ee,
	B as f,
	r as n,
	R as q,
	J as v,
	c as x,
} from "./index-RFMA0cqP.js"; /**
 * @license lucide-react v0.556.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const se = [
	["path", { d: "M12 20v-9", key: "1qisl0" }],
	[
		"path",
		{
			d: "M14 7a4 4 0 0 1 4 4v3a6 6 0 0 1-12 0v-3a4 4 0 0 1 4-4z",
			key: "uouzyp",
		},
	],
	["path", { d: "M14.12 3.88 16 2", key: "qol33r" }],
	["path", { d: "M21 21a4 4 0 0 0-3.81-4", key: "1b0z45" }],
	["path", { d: "M21 5a4 4 0 0 1-3.55 3.97", key: "5cxbf6" }],
	["path", { d: "M22 13h-4", key: "1jl80f" }],
	["path", { d: "M3 21a4 4 0 0 1 3.81-4", key: "1fjd4g" }],
	["path", { d: "M3 5a4 4 0 0 0 3.55 3.97", key: "1d7oge" }],
	["path", { d: "M6 13H2", key: "82j7cp" }],
	["path", { d: "m8 2 1.88 1.88", key: "fmnt4t" }],
	["path", { d: "M9 7.13V6a3 3 0 1 1 6 0v1.13", key: "1vgav8" }],
];
const te = S("bug", se); /**
 * @license lucide-react v0.556.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const ae = [
	[
		"rect",
		{
			width: "8",
			height: "4",
			x: "8",
			y: "2",
			rx: "1",
			ry: "1",
			key: "tgr4d6",
		},
	],
	[
		"path",
		{
			d: "M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2",
			key: "116196",
		},
	],
	["path", { d: "M12 11h4", key: "1jrz19" }],
	["path", { d: "M12 16h4", key: "n85exb" }],
	["path", { d: "M8 11h.01", key: "1dfujw" }],
	["path", { d: "M8 16h.01", key: "18s6g9" }],
];
const T = S("clipboard-list", ae); /**
 * @license lucide-react v0.556.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */
const re = [
	["circle", { cx: "12", cy: "12", r: "10", key: "1mglay" }],
	["circle", { cx: "12", cy: "12", r: "6", key: "1vlfrh" }],
	["circle", { cx: "12", cy: "12", r: "2", key: "1c9p78" }],
];
const ne = S("target", re);
async function oe(d) {
	const t = new URL(v("/api/workspace/trx/issues"), window.location.origin);
	t.searchParams.set("workspace_path", d);
	const r = await fetch(t.toString(), { credentials: "include" });
	if (!r.ok) {
		if (r.status === 404) return [];
		throw new Error(`Failed to fetch TRX issues: ${r.statusText}`);
	}
	return r.json();
}
async function ie(d, t) {
	const r = new URL(v("/api/workspace/trx/issues"), window.location.origin);
	r.searchParams.set("workspace_path", d);
	const a = await fetch(r.toString(), {
		method: "POST",
		credentials: "include",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify(t),
	});
	if (!a.ok) {
		const o = await a.text();
		throw new Error(`Failed to create issue: ${o || a.statusText}`);
	}
	return a.json();
}
async function le(d, t, r) {
	const a = new URL(
		v(`/api/workspace/trx/issues/${t}`),
		window.location.origin,
	);
	a.searchParams.set("workspace_path", d);
	const o = await fetch(a.toString(), {
		method: "PUT",
		credentials: "include",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify(r),
	});
	if (!o.ok) throw new Error(`Failed to update issue: ${o.statusText}`);
	return o.json();
}
async function ce(d, t, r) {
	const a = new URL(
		v(`/api/workspace/trx/issues/${t}/close`),
		window.location.origin,
	);
	a.searchParams.set("workspace_path", d);
	const o = await fetch(a.toString(), {
		method: "POST",
		credentials: "include",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify({ reason: r }),
	});
	if (!o.ok) throw new Error(`Failed to close issue: ${o.statusText}`);
	return o.json();
}
const B = {
	bug: { icon: te, color: "text-red-400", label: "Bug" },
	feature: { icon: Q, color: "text-purple-400", label: "Feature" },
	task: { icon: T, color: "text-blue-400", label: "Task" },
	epic: { icon: ne, color: "text-amber-400", label: "Epic" },
	chore: { icon: A, color: "text-gray-400", label: "Chore" },
};
const P = {
	0: "bg-red-500/20 text-red-400 border-red-500/30",
	1: "bg-orange-500/20 text-orange-400 border-orange-500/30",
	2: "bg-yellow-500/20 text-yellow-400 border-yellow-500/30",
	3: "bg-green-500/20 text-green-400 border-green-500/30",
	4: "bg-gray-500/20 text-gray-400 border-gray-500/30",
};
const $ = {
	open: "bg-blue-500/20 text-blue-400",
	in_progress: "bg-purple-500/20 text-purple-400",
	closed: "bg-green-500/20 text-green-400",
	blocked: "bg-red-500/20 text-red-400",
};
const U = n.memo(function d({
	issue: t,
	childIssues: r,
	isExpanded: a,
	onToggle: o,
	onStatusChange: h,
	onEdit: b,
	depth: w = 0,
}) {
	const u = B[t.issue_type] || B.task;
	const k = u.icon;
	const N = r && r.length > 0;
	const y = t.status === "closed";
	return e.jsxs("div", {
		className: x("space-y-1", w > 0 && "ml-4 border-l border-border pl-2"),
		children: [
			e.jsxs("div", {
				className: x(
					"group flex items-start gap-2 p-2 rounded transition-colors",
					y ? "opacity-50" : "hover:bg-muted/50",
				),
				children: [
					N
						? e.jsx("button", {
								type: "button",
								onClick: o,
								className: "flex-shrink-0 mt-0.5 p-0.5 hover:bg-muted rounded",
								children: a
									? e.jsx(W, { className: "w-3 h-3 text-muted-foreground" })
									: e.jsx(Y, { className: "w-3 h-3 text-muted-foreground" }),
							})
						: e.jsx("div", { className: "w-4" }),
					e.jsx(k, { className: x("w-4 h-4 flex-shrink-0 mt-0.5", u.color) }),
					e.jsxs("div", {
						className: "flex-1 min-w-0",
						children: [
							e.jsxs("div", {
								className: "flex items-center gap-2",
								children: [
									e.jsx("span", {
										className: x(
											"text-sm font-medium truncate",
											y && "line-through text-muted-foreground",
										),
										children: t.title,
									}),
									e.jsx("span", {
										className: "text-[10px] font-mono text-muted-foreground",
										children: t.id,
									}),
								],
							}),
							t.description &&
								e.jsx("p", {
									className:
										"text-xs text-muted-foreground line-clamp-1 mt-0.5",
									children: t.description,
								}),
						],
					}),
					e.jsxs("div", {
						className: "flex items-center gap-1 flex-shrink-0",
						children: [
							e.jsx(R, {
								variant: "outline",
								className: x("text-[9px] px-1 py-0 h-4", $[t.status] || $.open),
								children: t.status.replace("_", " "),
							}),
							e.jsxs(R, {
								variant: "outline",
								className: x(
									"text-[9px] px-1 py-0 h-4 border",
									P[t.priority] || P[2],
								),
								children: ["P", t.priority],
							}),
						],
					}),
					e.jsx("div", {
						className:
							"flex items-center gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity",
						children:
							!y &&
							e.jsxs(e.Fragment, {
								children: [
									e.jsx(f, {
										type: "button",
										variant: "ghost",
										size: "sm",
										onClick: b,
										className: "h-5 w-5 p-0",
										title: "Edit",
										children: e.jsx(Z, { className: "w-3 h-3" }),
									}),
									t.status !== "in_progress" &&
										e.jsx(f, {
											type: "button",
											variant: "ghost",
											size: "sm",
											onClick: () => h("in_progress"),
											className: "h-5 w-5 p-0",
											title: "Start",
											children: e.jsx(A, {
												className: "w-3 h-3 text-purple-400",
											}),
										}),
									e.jsx(f, {
										type: "button",
										variant: "ghost",
										size: "sm",
										onClick: () => h("closed"),
										className: "h-5 w-5 p-0",
										title: "Close",
										children: e.jsx(ee, {
											className: "w-3 h-3 text-green-400",
										}),
									}),
								],
							}),
					}),
				],
			}),
			N &&
				a &&
				e.jsx("div", {
					className: "space-y-1",
					children: r.map((j) =>
						e.jsx(
							d,
							{
								issue: j,
								isExpanded: !1,
								onToggle: () => {},
								onStatusChange: (m) => h(m),
								onEdit: b,
								depth: w + 1,
							},
							j.id,
						),
					),
				}),
		],
	});
});
const xe = n.memo(({ workspacePath: t, className: r }) => {
	const [a, o] = n.useState([]);
	const [h, b] = n.useState(!0);
	const [w, u] = n.useState("");
	const [k, N] = n.useState(new Set());
	const [y, j] = n.useState(!1);
	const [m, E] = n.useState("");
	const [C, O] = n.useState("task");
	const [_, M] = n.useState(!1);
	const g = n.useCallback(async () => {
		if (t) {
			b(!0), u("");
			try {
				const s = await oe(t);
				o(s);
			} catch (s) {
				u(s instanceof Error ? s.message : "Failed to load issues");
			} finally {
				b(!1);
			}
		}
	}, [t]);
	n.useEffect(() => {
		g();
	}, [g]);
	const {
		epics: J,
		standaloneIssues: V,
		childrenByParent: D,
	} = n.useMemo(() => {
		const s = new Map();
		const i = [];
		const c = [];
		for (const l of a)
			if (l.parent_id) {
				const z = s.get(l.parent_id) || [];
				z.push(l), s.set(l.parent_id, z);
			} else l.issue_type === "epic" ? c.push(l) : i.push(l);
		return { epics: c, standaloneIssues: i, childrenByParent: s };
	}, [a]);
	const H = n.useCallback((s) => {
		N((i) => {
			const c = new Set(i);
			return c.has(s) ? c.delete(s) : c.add(s), c;
		});
	}, []);
	const I = n.useCallback(
		async (s, i) => {
			if (t)
				try {
					i === "closed" ? await ce(t, s) : await le(t, s, { status: i }),
						await g();
				} catch (c) {
					u(c instanceof Error ? c.message : "Failed to update issue");
				}
		},
		[t, g],
	);
	const F = n.useCallback(async () => {
		if (!(!t || !m.trim())) {
			M(!0), u("");
			try {
				await ie(t, { title: m, issue_type: C }), E(""), j(!1), await g();
			} catch (s) {
				u(s instanceof Error ? s.message : "Failed to create issue");
			} finally {
				M(!1);
			}
		}
	}, [t, m, C, g]);
	const p = n.useMemo(() => {
		const s = a.filter((l) => l.status === "open").length;
		const i = a.filter((l) => l.status === "in_progress").length;
		const c = a.filter((l) => l.status === "closed").length;
		return { open: s, inProgress: i, closed: c, total: a.length };
	}, [a]);
	return t
		? h
			? e.jsx("div", {
					className: x("flex items-center justify-center h-full", r),
					children: e.jsxs("div", {
						className: "text-center text-muted-foreground",
						children: [
							e.jsx(L, { className: "w-6 h-6 mx-auto mb-2 animate-spin" }),
							e.jsx("p", {
								className: "text-xs",
								children: "Loading issues...",
							}),
						],
					}),
				})
			: e.jsxs("div", {
					className: x("flex flex-col h-full overflow-hidden", r),
					children: [
						e.jsxs("div", {
							className: "flex-shrink-0 p-2 border-b border-border",
							children: [
								e.jsxs("div", {
									className: "flex items-center justify-between mb-2",
									children: [
										e.jsx("span", {
											className:
												"text-xs font-medium text-muted-foreground uppercase tracking-wider",
											children: "Issues",
										}),
										e.jsxs("div", {
											className: "flex items-center gap-1",
											children: [
												e.jsx(f, {
													type: "button",
													variant: "ghost",
													size: "sm",
													onClick: g,
													disabled: h,
													className: "h-6 w-6 p-0",
													title: "Refresh",
													children: e.jsx(q, {
														className: x("w-3 h-3", h && "animate-spin"),
													}),
												}),
												e.jsx(f, {
													type: "button",
													variant: "ghost",
													size: "sm",
													onClick: () => j(!y),
													className: "h-6 w-6 p-0",
													title: "Add issue",
													children: e.jsx(K, { className: "w-3 h-3" }),
												}),
											],
										}),
									],
								}),
								p.total > 0 &&
									e.jsxs("div", {
										className:
											"flex items-center gap-3 text-[10px] text-muted-foreground",
										children: [
											e.jsxs("span", { children: [p.total, " total"] }),
											p.inProgress > 0 &&
												e.jsxs("span", {
													className: "text-purple-400",
													children: [p.inProgress, " active"],
												}),
											p.open > 0 &&
												e.jsxs("span", {
													className: "text-blue-400",
													children: [p.open, " open"],
												}),
											p.closed > 0 &&
												e.jsxs("span", {
													className: "text-green-400",
													children: [p.closed, " done"],
												}),
										],
									}),
								y &&
									e.jsxs("div", {
										className: "mt-2 p-2 bg-muted/30 rounded space-y-2",
										children: [
											e.jsx(X, {
												value: m,
												onChange: (s) => E(s.target.value),
												placeholder: "Issue title...",
												className: "h-7 text-xs",
												onKeyDown: (s) => s.key === "Enter" && F(),
											}),
											e.jsxs("div", {
												className: "flex items-center gap-2",
												children: [
													e.jsxs("select", {
														value: C,
														onChange: (s) => O(s.target.value),
														className:
															"h-6 text-xs bg-background border border-border rounded px-2",
														children: [
															e.jsx("option", {
																value: "task",
																children: "Task",
															}),
															e.jsx("option", {
																value: "bug",
																children: "Bug",
															}),
															e.jsx("option", {
																value: "feature",
																children: "Feature",
															}),
															e.jsx("option", {
																value: "epic",
																children: "Epic",
															}),
															e.jsx("option", {
																value: "chore",
																children: "Chore",
															}),
														],
													}),
													e.jsx("div", { className: "flex-1" }),
													e.jsx(f, {
														type: "button",
														variant: "ghost",
														size: "sm",
														onClick: () => j(!1),
														className: "h-6 px-2 text-xs",
														children: "Cancel",
													}),
													e.jsx(f, {
														type: "button",
														variant: "default",
														size: "sm",
														onClick: F,
														disabled: _ || !m.trim(),
														className: "h-6 px-2 text-xs",
														children: _
															? e.jsx(L, { className: "w-3 h-3 animate-spin" })
															: "Create",
													}),
												],
											}),
										],
									}),
							],
						}),
						w &&
							e.jsxs("div", {
								className:
									"flex-shrink-0 px-2 py-1 bg-destructive/10 text-destructive text-xs flex items-center gap-1",
								children: [e.jsx(G, { className: "w-3 h-3" }), w],
							}),
						e.jsx("div", {
							className: "flex-1 overflow-auto p-2 space-y-1",
							children:
								a.length === 0
									? e.jsx("div", {
											className: "flex items-center justify-center h-full",
											children: e.jsxs("div", {
												className: "text-center text-muted-foreground",
												children: [
													e.jsx(T, {
														className: "w-8 h-8 mx-auto mb-2 opacity-50",
													}),
													e.jsx("p", {
														className: "text-xs",
														children: "No issues yet",
													}),
													e.jsxs("p", {
														className: "text-[10px] mt-1",
														children: [
															"Click + to create one or run",
															" ",
															e.jsx("code", {
																className: "bg-muted px-1 rounded",
																children: "trx init",
															}),
														],
													}),
												],
											}),
										})
									: e.jsxs(e.Fragment, {
											children: [
												J.map((s) =>
													e.jsx(
														U,
														{
															issue: s,
															childIssues: D.get(s.id),
															isExpanded: k.has(s.id),
															onToggle: () => H(s.id),
															onStatusChange: (i) => I(s.id, i),
															onEdit: () => {},
														},
														s.id,
													),
												),
												V.map((s) =>
													e.jsx(
														U,
														{
															issue: s,
															isExpanded: !1,
															onToggle: () => {},
															onStatusChange: (i) => I(s.id, i),
															onEdit: () => {},
														},
														s.id,
													),
												),
											],
										}),
						}),
					],
				})
		: e.jsx("div", {
				className: x("flex items-center justify-center h-full", r),
				children: e.jsxs("div", {
					className: "text-center text-muted-foreground",
					children: [
						e.jsx(T, { className: "w-8 h-8 mx-auto mb-2 opacity-50" }),
						e.jsx("p", {
							className: "text-xs",
							children: "No workspace selected",
						}),
					],
				}),
			});
});
export { xe as TrxView };
