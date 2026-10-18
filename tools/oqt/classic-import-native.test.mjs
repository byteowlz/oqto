import assert from "node:assert/strict";
import { copyFile, mkdtemp, rm, symlink, writeFile } from "node:fs/promises";
import { execFileSync } from "node:child_process";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { test } from "node:test";

const source = process.env.OQT_PI_SOURCE;
const pin = "cdaf6c227d4718a72b59ccd02152c9614df44cfc";
assert.equal(execFileSync("git", ["-C", source, "rev-parse", "HEAD"], { encoding: "utf8" }).trim(), pin);
const modules = await mkdtemp(join(tmpdir(), "oqt-classic-native-"));
await symlink(join(source, "node_modules"), join(modules, "node_modules"));
await writeFile(join(modules, "loader.mjs"), "export const load = name => import(name);");
const { load } = await import(pathToFileURL(join(modules, "loader.mjs")));
for (const name of ["classic-import.ts", "classic-import-worker.ts"]) await copyFile(new URL(name, import.meta.url), join(modules, name));
const { classicSnapshot } = await import(pathToFileURL(join(modules, "classic-import.ts")));
const { importClassicSnapshot, ClassicImportDoc } = await import(pathToFileURL(join(modules, "classic-import-worker.ts")));
const { BACKGROUND_CONTEXT: context } = await load("@earendil-works/chord/context");
const { Harness, createRegistry } = await load("@earendil-works/pi-durable");
const { createModels, fauxProvider } = await load("@earendil-works/pi-ai");
const { openNodeSqliteStorage } = await load("@earendil-works/pi-durable/storage/sqlite/node");
const { buildSessionContext, convertToLlm } = await load("@earendil-works/pi-coding-agent");
const header = { type: "session", version: 3, id: "9267f005-48a0-4c42-920e-3277aa2c6070", cwd: "/inert", timestamp: "2026-10-17T00:00:00Z" };
const row = (id, parentId, type, extra) => ({ id, parentId, type, timestamp: header.timestamp, ...extra });
const msg = (id, parentId) => row(id, parentId, "message", { message: { role: "user", content: id, timestamp: 1 } });
const rows = [msg("a", null), msg("b", "a"), row("opaque", "b", "custom", { customType: "inert", data: { secret: [1, 2] } }), row("c1", "opaque", "compaction", { summary: "first", firstKeptEntryId: "b", tokensBefore: 100 }), row("custom", "c1", "custom_message", { customType: "note", content: "custom", display: false }), row("edit", "custom", "context_edit", { targetId: "custom", replacement: { content: "edited" } }), row("c2", "edit", "compaction", { summary: "second", firstKeptEntryId: "c1", tokensBefore: 200, systemMessage: { role: "system", content: "preserved", timestamp: 2 } }), row("branch", "c2", "branch_summary", { summary: "other branch", fromId: "b" }), msg("sibling", "a")];
const snap = rs => classicSnapshot([header, ...rs].map(JSON.stringify).join("\n") + "\n", "fixture");

test("SQLite archive/map and selected contexts equal pinned classic, survive reopen, never infer", async () => {
 const faux = fauxProvider(); const models = createModels(); models.setProvider(faux.provider);
 let harness;
 try {
  for (const leaf of ["b", "edit", "c2", "branch", "sibling", null]) {
   const database = join(modules, `case-${leaf}.sqlite`);
   const open = async () => Harness.open(await openNodeSqliteStorage(database), { models, registry: createRegistry() }, context);
   harness = await open(); let conversation = await harness.root(context);
   const snapshot = snap(rows);
   const result = await importClassicSnapshot({ harness, conversation, snapshot, leaf }, context);
   assert.equal(result.mappings.length, rows.length);
   const expected = buildSessionContext(rows, leaf);
   assert.deepEqual((await conversation.context(context)).messages, convertToLlm(expected.messages));
   const state = await harness.snapshot(ClassicImportDoc, conversation.id, context);
   assert.deepEqual(state.snapshot, snapshot);
   let cursor; const entries = [];
   do { const page = await conversation.entries({ order: "ascending" }, 2, cursor, context); entries.push(...page.items); cursor = page.cursor; } while (cursor);
   assert.equal(entries.length, rows.length + 1);
   for (let i = 0; i < rows.length; i++) { assert.deepEqual(entries[i].data.record, rows[i]); assert.equal(result.mappings[i].entryId, entries[i].id); }
   await harness.close(context); harness = await open(); conversation = await harness.root(context);
   assert.equal((await importClassicSnapshot({ harness, conversation, snapshot, leaf }, context)).kind, "unchanged");
   assert.deepEqual((await conversation.context(context)).messages, convertToLlm(expected.messages));
   const appended = snap([...rows, msg("next", "sibling")]);
   await importClassicSnapshot({ harness, conversation, snapshot: appended }, context);
   await conversation.commit(tx => tx.appendEntry(conversation.id, { kind: "local", data: {} }), context);
   await assert.rejects(importClassicSnapshot({ harness, conversation, snapshot: appended }, context), /diverged/);
   await harness.close(context); harness = undefined;
  }
  assert.equal(faux.state.callCount, 0);
 } finally { if (harness) await harness.close(context); await rm(modules, { recursive: true, force: true }); }
});
