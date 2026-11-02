import { MarkdownRenderer } from "@/components/data-display";
/**
 * OpenUI parts in the web chat (ADR-0074, oqto-8qse.4).
 *
 * The agent's `openui` tool call carries an OpenUI Lang program from the Oqto
 * profile. OqtoUI parses it with the same profile the Pi TUI uses and renders it
 * with Oqto's own components and tokens, so a program looks the same here and in
 * the terminal. Rendering is read-only: the tool runs inside the agent's session,
 * so answers are given where it asked (the terminal today), never fabricated here.
 *
 * Safety: the program is data, not code. It is parsed against a closed component
 * schema, unknown components are dropped, no HTML is injected and no action runs.
 */
import { createOqtoParser } from "@byteowlz/oqto-openui";
import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import "./openui-part.css";

type Element = {
	type: "element";
	typeName: string;
	props: Record<string, unknown>;
};

const isElement = (v: unknown): v is Element =>
	!!v &&
	typeof v === "object" &&
	(v as Element).type === "element" &&
	typeof (v as Element).typeName === "string";
const arr = (v: unknown): unknown[] => (Array.isArray(v) ? v : []);
const text = (v: unknown): string =>
	typeof v === "string"
		? v
		: typeof v === "number" || typeof v === "boolean"
			? String(v)
			: "";
const props = (v: unknown): Record<string, unknown> =>
	isElement(v) ? v.props : {};
/** Plain data objects (table columns, rows) are not elements. */
const record = (v: unknown): Record<string, unknown> =>
	!!v && typeof v === "object" ? (v as Record<string, unknown>) : {};

const isBound = (v: unknown) =>
	!!v && typeof v === "object" && "k" in (v as object);

export interface OpenUIPartInput {
	program: string;
	fallback?: string;
	wait?: boolean;
}

/** Read the tool input without trusting its shape. */
export function readOpenUIPart(input: unknown): OpenUIPartInput | null {
	if (!input || typeof input !== "object") return null;
	const value = input as Record<string, unknown>;
	if (typeof value.program !== "string" || !value.program.trim()) return null;
	return {
		program: value.program,
		fallback: typeof value.fallback === "string" ? value.fallback : undefined,
		wait: value.wait === true,
	};
}

/** Positional paths, computed outside JSX, are the stable identity of list items. */
const slots = <T,>(owner: string, list: readonly T[]) =>
	list.map((value, index) => ({ key: `${owner}:${index}`, value }));

const SILENT = new Set([
	"Series",
	"Slice",
	"Point",
	"SelectItem",
	"RadioItem",
	"ChipItem",
	"OptionCard",
	"Col",
	"FormControl",
]);

function toneOf(v: unknown): string {
	return ["info", "warning", "error", "success", "neutral"].includes(String(v))
		? String(v)
		: "info";
}

/** A stable, de-emphasised line for components this host does not draw yet. */
function NotRendered({ name }: { name: string }) {
	return (
		<div className="wb-openui__unsupported">
			{name} is not rendered in the web chat yet.
		</div>
	);
}

function Charts({ node }: { node: Element }) {
	const p = node.props;
	const labels = arr(p.labels).map(text);
	const series = arr(p.series).filter(isElement);
	if (node.typeName === "BarChart" || node.typeName === "HorizontalBarChart") {
		const values = series.map((s) =>
			arr(props(s).values).map((v) => Number(v) || 0),
		);
		const totals = labels.map((_, i) =>
			values.reduce((a, v) => a + (v[i] ?? 0), 0),
		);
		const max = Math.max(1, ...totals);
		return (
			<div className="wb-openui__chart">
				{slots("bar-label", labels).map(({ key, value: label }, i) => (
					<div className="wb-openui__bar" key={key}>
						<span className="wb-openui__bar-label">{label}</span>
						<span className="wb-openui__bar-track">
							{slots("bar-series", values).map(
								({ key: seriesKey, value: v }, k) => (
									<span
										className="wb-openui__bar-fill"
										key={seriesKey}
										style={{ width: `${((v[i] ?? 0) / max) * 100}%` }}
										data-series={k}
									/>
								),
							)}
						</span>
						<span className="wb-openui__bar-value">{totals[i]}</span>
					</div>
				))}
				<div className="wb-openui__legend">
					{slots("bar-legend", series).map(
						({ key: legendKey, value: s }, k) => (
							<span key={legendKey} data-series={k}>
								{text(props(s).category)}
							</span>
						),
					)}
				</div>
			</div>
		);
	}
	// Line/Area: one sparkline per series over the shared labels.
	const values = series.map((s) =>
		arr(props(s).values).map((v) => Number(v) || 0),
	);
	const max = Math.max(1, ...values.flat());
	return (
		<div className="wb-openui__chart">
			{slots("spark", values).map(({ key, value: v }, k) => (
				<svg
					key={key}
					viewBox="0 0 100 24"
					preserveAspectRatio="none"
					className="wb-openui__spark"
					data-series={k}
					role="img"
					aria-label={text(props(series[k] ?? {}).category) || "series"}
				>
					<title>{text(props(series[k] ?? {}).category) || "series"}</title>
					<polyline
						points={v
							.map(
								(y, i) =>
									`${(i / Math.max(1, v.length - 1)) * 100},${24 - (y / max) * 22}`,
							)
							.join(" ")}
						fill="none"
						strokeWidth={1.4}
						vectorEffect="non-scaling-stroke"
					/>
				</svg>
			))}
			<div className="wb-openui__legend">
				{slots("spark-legend", series).map(
					({ key: legendKey, value: s }, k) => (
						<span key={legendKey} data-series={k}>
							{text(props(s).category)}
						</span>
					),
				)}
			</div>
			<div className="wb-openui__axis">{labels.join("  ·  ")}</div>
		</div>
	);
}

function Node({ node }: { node: unknown }) {
	if (Array.isArray(node)) {
		return (
			<>
				{slots("node", node).map(({ key, value }) => (
					<Node key={key} node={value} />
				))}
			</>
		);
	}
	if (!isElement(node)) return null;
	const p = node.props;
	const children = (v: unknown) => <Node node={v} />;
	switch (node.typeName) {
		case "Card":
			return (
				<section className="wb-openui__card">
					{p.title ? (
						<div className="wb-openui__card-title">{text(p.title)}</div>
					) : null}
					{children(p.children)}
				</section>
			);
		case "Stack":
			return (
				<div
					className={`wb-openui__stack${p.direction === "row" ? " wb-openui__stack--row" : ""}`}
				>
					{children(p.children)}
				</div>
			);
		case "CardHeader":
			return (
				<header className="wb-openui__header">
					<h4>{text(p.title)}</h4>
					{p.subtitle ? <p>{text(p.subtitle)}</p> : null}
				</header>
			);
		case "InlineHeader":
			return (
				<header className="wb-openui__header">
					<h4>{text(p.heading)}</h4>
					{p.description ? <p>{text(p.description)}</p> : null}
				</header>
			);
		case "TextContent":
			return (
				<p className="wb-openui__text" data-size={text(p.size) || undefined}>
					{text(p.text)}
				</p>
			);
		case "MarkDownRenderer":
			return (
				<MarkdownRenderer
					content={text(p.textMarkdown)}
					className="chat-prose"
				/>
			);
		case "Callout":
		case "TextCallout":
			return (
				<div className="wb-openui__callout" data-tone={toneOf(p.variant)}>
					{p.title ? <b>{text(p.title)}</b> : null}
					{p.description ? <span>{text(p.description)}</span> : null}
				</div>
			);
		case "CodeBlock":
			return (
				<pre className="wb-openui__code">
					<code>{text(p.codeString)}</code>
				</pre>
			);
		case "Separator":
			return <hr className="wb-openui__rule" />;
		case "Tabs":
			return <Tabs node={node} />;
		case "Steps":
			return (
				<ol className="wb-openui__steps">
					{slots("step", arr(p.items)).map(({ key, value: item }) => (
						<li key={key}>
							<b>{text(props(item).title)}</b>
							<span>{text(props(item).details)}</span>
						</li>
					))}
				</ol>
			);
		case "Accordion":
			return (
				<div className="wb-openui__accordion">
					{slots("acc", arr(p.items)).map(({ key, value: item }) => (
						<details key={key}>
							<summary>{text(props(item).trigger)}</summary>
							{children(props(item).content)}
						</details>
					))}
				</div>
			);
		case "Table":
			return <DataTable node={node} />;
		case "EditableTable": {
			// The web chat is read-only: show the grid, not an editor whose save
			// would have no authority here. Editing happens where the tool runs.
			const columns = arr(p.columns).map(record);
			const rows = arr(p.data).map((row) =>
				arr((row as { values?: unknown })?.values),
			);
			if (!columns.length) return null;
			return (
				<table className="wb-openui__table">
					<thead>
						<tr>
							{slots("eth", columns).map(({ key, value: c }) => (
								<th key={key} data-numeric={c.type === "number" || undefined}>
									{text(c.header ?? c.key)}
								</th>
							))}
						</tr>
					</thead>
					<tbody>
						{slots("etr", rows).map(({ key, value: row }) => (
							<tr key={key}>
								{slots("etd", row).map(({ key: cellKey, value: cell }, i) => (
									<td
										key={cellKey}
										data-numeric={columns[i]?.type === "number" || undefined}
									>
										{text(cell)}
									</td>
								))}
							</tr>
						))}
					</tbody>
				</table>
			);
		}
		case "TagBlock":
			return (
				<div className="wb-openui__tags">
					{slots("tag", arr(p.tags)).map(({ key, value: tag }) => (
						<span key={key}>{text(tag)}</span>
					))}
				</div>
			);
		case "Tag":
			return <span className="wb-openui__tags">{text(p.text)}</span>;
		case "EntityList": {
			const row = (r: unknown) => {
				const o = (r ?? {}) as Record<string, unknown>;
				return (
					<div className="wb-openui__kv" key={text(o.left)}>
						<span>{text(o.left)}</span>
						<b>{text(o.right)}</b>
					</div>
				);
			};
			return (
				<div className="wb-openui__kvlist">
					{p.header ? row(p.header) : null}
					{arr(p.rows).map((r) => row(r))}
					{p.footer ? row(p.footer) : null}
				</div>
			);
		}
		case "ListBlock":
			return (
				<ul className="wb-openui__list">
					{slots("li", arr(p.items)).map(({ key, value: item }) => (
						<li key={key}>
							<b>{text(props(item).title)}</b>
							{props(item).subtitle ? (
								<span>{text(props(item).subtitle)}</span>
							) : null}
						</li>
					))}
				</ul>
			);
		case "OverviewCardBlock":
			return (
				<div className="wb-openui__kpis">
					{slots("kpi", arr(p.items)).map(({ key, value: item }) => {
						const top = props(props(item).top);
						const bottom = props(props(item).bottom);
						return (
							<div className="wb-openui__kpi" key={key}>
								<span>{text(top.value ?? top.title)}</span>
								<b>{text(bottom.value)}</b>
								<small>{text(bottom.subtext)}</small>
							</div>
						);
					})}
				</div>
			);
		case "Buttons":
			return (
				<div className="wb-openui__buttons">
					{slots("button", arr(p.buttons)).map(({ key, value: button }) => (
						<button
							key={key}
							type="button"
							disabled
							title="Answers are given in the terminal"
						>
							{text(props(button).label)}
						</button>
					))}
				</div>
			);
		case "Button":
			return (
				<div className="wb-openui__buttons">
					<button
						type="button"
						disabled
						title="Answers are given in the terminal"
					>
						{text(p.label)}
					</button>
				</div>
			);
		case "Form":
			return (
				<div className="wb-openui__form">
					{p.name ? (
						<div className="wb-openui__form-title">{text(p.name)}</div>
					) : null}
					{children(p.fields)}
					{children(p.buttons)}
				</div>
			);
		case "FormControl":
			return (
				<div className="wb-openui__field">
					<span>{text(p.label)}</span>
					{children(p.input)}
					{p.hint ? <small>{text(p.hint)}</small> : null}
				</div>
			);
		case "Input":
		case "TextArea":
		case "DatePicker":
			return (
				<span className="wb-openui__input">{text(p.placeholder) || "…"}</span>
			);
		case "Select":
		case "RadioGroup":
		case "OptionCards":
		case "Chips":
		case "CheckBoxGroup":
			return (
				<span className="wb-openui__options">
					{arr(p.items)
						.map((item) => text(props(item).label ?? props(item).title))
						.filter(Boolean)
						.join(" · ")}
				</span>
			);
		case "Image":
		case "ImageBlock":
			return text(p.src) ? (
				<img className="wb-openui__image" src={text(p.src)} alt={text(p.alt)} />
			) : null;
		case "BarChart":
		case "LineChart":
		case "AreaChart":
		case "HorizontalBarChart":
		case "RadarChart":
			return <Charts node={node} />;
		case "PieChart":
		case "RadialChart":
		case "SingleStackedBarChart": {
			const labels = arr(p.labels).map(text);
			const values = arr(p.values).map((v) => Number(v) || 0);
			const total = Math.max(
				1,
				values.reduce((a, b) => a + b, 0),
			);
			return (
				<div className="wb-openui__share">
					<span className="wb-openui__share-bar">
						{slots("share", values).map(({ key, value: v }, i) => (
							<span
								key={key}
								data-series={i}
								style={{ width: `${(v / total) * 100}%` }}
							/>
						))}
					</span>
					<div className="wb-openui__legend">
						{slots("share-label", labels).map(({ key, value: l }, i) => (
							<span key={key} data-series={i}>
								{l} {Math.round(((values[i] ?? 0) / total) * 100)}%
							</span>
						))}
					</div>
				</div>
			);
		}
		case "MetricIndicatorInline":
			return (
				<span className="wb-openui__metric">
					{text(p.value)} {text(p.subtext)}
				</span>
			);
		default:
			if (SILENT.has(node.typeName)) return null;
			return <NotRendered name={node.typeName} />;
	}
}

function Tabs({ node }: { node: Element }) {
	const items = arr(node.props.items);
	if (!items.length) return null;
	return (
		<div className="wb-openui__tabs">
			{slots("tab", items).map(({ key, value: item }, i) => (
				<details key={key} open={i === 0}>
					<summary>{text(props(item).trigger)}</summary>
					<Node node={props(item).content} />
				</details>
			))}
		</div>
	);
}

function DataTable({ node }: { node: Element }) {
	const columns = arr(node.props.columns).map(record);
	if (!columns.length) return null;
	const rows = Math.max(0, ...columns.map((c) => arr(c.data).length));
	return (
		<table className="wb-openui__table">
			<thead>
				<tr>
					{slots("th", columns).map(({ key, value: c }) => (
						<th key={key} data-numeric={c.type === "number" || undefined}>
							{text(c.label)}
						</th>
					))}
				</tr>
			</thead>
			<tbody>
				{slots("tr", Array.from({ length: rows })).map(({ key: rowKey }, r) => (
					<tr key={rowKey}>
						{slots("td", columns).map(({ key, value: c }) => (
							<td key={key} data-numeric={c.type === "number" || undefined}>
								{text(arr(c.data)[r])}
							</td>
						))}
					</tr>
				))}
			</tbody>
		</table>
	);
}

export function OpenUIPart({
	input,
	answered,
}: { input: unknown; answered?: unknown }) {
	const { t } = useTranslation();
	const part = readOpenUIPart(input);
	const parsed = useMemo(() => {
		if (!part) return null;
		try {
			const result = createOqtoParser().parse(part.program);
			const errors = result.meta.errors.map(
				(e: { message?: string }) => e.message ?? "",
			);
			return {
				root: result.root as unknown,
				errors,
				incomplete: result.meta.incomplete,
			};
		} catch {
			return { root: null, errors: [], incomplete: true };
		}
	}, [part]);

	if (!part) return null;
	if (!parsed?.root) {
		return (
			<div className="wb-openui wb-openui--fallback">
				{part.fallback ? <p>{part.fallback}</p> : null}
				<div className="wb-openui__unsupported">
					{t(
						"oqtoUi.openui.invalid",
						"This UI could not be rendered; showing its text fallback.",
					)}
				</div>
			</div>
		);
	}
	return (
		<div className="wb-openui" data-openui-profile="oqto">
			<Node node={parsed.root} />
			{answered !== undefined ? (
				<div className="wb-openui__answered">
					{t("oqtoUi.openui.answered", "Answered in the terminal")}:{" "}
					{JSON.stringify(answered)}
				</div>
			) : part.wait ? (
				<div className="wb-openui__hint">
					{t(
						"oqtoUi.openui.answerInTerminal",
						"This UI answers in the session that asked (the terminal).",
					)}
				</div>
			) : null}
			{parsed.errors.length ? (
				<div className="wb-openui__unsupported">
					{parsed.errors.length} component warning(s)
				</div>
			) : null}
		</div>
	);
}
