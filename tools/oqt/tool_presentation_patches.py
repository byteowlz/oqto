"""Pinned cdaf6c227d4718a72b59ccd02152c9614df44cfc derivative tool UI.

No classic extension contract is exported. Exact-once anchors are the same
fail-closed mechanism as source_patches.py; prepare.py owns full archive pinning.
Only the native component renders controls/theme and owns expansion.
"""
from pathlib import Path

PIN = "cdaf6c227d4718a72b59ccd02152c9614df44cfc"
SERVICE_PATH = "packages/coding-agent/src/experimental/services/tool-presentation.ts"
SERVICE = r'''import { defineService } from "@earendil-works/chord";

/** Oqt pinned derivative v1: semantic display only, never an execution/action API.
 * Untrusted strings are escaped by the native host before applying theme tokens.
 * Raw arguments/results are shown only via the component's standard expansion.
 */
export interface ToolPresentationInput {
 readonly args: unknown;
 readonly phase: "preparing" | "pending" | "complete";
 readonly result?: { readonly isError: boolean; readonly details?: unknown };
}
export interface ToolPresentationSummary {
 readonly title: string;
 readonly lines: readonly string[];
}
export type ToolPresentationRenderer = (input: ToolPresentationInput) => ToolPresentationSummary;
export interface ToolPresentation {
 register(name: "request_access", renderer: ToolPresentationRenderer): () => void;
 render(name: string, input: ToolPresentationInput): ToolPresentationSummary | undefined;
}
export const ToolPresentation = defineService<ToolPresentation>("oqt.local.tool-presentation.v1", { local: true });

/** No plugin can smuggle terminal escapes, CR, bidi overrides or hidden controls.
 * Escape rather than strip so paths remain distinguishable for operator review.
 */
export function escapeToolPresentation(value: string): string {
 return Array.from(value, (character) => {
  const code = character.codePointAt(0) ?? 0;
  return code < 32 || (code >= 127 && code <= 159) || /[\p{Cf}\p{Zl}\p{Zp}]/u.test(character)
   ? `\\u${code.toString(16).padStart(4, "0")}` : character;
 }).join("");
}
export function createToolPresentation(): ToolPresentation {
 const renderers = new Map<string, ToolPresentationRenderer>();
 return {
  register(name, renderer) {
   if (name !== "request_access") throw new Error("Unsupported semantic tool presentation");
   // Reload installs candidates before disposing old facets. Disposers are identity-bound.
   renderers.set(name, renderer);
   return () => { if (renderers.get(name) === renderer) renderers.delete(name); };
  },
  render(name, input) { return renderers.get(name)?.(input); },
 };
}
'''

PATCHES = [
    ("packages/coding-agent/src/experimental/plugin.ts",
     'export { type PresentationSelectItem, PresentationUI } from "./services/presentation-ui.ts";',
     'export { type PresentationSelectItem, PresentationUI } from "./services/presentation-ui.ts";\nexport { ToolPresentation, type ToolPresentationInput, type ToolPresentationSummary, type ToolPresentationRenderer } from "./services/tool-presentation.ts";'),
    ("packages/coding-agent/src/experimental/client-tui.ts",
     'import { PresentationUI } from "./services/presentation-ui.ts";',
     'import { PresentationUI } from "./services/presentation-ui.ts";\nimport { ToolPresentation, createToolPresentation } from "./services/tool-presentation.ts";'),
    ("packages/coding-agent/src/experimental/client-tui.ts",
     '\t#chatView: ExperimentalChatView | undefined;',
     '\t#chatView: ExperimentalChatView | undefined;\n\treadonly #toolPresentation = createToolPresentation();'),
    ("packages/coding-agent/src/experimental/client-tui.ts",
     '\t\t\t\tenv.provide(PresentationUI, {',
     '\t\t\t\tenv.provide(ToolPresentation, this.#toolPresentation);\n\t\t\t\tenv.provide(PresentationUI, {'),
    ("packages/coding-agent/src/experimental/client-tui.ts",
     'new ExperimentalChatView(this.#ui, process.cwd())',
     'new ExperimentalChatView(this.#ui, process.cwd(), this.#toolPresentation)'),
    ("packages/coding-agent/src/experimental/client-tui-chat.ts",
     'import { theme } from "../modes/interactive/theme/theme.ts";',
     'import { theme } from "../modes/interactive/theme/theme.ts";\nimport type { ToolPresentation } from "./services/tool-presentation.ts";'),
    ("packages/coding-agent/src/experimental/client-tui-chat.ts",
     '\tconstructor(ui: TUI, cwd: string) {',
     '\treadonly presentation?: ToolPresentation;\n\tconstructor(ui: TUI, cwd: string, presentation?: ToolPresentation) {\n\t\tthis.presentation = presentation;'),
    ("packages/coding-agent/src/experimental/client-tui-chat.ts",
     '\t\t\targs ?? {},\n\t\t\t{},',
     '\t\t\targs ?? {},\n\t\t\t{ presentation: toolName === "request_access" && this.presentation ? (input) => this.presentation?.render(toolName, input) : undefined },'),
    ("packages/coding-agent/src/modes/interactive/components/tool-execution.ts",
     'const FALLBACK_PREVIEW_LINES = 10;',
     'import { escapeToolPresentation, type ToolPresentationInput, type ToolPresentationSummary } from "../../../experimental/services/tool-presentation.ts";\n\nconst FALLBACK_PREVIEW_LINES = 10;'),
    ("packages/coding-agent/src/modes/interactive/components/tool-execution.ts",
     'export interface ToolExecutionOptions {',
     'export interface ToolExecutionOptions {\n\tpresentation?: (input: ToolPresentationInput) => ToolPresentationSummary | undefined;'),
    ("packages/coding-agent/src/modes/interactive/components/tool-execution.ts",
     '\tprivate hideComponent = false;',
     '\tprivate hideComponent = false;\n\tprivate presentation?: ToolExecutionOptions["presentation"];'),
    ("packages/coding-agent/src/modes/interactive/components/tool-execution.ts",
     '\t\tthis.toolDefinition = toolDefinition;',
     '\t\tthis.toolDefinition = toolDefinition;\n\t\tthis.presentation = options.presentation;'),
    ("packages/coding-agent/src/modes/interactive/components/tool-execution.ts",
     '\t\tif (this.hasRendererDefinition()) {\n\t\t\tthis.addChild',
     '\t\tif (this.presentation || this.hasRendererDefinition()) {\n\t\t\tthis.addChild'),
    ("packages/coding-agent/src/modes/interactive/components/tool-execution.ts",
     '\t\tif (this.hasRendererDefinition()) {\n\t\t\tconst renderContainer',
     '''\t\tif (this.presentation) {
            this.contentBox.setBgFn(bgFn);
            this.contentBox.setPaddingX(this.outputPad);
            this.contentBox.clear();
            let summary: ToolPresentationSummary | undefined;
            try {
                summary = this.presentation({ args: this.args,
                    phase: this.result && !this.isPartial ? "complete" : this.executionStarted ? "pending" : "preparing",
                    result: this.result ? { isError: this.result.isError, details: this.result.details } : undefined });
            } catch { /* A bad presentation never changes execution or exposes raw fallback. */ }
            const safe = escapeToolPresentation;
            const title = typeof summary?.title === "string" ? summary.title : "Access request";
            const lines = Array.isArray(summary?.lines) ? summary.lines.filter((line): line is string => typeof line === "string") : ["Presentation unavailable; expand for details"];
            let text = theme.fg("toolTitle", safe(title)) + "\\n" + lines.map((line) => theme.fg("toolOutput", safe(line))).join("\\n");
            if (this.expanded) {
                let raw: string;
                try { raw = JSON.stringify({ arguments: this.args, result: this.result }, null, 2); }
                catch { raw = "Raw details unavailable"; }
                // JSON indentation/newlines are host-produced; escape all value controls.
                text += "\\n" + theme.fg("muted", "Raw details") + "\\n" + raw.split("\\n").map((line) => safe(line)).join("\\n");
            } else {
                text += "\\n" + theme.fg("muted", this.result ? "Click result for raw details" : "Raw details available when the request settles");
            }
            this.contentBox.addChild(this.createResultRegion(new Text(text, 0, 0)));
            hasContent = true;
        } else if (this.hasRendererDefinition()) {
\t\t\tconst renderContainer'''),
]


def apply(snapshot: Path) -> list[Path]:
    """Apply exact pinned anchors transactionally in memory, then write derivatives.

    The caller MUST archive PIN (prepare.py already verifies the full revision).
    Calling twice or against a changed anchor fails closed, without partial edits.
    """
    snapshot = Path(snapshot)
    service = snapshot / SERVICE_PATH
    if service.exists():
        raise SystemExit("Upstream changed; refusing existing ToolPresentation service")
    texts: dict[Path, str] = {}
    for file, old, new in PATCHES:
        path = snapshot / file
        text = texts.get(path, path.read_text())
        if text.count(old) != 1:
            raise SystemExit(f"Upstream changed; refusing unreviewed tool presentation patch {file}: {old[:90]!r}")
        texts[path] = text.replace(old, new)
    for path, text in texts.items():
        path.write_text(text)
    service.write_text(SERVICE)
    return [*texts, service]
