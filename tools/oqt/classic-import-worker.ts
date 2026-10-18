import type { Context } from "@earendil-works/chord";
import { buildSessionContext, convertToLlm, type SessionEntry } from "@earendil-works/pi-coding-agent";
import { AgentDoc, defineDoc, type Conversation, type Harness, type EntryId } from "@earendil-works/pi-durable";
import { classicSnapshot, type ClassicSnapshot, planClassicUpdate } from "./classic-import.ts";

export type ClassicImportSeed = { source: string; sourceHash: string; origin: string; leafId?: string | null };
/** Wire payload is raw immutable JSONL, SHA-256 of exact UTF-8 bytes, explicit namespace. */
export function validateClassicImportSeed(seed: ClassicImportSeed): ClassicSnapshot {
 const snapshot = classicSnapshot(seed.source, seed.origin);
 if (snapshot.sourceDigest !== seed.sourceHash) throw new Error("Classic snapshot hash mismatch");
 return snapshot;
}
export async function applyClassicImport(harness: Harness, seed: ClassicImportSeed, context: Context) {
 const snapshot = validateClassicImportSeed(seed);
 const conversation = await harness.root(context);
 return importClassicSnapshot({ harness, conversation, snapshot, leaf: seed.leafId }, context);
}

export const CLASSIC_PROJECTION_VERSION = "cdaf6c227d4718a72b59ccd02152c9614df44cfc:1";
type Mapping = { nativeId: string; conversationId: string; entryId: EntryId };
type ImportState = {
 version: string; snapshot: ClassicSnapshot | null; leaf: string | null;
 mappings: Mapping[]; watermark: EntryId | null; agent: string;
};
export const ClassicImportDoc = defineDoc<ImportState>({
 kind: "oq.classic-import.v1", version: 1, scope: "conversation",
 history: "latest", fork: "initial",
 initial: () => ({ version: CLASSIC_PROJECTION_VERSION, snapshot: null, leaf: null, mappings: [], watermark: null, agent: "" }),
});
const supported = new Set(["message", "model_change", "thinking_level_change", "usage", "compaction", "branch_summary", "custom", "custom_message", "label", "session_info", "context_edit"]);

/** Validated snapshot only. No source IO, catalog publication, scheduler, submissions,
 * extension loading or authority import. Host MUST hold its exclusive non-admitting
 * worker bootstrap gate throughout this call and must not resume archival workers.
 * Every source record has an inert native entry; the selected context is a separate
 * versioned materialization, never a claim that the native branch graph is linear.
 */
export async function importClassicSnapshot(
 options: { harness: Harness; conversation: Conversation; snapshot: ClassicSnapshot; leaf?: string | null },
 context: Context,
) {
 const { harness, conversation, snapshot } = options;
 const leaf = options.leaf === undefined ? snapshot.activeLeaf : options.leaf;
 if (leaf !== null && !Object.hasOwn(snapshot.entryDigests, leaf)) throw new Error("Unknown selected classic leaf");
 const byId = new Map(snapshot.records.map(r => [r.id, r]));
 for (const r of snapshot.records) {
  if (!supported.has(String(r.type))) throw new Error(`Unsupported classic semantics: ${r.type}`);
  if (!Number.isFinite(Date.parse(String(r.timestamp)))) throw new Error("Invalid classic timestamp");
  if (r.type === "message") {
   const m = r.message;
   if (!m || typeof m !== "object" || Array.isArray(m) || !["system", "user", "assistant", "toolResult"].includes(String(m.role))) throw new Error("Unsupported classic message role");
  }
  if (r.type === "context_edit" && (!byId.has(r.targetId) || !(r.replacement === null || (typeof r.replacement === "object" && r.replacement !== null && !Array.isArray(r.replacement) && Object.hasOwn(r.replacement, "content"))))) throw new Error("Unsupported classic context edit");
  if (r.type === "compaction") {
   if (typeof r.summary !== "string" || typeof r.tokensBefore !== "number") throw new Error("Invalid classic compaction");
   // The canonical builder treats absent kept IDs as no kept range. Preserve that.
  }
 }
 // Only the pinned pure builder is used, never SessionManager or mutable loaders.
 const projected = buildSessionContext(snapshot.records as unknown as SessionEntry[], leaf);
 const model = convertToLlm(projected.messages);
 return harness.commit(async tx => {
  const state = await tx.doc(ClassicImportDoc, conversation.id);
  const newest = (await tx.scanEntries({ conversationId: conversation.id }, 1)).items[0];
  const agent = JSON.stringify(await tx.doc(AgentDoc, conversation.id));
  const tasks = await tx.scanTasks({ conversationId: conversation.id }, 1000);
  if (tasks.items.some(t => !["completed", "failed", "aborted"].includes(t.state.status))) throw new Error("Classic import requires an unscheduled idle worker");
  if (state.snapshot) {
   if (state.version !== CLASSIC_PROJECTION_VERSION) throw new Error("Classic projection version conflict");
   if (newest?.id !== state.watermark || agent !== state.agent) throw new Error("Classic destination diverged");
   const update = planClassicUpdate(state.snapshot, snapshot, false);
   if (update.kind === "conflict") throw new Error(update.reason);
   // Byte-prefix evidence is stricter than semantic identity: no silent rewrite.
   if (!snapshot.sourceText.startsWith(state.snapshot.sourceText)) throw new Error("Classic source rewritten");
   if (update.kind === "unchanged" && leaf === state.leaf) return { kind: "unchanged" as const, conversationId: conversation.id, watermark: state.watermark, mappings: structuredClone(state.mappings) };
   if (update.kind === "branch") throw new Error(`Classic source branch requires a new target: ${update.reason}`);
  } else if (newest) throw new Error("Classic import requires an empty target");
  const known = new Set(state.mappings.map(m => m.nativeId));
  for (const record of snapshot.records) {
   if (known.has(String(record.id))) continue;
   const entry = await tx.appendEntry(conversation.id, { kind: "oq.classic-archive.v1", data: { sourceId: snapshot.sourceId, record } });
   state.mappings.push({ nativeId: String(record.id), conversationId: conversation.id, entryId: entry.id });
  }
  const entry = await tx.appendEntry(conversation.id, {
   kind: "oq.classic-projection.v1", head: "self", model,
   data: { sourceId: snapshot.sourceId, sourceDigest: snapshot.sourceDigest, leaf, version: CLASSIC_PROJECTION_VERSION, recordedSettings: { model: projected.model, thinkingLevel: projected.thinkingLevel }, classicContext: JSON.parse(JSON.stringify(projected)) },
  });
  state.snapshot = JSON.parse(JSON.stringify(snapshot));
  state.version = CLASSIC_PROJECTION_VERSION;
  state.leaf = leaf;
  state.watermark = entry.id;
  state.agent = agent;
  return { kind: "imported" as const, conversationId: conversation.id, watermark: entry.id, mappings: structuredClone(state.mappings) };
 }, context);
}
