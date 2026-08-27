/* OqtoUI corner-mode concept gallery v4 (oqto-m5sp). Fixture-only design probe.
   Model: workspace(tenant) -> workdir -> session. Tap = do. Hold = choose.
   v4: navigator anatomy from the ribbon/list mockups, herdr attention
   semantics (done = unseen completion), context-aware status line. */
"use strict";

/* ---------------- fixtures ---------------- */

const OQTO_LOGO = `<svg class="ws-face" viewBox="0 0 32 32" aria-hidden="true"><circle cx="16" cy="13" r="7" fill="#f2f5f3"/><path d="M9 13c-2 7-4 9-6 10 3 1 5 0 6-1 0 2-1 4-3 5 3 1 5-1 6-3 1 2 1 4 0 6 2-1 4-3 4-6 1 3 3 5 6 5-2-2-2-4-2-6 2 2 4 2 6 1-2-1-3-3-3-5 2 1 4 1 6-1-3-1-5-3-6-10z" fill="#f2f5f3"/><rect x="11" y="10" width="10" height="4" fill="#0f1412"/></svg>`;
const SLDR_LOGO = `<svg class="ws-face" viewBox="0 0 32 32" aria-hidden="true"><rect x="4" y="7" width="24" height="15" fill="none" stroke="#d4c275" stroke-width="2.4"/><path d="M9 25h14" stroke="#d4c275" stroke-width="2.4"/><path d="M8 17l5-5 4 3 6-6" fill="none" stroke="#d4c275" stroke-width="2.4"/></svg>`;
const WIKI_LOGO = `<svg class="ws-face" viewBox="0 0 32 32" aria-hidden="true"><rect x="5" y="5" width="9" height="9" fill="none" stroke="#5b8fc9" stroke-width="2"/><rect x="18" y="5" width="9" height="9" fill="none" stroke="#5b8fc9" stroke-width="2"/><rect x="5" y="18" width="9" height="9" fill="none" stroke="#5b8fc9" stroke-width="2"/><path d="M18 22h9M22.5 18v9" stroke="#5b8fc9" stroke-width="2"/></svg>`;

const TENANTS = [
  {
    id: "byteowlz", name: "byteowlz", logo: OQTO_LOGO,
    workdirs: [
      { id: "oqto", name: "oqto_refactor", path: "~/byteowlz/oqto_refactor", logo: OQTO_LOGO },
      { id: "sldr", name: "sldr", path: "~/byteowlz/sldr", logo: SLDR_LOGO },
      { id: "ctx", name: "ctx", path: "~/byteowlz/ctx" },
      { id: "mmry", name: "mmry", path: "~/byteowlz/mmry" },
      { id: "tmpltr", name: "tmpltr", path: "~/byteowlz/tmpltr" },
      { id: "skills", name: "skillissues", path: "~/byteowlz/skillissues" },
      { id: "hypr", name: "hypr-config", path: "~/byteowlz/hypr" },
    ],
  },
  {
    id: "personal", name: "personal", logo: WIKI_LOGO,
    workdirs: [
      { id: "wiki", name: "wiki", path: "~/wiki", logo: WIKI_LOGO },
      { id: "movies", name: "Movies", path: "~/Movies" },
    ],
  },
  {
    id: "iem", name: "iem-work",
    workdirs: [
      { id: "incubator", name: "genai-incubator", path: "~/work/incubator" },
      { id: "demos", name: "demos", path: "~/work/demos" },
    ],
  },
];

const SESSION_NAMES = [
  ["Corner-mode gallery build", "working"], ["Chat persistence diagnosis", "blocked"],
  ["Prepare v0.5.1", "done"], ["Gallery App vertical slice", "working"],
  ["Speaker view polish", "idle"], ["Audit browser skills", "idle"],
  ["Egress tier rollout", "done"], ["Memory dedup sweep", "working"],
  ["Template JSON pipeline", "idle"], ["Theme contrast pass", "done"],
];

const STATUS_COLOR = { working: "var(--accent)", blocked: "var(--red)", done: "var(--blue)", idle: "var(--faint)" };

function hash(text) {
  let h = 2166136261;
  for (let i = 0; i < text.length; i += 1) { h ^= text.charCodeAt(i); h = Math.imul(h, 16777619); }
  return h >>> 0;
}

const ICON_HUES = ["#3ba77c", "#3ba7a0", "#d4c275", "#5b8fc9", "#9b7fc9", "#d96b69", "#5bc79a"];

function proceduralIcon(name) {
  const h = hash(name);
  const a = ICON_HUES[h % ICON_HUES.length];
  const b = ICON_HUES[(h >> 3) % ICON_HUES.length];
  const parts = [];
  for (let cell = 0; cell < 16; cell += 1) {
    if (!((h >> (cell % 27)) & 1)) continue;
    const x = (cell % 4) * 8; const y = Math.floor(cell / 4) * 8;
    const kind = (h >> ((cell * 2) % 24)) & 3;
    const color = cell % 3 === 0 ? b : a;
    if (kind === 0) parts.push(`<rect x="${x}" y="${y}" width="8" height="8" fill="${color}"/>`);
    else if (kind === 1) parts.push(`<circle cx="${x + 4}" cy="${y + 4}" r="4" fill="${color}"/>`);
    else if (kind === 2) parts.push(`<path d="M${x} ${y + 8} L${x + 4} ${y} L${x + 8} ${y + 8} Z" fill="${color}"/>`);
    else parts.push(`<rect x="${x}" y="${y + 3}" width="8" height="2.6" fill="${color}"/>`);
  }
  if (parts.length < 4) parts.push(`<rect x="8" y="8" width="16" height="16" fill="${a}"/>`);
  return `<svg class="ws-face" viewBox="0 0 32 32" aria-hidden="true"><rect width="32" height="32" fill="rgba(255,255,255,.04)"/>${parts.join("")}</svg>`;
}

function dirIcon(dir) { return dir.logo ?? proceduralIcon(dir.name); }
function tenantAccent(tenant) { return ICON_HUES[hash(tenant.id) % ICON_HUES.length]; }

const READABLE_A = ["oral", "taut", "vile", "calm", "warm", "flat", "keen", "soft"];
const READABLE_B = ["list", "lass", "ones", "moss", "dune", "reed", "fern", "kelp"];
const READABLE_C = ["zero", "rand", "meat", "silk", "iron", "opal", "wolf", "moth"];

function readableId(seed) {
  return `${READABLE_A[seed % 8]}-${READABLE_B[(seed >> 2) % 8]}-${READABLE_C[(seed >> 4) % 8]}`;
}

function seededSessions(tenantId, dirId) {
  const n = 2 + (hash(tenantId + dirId) % 4);
  const out = [];
  for (let i = 0; i < n; i += 1) {
    const seed = hash(tenantId + dirId) + i * 3;
    let [name, status] = SESSION_NAMES[seed % SESSION_NAMES.length];
    /* guarantee one unseen completion per larger workdir so the herdr
       attention semantics are visible in the probe */
    if (i === 1 && n >= 3) status = "done";
    out.push({
      id: `${dirId}-s${i}`,
      name,
      status,
      readable: readableId(seed),
      msgs: 3 + (seed % 40),
      tokens: `${(12 + (seed % 110)).toFixed(0)}.${seed % 9}k`,
      updated: `2026/08/${10 + i} - ${9 + i}:2${i}`,
    });
  }
  return out;
}

/* herdr attention semantics: done = completion while unseen; opening a
   session marks it seen, so its done collapses to idle. */
function seenKey(tenantId, dirId, sessionId) { return `${tenantId}/${dirId}/${sessionId}`; }
function displayStatus(s, tenantId, dirId) {
  if (s.status === "done" && state.seen.has(seenKey(tenantId, dirId, s.id))) return "idle";
  return s.status;
}
function dirAttention(tenantId, d) {
  const statuses = sessionsOf(tenantId, d.id).map((s) => displayStatus(s, tenantId, d.id));
  if (statuses.includes("blocked")) return "blocked";
  if (statuses.includes("done")) return "done";
  if (statuses.includes("working")) return "working";
  return null;
}

const CHAT = [
  ["user", "Let's explore the four-corner interaction model for mobile.", null],
  ["agent", "Corners are the strongest touch targets we have. I'll keep the title and status bars and hang the corner buttons off their ends.", "Reading docs/adr/0037-oqto-ui-portable-rearrangeable-views.md"],
  ["agent", "Hold a corner for the item carousel — release to equip, exactly like the MGS menus.", "Editing corner-mode/app.js"],
  ["user", "Left expansions should keep the dark rail identity, right ones the lighter pane.", null],
  ["agent", "Done. Left sheets inherit the sidebar surface, right strips the files-pane surface, so the desktop split and corner mode share one spatial memory.", null],
];

const TOOLS = [["files", "Files"], ["editor", "Editor"], ["terminal", "Terminal"], ["gallery", "Gallery"], ["diff", "Changes"]];
const MODELS = [["opus-4.7", "deep · slow"], ["sonnet-4.6", "balanced"], ["haiku-4.5", "fast"], ["o4-mini", "cheap"]];
const RADIAL_ACTIONS = [["clip", "Attach", "file · image · bundle"], ["mic", "Voice", "toggle voice mode"], ["cpu", "Model", "switch model"], ["branch", "Fork", "branch the session"]];

/* ---------------- utilities ---------------- */

function escapeHtml(value) {
  return value.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);
}

function fuzzy(query, text) {
  if (!query) return { score: 0, html: escapeHtml(text) };
  const q = query.toLowerCase(); const t = text.toLowerCase();
  let qi = 0; let score = 0; let last = -2; const marks = new Set();
  for (let ti = 0; ti < t.length && qi < q.length; ti += 1) {
    if (t[ti] === q[qi]) { marks.add(ti); score += last === ti - 1 ? 3 : 1; last = ti; qi += 1; }
  }
  if (qi < q.length) return null;
  let html = "";
  for (let i = 0; i < text.length; i += 1) html += marks.has(i) ? `<span class="match">${escapeHtml(text[i])}</span>` : escapeHtml(text[i]);
  return { score, html };
}

const GLYPHS = {
  plus: '<path d="M12 5v14M5 12h14"/>',
  sessions: '<path d="M4 5h16v11H8l-4 4z"/>',
  files: '<path d="M4 6a2 2 0 0 1 2-2h4l2 2h6a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2z"/>',
  editor: '<path d="M8 4l-6 8 6 8"/><path d="M16 4l6 8-6 8"/>',
  terminal: '<path d="M4 5h16v14H4z"/><path d="M7 9l3 3-3 3"/><path d="M12 15h5"/>',
  gallery: '<rect x="3" y="5" width="18" height="14"/><circle cx="9" cy="10" r="1.6"/><path d="M4 17l5-5 4 3 4-4 3 3"/>',
  diff: '<path d="M7 4v16"/><path d="M17 4v10"/><circle cx="17" cy="18" r="2"/><path d="M5 8l2-2 2 2"/>',
  plus: '<path d="M12 5v14M5 12h14"/>',
  send: '<path d="M4 12l16-8-6 16-2.5-6z"/>',
  search: '<circle cx="10" cy="10" r="6"/><path d="M15 15l5 5"/>',
  pin: '<path d="M9 4h6l-1 7 3 3H7l3-3z"/><path d="M12 14v6"/>',
  bolt: '<path d="M13 3L5 14h6l-1 7 8-11h-6z"/>',
  agent: '<rect x="5" y="7" width="14" height="11"/><circle cx="10" cy="12" r="1.4"/><circle cx="14" cy="12" r="1.4"/><path d="M12 4v3"/>',
  grid: '<rect x="4" y="4" width="7" height="7"/><rect x="13" y="4" width="7" height="7"/><rect x="4" y="13" width="7" height="7"/><rect x="13" y="13" width="7" height="7"/>',
  mic: '<rect x="9" y="4" width="6" height="11" rx="3"/><path d="M6 11a6 6 0 0 0 12 0"/><path d="M12 17v3"/>',
  clip: '<path d="M8 12l8-8 4 4-9 9-4-4 8-8"/>',
  branch: '<circle cx="7" cy="6" r="2.4"/><circle cx="7" cy="18" r="2.4"/><circle cx="17" cy="8" r="2.4"/><path d="M7 8.4v7.2"/><path d="M9.2 6.8c4 0 5.4 1.2 5.6 3.6"/>',
  cpu: '<rect x="7" y="7" width="10" height="10"/><path d="M10 3v4M14 3v4M10 17v4M14 17v4M3 10h4M3 14h4M17 10h4M17 14h4"/>',
  swap: '<path d="M7 4v12"/><path d="M4 13l3 3 3-3"/><path d="M17 20V8"/><path d="M14 11l3-3 3 3"/>',
};
function glyph(name) {
  return `<svg class="gl" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${GLYPHS[name]}</svg>`;
}

/* ---------------- state ---------------- */

const state = {
  variant: "A",
  vocabInverted: false,
  tenantId: "byteowlz",
  dirId: "oqto",
  sessionByDir: {},
  tool: "files",
  toolHistory: ["files"],
  mru: [],            // [{tenant,dir,session}] most-recent-first
  open: null,          // "nav" | "tools" | "status" | "models" | "tenants"
  hold: null,          // { corner, kind, items, sel, moved }
  pinned: { left: false, right: false },
  classic: false,
  padOverlay: false,
  query: "",
  input: "",
  chat: CHAT.map((row) => [...row]),
  toast: null,
  model: "opus-4.7",
  preview: null,
  seen: new Set(),
};

function tenant() { return TENANTS.find((t) => t.id === state.tenantId); }
function dir() { return tenant().workdirs.find((d) => d.id === state.dirId); }
function sessionsOf(tenantId, dirId) { return seededSessions(tenantId, dirId); }
function session() {
  const list = sessionsOf(state.tenantId, state.dirId);
  return list.find((s) => s.id === state.sessionByDir[state.dirId]) ?? list[0];
}
function pushMru(tenantId, dirId, sessionId) {
  state.mru = [{ tenant: tenantId, dir: dirId, session: sessionId }, ...state.mru.filter((m) => m.session !== sessionId)].slice(0, 8);
}
function gotoSession(tenantId, dirId, sessionId) {
  state.seen.add(seenKey(tenantId, dirId, sessionId));
  state.tenantId = tenantId; state.dirId = dirId;
  state.sessionByDir[dirId] = sessionId;
  pushMru(tenantId, dirId, sessionId);
}
function previousTarget() { return state.mru[1] ?? null; }

/* seed initial MRU */
pushMru("byteowlz", "oqto", "oqto-s0");

/* ---------------- variants ---------------- */

const VARIANTS = {
  A: { label: "Navigator (mobile)", note: "<b>v4.</b> <b>Tap top-left → navigator</b> (mockup-06 anatomy): + new project heads the ribbon, session rows carry <i>[readable-id]</i> and date | messages | tokens, search sits at the <b>top</b> under the header, and the tenant status line shows visibility + isolation. <b>Hold top-left → quick project switch</b> (mockup-04, release to select); tenant switching stays on the header SWITCH button. Status dots use <b>herdr attention semantics</b>: blue = finished while unseen; opening a session clears it to idle. The bottom status line is <b>context-aware</b> — its segments swap when the navigator or a tool has focus (config-bound in the real product). Other corners unchanged: tap top-right Files / hold wheel; bottom-left previous session / MRU fan; bottom-right send / quarter radial." },
  B: { label: "Right edge strips", note: "Same navigator left; top-right opens a narrow <b>edge strip</b> of tool icons instead of jumping to Files. Compare reachability vs one-tap-Files." },
  C: { label: "Right quadrant sheet", note: "Same navigator left; top-right opens a <b>quarter sheet</b> with the tool list and fuzzy search. Richest, but covers the chat." },
  D: { label: "Big picture + controller", note: "Fullscreen desktop. Navigator pins as the left rail (ribbon + list); files/tools pin right; corner grammar unchanged. Toggle <b>PAD overlay</b> in the panel to see the shoulder mapping (L1/R1/L2/R2, stick-steer, release-on-item commits, release-on-nothing cancels)." },
};

/* ---------------- shared shell ---------------- */

const stage = document.getElementById("stage");

function cornerButton(corner, side, content, label) {
  return `<button class="corner-btn side-${side}" data-corner="${corner}" data-open="${state.open === corner}" aria-label="${label}">${content}<span class="hold-hint"></span></button>`;
}

function padChip(corner, key, extra) {
  if (!state.padOverlay || state.variant !== "D") return "";
  const pos = { tl: "left:6px;top:6px;", tr: "right:6px;top:6px;", bl: "left:6px;bottom:6px;", br: "right:6px;bottom:6px;" }[corner];
  return `<span class="pad-chip" style="${pos}"><b>${key}</b> ${extra}</span>`;
}

function topBar() {
  const s = session();
  return `<div class="cm-top">
    ${cornerButton("tl", "left", dirIcon(dir()), "Navigator (hold: quick project switch)")}
    ${padChip("tl", "L1", "tap nav · hold projects")}
    <div class="cm-title"><strong>${escapeHtml(s.name)}</strong><small>${escapeHtml(dir().name)} [${s.id}] · ${escapeHtml(tenant().name)}</small></div>
    ${cornerButton("tr", "right", glyph(state.tool), "Tools (tap: Files / previous tool on blank release)")}
    ${padChip("tr", "R1", "tap files · hold wheel")}
  </div>`;
}

function composerRow() {
  return `<div class="cm-composer">
    ${cornerButton("bl", "left", glyph("swap"), "Previous session (hold: quick switch)")}
    ${padChip("bl", "L2", "tap prev · hold fan")}
    <input id="prompt-input" placeholder="Ask ${escapeHtml(dir().name)}…" value="${escapeHtml(state.input)}" autocomplete="off" />
    ${cornerButton("br", "right", glyph("send"), "Send (hold: attach, voice, model, fork)")}
    ${padChip("br", "R2", "tap send · hold radial")}
  </div>`;
}

/* Context-aware status line: the segment set follows the focused surface.
   In the real product each segment is a config-bound (provider, template)
   pair; presets define the defaults, user config overrides them. */
function statusSegments() {
  const d = dir();
  if (state.open === "nav" || state.open === "tenants") {
    const n = sessionsOf(state.tenantId, state.dirId).length;
    return [`<b>${escapeHtml(d.name)}</b>`, `${n} sessions`, "private", "isolation: developer"];
  }
  if (state.open === "tools") {
    return [`<b>${escapeHtml(d.path)}</b>`, "3 changed", "watcher: live"];
  }
  const s = session();
  const shown = displayStatus(s, state.tenantId, state.dirId);
  return [`<b>●</b> ${shown}`, state.model, "24.1k · 12%", "v0.5.0"];
}

function statusBar() {
  return `<div class="cm-statusbar">
    <button class="cm-status" data-open-status aria-label="Agent state" data-open="${state.open === "status"}">${statusSegments().map((seg) => `<span>${seg}</span>`).join("")}</button>
  </div>`;
}

function bottomBar() {
  return `<div class="cm-bottom">${statusBar()}
    ${state.padOverlay ? `<div class="pad-legend"><b>L1</b> navigator · <b>R1</b> files/wheel · <b>L2</b> prev session/fan · <b>R2</b> send/radial</div>` : ""}
  </div>`;
}

function chat() {
  return `<div class="cm-content" id="chat-scroll">${state.chat.map(([author, text, tool], i) => `
    <article class="msg" data-author="${author}" data-msg-index="${i}">
      <header><b>${author === "user" ? "You" : escapeHtml(dir().name)}</b><span>15:0${i}</span></header>
      <p>${escapeHtml(text)}</p>${tool ? `<div class="tool">${escapeHtml(tool)}</div>` : ""}
    </article>`).join("")}</div>
    ${scrollRail()}`;
}

/* ---------------- navigator ---------------- */

function navigatorPanel(fullscreen) {
  const t = tenant();
  const accent = tenantAccent(t);
  const d = dir();
  const listHtml = state.query ? searchAllRows() : dirRows();
  const sessionCount = sessionsOf(state.tenantId, state.dirId).length;
  return `<section class="navigator" data-fullscreen="${fullscreen}">
    <div class="nav-ribbon">
      <button class="wd-chip wd-new" data-new-project title="New project">${glyph("plus")}<small>new</small></button>
      ${t.workdirs.map((w) => {
        const att = dirAttention(state.tenantId, w);
        return `<button class="wd-chip" data-dir="${w.id}" data-active="${w.id === state.dirId}">${dirIcon(w)}<small>${escapeHtml(w.name.slice(0, 8))}</small>${att ? `<span class="att-dot" style="background:${STATUS_COLOR[att]}"></span>` : ""}</button>`;
      }).join("")}
    </div>
    <div class="nav-main" style="--accent-border:${accent}">
      <div class="nav-tenant" style="border-bottom-color:${accent}">
        ${t.logo ?? proceduralIcon(t.name)}
        <div class="grow"><b>Projects (${t.workdirs.length}) &amp; Sessions</b><br><small>${escapeHtml(t.name)} · WORKSPACE</small></div>
        <button class="nav-tenant-switch" data-open-tenants>SWITCH</button>
      </div>
      <div class="fuzzy">${glyph("search")}<input id="fuzzy-input" placeholder="Fuzzy — searches whole workspace…" value="${escapeHtml(state.query)}" autocomplete="off" /></div>
      <div class="nav-list">
        <button class="row-item row-new" data-new-session>${glyph("plus")}<span class="grow">start new Session<small>in ${escapeHtml(d.name)}</small></span></button>
        ${listHtml}
      </div>
      <div class="nav-status">${escapeHtml(d.name)} | ${sessionCount} sessions | private | isolation: developer</div>
    </div>
  </section>`;
}

function sessionRowMeta(s) {
  return `${s.updated} | ${s.msgs} messages | ${s.tokens} tokens`;
}

function dirRows() {
  const current = session();
  return sessionsOf(state.tenantId, state.dirId).map((s) => {
    const shown = displayStatus(s, state.tenantId, state.dirId);
    return `
    <button class="row-item" data-session="${s.id}" data-current="${s.id === current.id}">
      <span class="status-dot" data-status="${shown}" style="background:${STATUS_COLOR[shown]}"></span>
      <span class="grow"><span>${escapeHtml(s.name)} <i class="rid">[${s.readable}]</i></span><small>${sessionRowMeta(s)}</small></span>
    </button>`;
  }).join("");
}

function searchAllRows() {
  const rows = [];
  for (const d of tenant().workdirs) {
    for (const s of sessionsOf(state.tenantId, d.id)) {
      const m = fuzzy(state.query, s.name);
      if (m) rows.push({ d, s, html: m.html, score: m.score + fuzzy(state.query, d.name) ? m.score : 0 });
    }
  }
  rows.sort((a, b) => b.score - a.score);
  if (rows.length === 0) return `<div class="sheet-empty">No sessions match "${escapeHtml(state.query)}".</div>`;
  return rows.map(({ d, s, html }) => {
    const shown = displayStatus(s, state.tenantId, d.id);
    return `
    <button class="row-item" data-dir="${d.id}" data-session="${s.id}">
      <span class="nav-dir-chip">${escapeHtml(d.name.slice(0, 10))}</span>
      <span class="grow"><span>${html} <i class="rid">[${s.readable}]</i></span><small>${sessionRowMeta(s)}</small></span>
      <span class="status-dot" data-status="${shown}" style="background:${STATUS_COLOR[shown]}"></span>
    </button>`;
  }).join("");
}

/* ---------------- expansions ---------------- */

function tenantsSheet() {
  return `<button class="scrim" data-close aria-label="Close"></button><section class="sheet from-left">
    <div class="sheet-head">${glyph("grid")}<h2>WORKSPACES (TENANTS)</h2></div>
    ${TENANTS.map((t) => `
      <button class="row-item" data-tenant="${t.id}" data-current="${t.id === state.tenantId}" style="padding:10px 12px">
        ${t.logo ?? proceduralIcon(t.name)}<span class="grow">${escapeHtml(t.name)}<small>${t.workdirs.length} workdirs</small></span>
      </button>`).join("")}
  </section>`;
}

function toolStrip() {
  return `<button class="scrim" data-close aria-label="Close"></button><nav class="strip-v" aria-label="Tools">${TOOLS.map(([id, label]) => `
    <button data-tool="${id}" data-current="${id === state.tool}" aria-label="${label}">${glyph(id)}</button>`).join("")}
  </nav>`;
}

function toolSheet() {
  return `<button class="scrim" data-close aria-label="Close"></button><section class="sheet quad-tr">
    <div class="sheet-head">${glyph(state.tool)}<h2>WORKSPACE TOOLS</h2></div>
    ${fuzzyBox("Fuzzy search files…")}
    <div class="sheet-list">${TOOLS.map(([id, label]) => `
      <button class="row-item" data-tool="${id}" data-current="${id === state.tool}">${glyph(id)}<span class="grow">${label}</span></button>`).join("")}
    </div>
  </section>`;
}

function statusStrip() {
  return `<button class="scrim" data-close aria-label="Close"></button><div class="strip-h left-id">
    <button aria-label="Tasks">${glyph("agent")}</button>
    <span class="grow" style="font-size:11px;color:var(--muted)">Task 3/5 · Build the responsive surfaces</span>
    <span class="meta"><span><b>24.1k</b> / 200k</span><span>runner 0/26</span></span>
  </div>`;
}

function modelsSheet() {
  return `<button class="scrim" data-close aria-label="Close"></button><section class="sheet from-bottom">
    <div class="sheet-head">${glyph("cpu")}<h2>MODEL</h2></div>
    <div class="sheet-list">${MODELS.map(([id, hint]) => `
      <button class="row-item" data-model="${id}" data-current="${id === state.model}">
        ${glyph("cpu")}<span class="grow">${id}<small>${hint}</small></span>
      </button>`).join("")}</div>
  </section>`;
}

function fuzzyBox(placeholder) {
  return `<div class="fuzzy">${glyph("search")}<input id="fuzzy-input" placeholder="${placeholder}" value="${escapeHtml(state.query)}" autocomplete="off" /></div>`;
}

function expansion() {
  if (state.open === "tools") return state.variant === "B" ? toolStrip() : state.variant === "C" ? toolSheet() : "";
  if (state.open === "status") return statusStrip();
  return "";
}

/* ---------------- scroll rail ---------------- */

const RAIL_MIN_MESSAGES = 8;
const RAIL_MAX_DOTS = 14;

function railDots() {
  const messages = state.chat;
  if (messages.length < RAIL_MIN_MESSAGES) return [];
  const perDot = Math.max(1, Math.ceil(messages.length / RAIL_MAX_DOTS));
  const dots = [];
  for (let i = 0; i < messages.length; i += perDot) dots.push({ index: i, author: messages[i][0] });
  return dots;
}

function scrollRail() {
  const dots = railDots();
  if (dots.length < 2) return "";
  return `<nav class="scroll-rail" id="scroll-rail" aria-label="Quick scroll">${dots.map((dot) => `
    <button class="rail-dot" data-rail-index="${dot.index}" data-author="${dot.author}" aria-label="Jump to message ${dot.index + 1}"></button>`).join("")}
  </nav>`;
}

function railPreview(index) {
  const msg = state.chat[index];
  if (!msg) return;
  state.preview = { index, author: msg[0], text: msg[1] };
  render();
}

function railPreviewBubble() {
  if (!state.preview) return "";
  const { index, author, text } = state.preview;
  return `<div class="rail-preview" data-author="${author}">
    <small>${author === "user" ? "You" : escapeHtml(dir().name)} · msg ${index + 1}/${state.chat.length}</small>
    <p>${escapeHtml(text.length > 110 ? `${text.slice(0, 110)}…` : text)}</p>
  </div>`;
}

/* ---------------- hold menus ---------------- */

function projectCarouselItems() {
  return tenant().workdirs.map((d) => {
    const sessions = sessionsOf(state.tenantId, d.id);
    const statuses = sessions.map((x) => displayStatus(x, state.tenantId, d.id));
    const active = statuses.filter((x) => x === "working").length;
    const blocked = statuses.filter((x) => x === "blocked").length;
    return {
      id: d.id,
      face: dirIcon(d),
      label: d.name,
      sub: `${active} active · ${blocked} blocked · ${sessions.length} sessions`,
    };
  });
}

function tenantCarouselItems() {
  return TENANTS.map((t) => ({ face: t.logo ?? proceduralIcon(t.name), label: t.name, sub: `${t.workdirs.length} WORKDIRS`, id: t.id }));
}

function toolCarouselItems() {
  return TOOLS.map(([id, label]) => ({ face: glyph(id), label, id }));
}

function fanItems() {
  const out = [];
  for (const m of state.mru) {
    const tn = TENANTS.find((t) => t.id === m.tenant);
    const dd = tn.workdirs.find((d) => d.id === m.dir);
    const ss = sessionsOf(m.tenant, m.dir).find((s) => s.id === m.session);
    if (tn && dd && ss) out.push({ tenant: tn, dir: dd, session: ss });
  }
  return out.slice(0, 5);
}

function holdMenu() {
  if (!state.hold) return "";
  const { kind, items, sel } = state.hold;
  if (kind === "qradial") return quarterRadial();
  if (kind === "fan") {
    return `<div class="fan" id="hold-menu">
      <div class="fan-hint">QUICK SWITCH · RELEASE TO OPEN</div>
      ${items.map((item, i) => `<div class="fan-card" data-sel="${i === sel}"><b>${escapeHtml(item.session.name)}</b><small>${escapeHtml(item.tenant.name)} · ${escapeHtml(item.dir.name)}</small></div>`).join("")}
    </div>`;
  }
  const at = state.hold.corner === "tl" ? "at-top" : "at-top";
  const current = items[sel];
  return `<div class="mgs ${at}" id="hold-menu">
    <div class="mgs-row">${items.map((item, i) => `<div class="mgs-item" data-sel="${i === sel}">${item.face}</div>`).join("")}</div>
    <div class="mgs-label">${escapeHtml(current.label)}<small>${escapeHtml(current.sub ?? "RELEASE TO EQUIP · CENTER TO CANCEL")}</small></div>
  </div>`;
}

/* quarter-arc radial anchored at a corner, sweeping the interior quadrant */
function phoneZoom() {
  if (state.variant === "D") return 1;
  return Number.parseFloat(getComputedStyle(document.documentElement).getPropertyValue("--phone-scale")) || 1;
}

function quarterRadial() {
  const { corner, items, sel } = state.hold;
  const btn = document.querySelector(`.corner-btn[data-corner="${corner}"]`);
  const screen = document.getElementById("screen");
  if (!btn || !screen) return "";
  const rect = btn.getBoundingClientRect();
  const root = screen.getBoundingClientRect();
  const zoom = phoneZoom();
  const cx = (rect.left + rect.width / 2 - root.left) / zoom;
  const cy = (rect.top + rect.height / 2 - root.top) / zoom;
  /* interior quadrant direction per corner */
  const base = { br: 225, bl: 315, tr: 135, tl: 45 }[corner]; /* diagonal into the interior quadrant (screen y is down) */
  const spread = 90; /* quarter arc */
  const step = items.length > 1 ? spread / (items.length - 1) : 0;
  const radius = 88;
  const angleFor = (i) => ((base - spread / 2 + i * step) * Math.PI) / 180;
  const tiles = items.map((item, i) => {
    const a = angleFor(i);
    const x = cx + Math.cos(a) * radius * (corner[0] === "b" ? 1 : 1);
    const y = cy + Math.sin(a) * radius;
    return `<div class="qradial-item" data-sel="${i === sel}" style="left:${Math.round(x)}px;top:${Math.round(y)}px">${glyph(item.glyph)}</div>`;
  }).join("");
  const current = items[sel];
  const la = angleFor(sel);
  const lx = cx + Math.cos(la) * (radius + 52);
  const ly = cy + Math.sin(la) * (radius + 52);
  return `<div class="qradial" id="hold-menu">
    <div class="qradial-deadzone" style="left:${Math.round(cx)}px;top:${Math.round(cy)}px"></div>
    ${tiles}
    <div class="qradial-label" style="left:${Math.round(lx)}px;top:${Math.round(ly)}px">${current.label}<small>RELEASE TO ${current.sub ?? "SELECT"} · CORNER = CANCEL</small></div>
  </div>`;
}

/* ---------------- gestures ---------------- */

const HOLD_MS = 320;
let holdTimer = null;
let pressedCorner = null;

function holdItemsFor(corner) {
  if (corner === "tl") return { kind: "mgs", items: projectCarouselItems(), sel: tenant().workdirs.findIndex((d) => d.id === state.dirId) };
  if (corner === "tr") return { kind: "mgs", items: toolCarouselItems(), sel: TOOLS.findIndex(([id]) => id === state.tool) };
  if (corner === "br") return { kind: "qradial", items: RADIAL_ACTIONS.map(([g, label, sub]) => ({ glyph: g, label, sub, id: g })), sel: 0 };
  return { kind: "fan", items: fanItems(), sel: 0 };
}

function openNavigator() { state.query = ""; state.open = "nav"; render(); }

function tapAction(corner) {
  if (corner === "tl") { state.open = state.open === "nav" ? null : "nav"; state.query = ""; }
  else if (corner === "tr") { setTool("files"); state.open = null; }
  else if (corner === "bl") { switchToPrevious(); }
  else if (corner === "br") { sendMessage(); }
  render();
}

function setTool(id) {
  if (state.tool === id) return;
  state.toolHistory = [id, ...state.toolHistory.filter((t) => t !== id)].slice(0, 4);
  state.tool = id;
}

function switchToPrevious() {
  const prev = previousTarget();
  if (!prev) { showToast("No previous session yet."); return; }
  gotoSession(prev.tenant, prev.dir, prev.session);
  state.open = null;
}

function showToast(text) {
  state.toast = text;
  render();
  setTimeout(() => { if (state.toast === text) { state.toast = null; render(); } }, 1800);
}

function sendMessage() {
  const text = state.input.trim();
  if (!text) { showToast("Type a prompt first — <b>send</b> taps, hold for attach / voice / model / fork."); return; }
  state.chat.push(["user", text, null]);
  state.input = "";
  state.open = null;
  render();
  showToast("Sent. (Probe: streaming response not simulated.)");
}

function commitHold() {
  const { kind, items, sel, corner, moved } = state.hold;
  state.hold = null;
  /* blank release (no drag): recency actions per corner, cancel on act corners */
  if (!moved) {
    if (corner === "tr") { const prevTool = state.toolHistory.find((t) => t !== state.tool); if (prevTool) setTool(prevTool); showToast(`Tool: <b>${prevTool ?? state.tool}</b>`); }
    else if (corner === "bl") { switchToPrevious(); }
    render();
    return;
  }
  if (kind === "qradial") {
    const action = items[sel];
    if (action.id === "cpu") { state.open = "models"; state.query = ""; }
    else if (action.id === "clip") showToast("<b>Attach</b> — probe: file/context picker would open.");
    else if (action.id === "mic") showToast("<b>Voice</b> — probe: voice mode toggled on.");
    else if (action.id === "branch") showToast("<b>Fork</b> — probe: fork confirmation would open.");
  } else if (kind === "mgs") {
    if (corner === "tl") {
      state.dirId = items[sel].id;
      const first = sessionsOf(state.tenantId, state.dirId)[0];
      if (first) gotoSession(state.tenantId, state.dirId, first.id);
      state.open = null;
    } else if (corner === "tr") { setTool(items[sel].id); state.open = null; }
  } else if (kind === "fan") {
    const item = items[sel];
    gotoSession(item.tenant.id, item.dir.id, item.session.id);
    state.open = null;
  }
  render();
}

function moveSelection(event) {
  if (!state.hold) return;
  const menu = document.getElementById("hold-menu");
  if (!menu) return;
  const before = state.hold.sel;
  if (state.hold.kind === "qradial") {
    const btn = document.querySelector(`.corner-btn[data-corner="${state.hold.corner}"]`);
    const screen = document.getElementById("screen");
    if (!btn || !screen) return;
    const rect = btn.getBoundingClientRect();
    const root = screen.getBoundingClientRect();
    const cx = rect.left + rect.width / 2;
    const cy = rect.top + rect.height / 2;
    const dx = event.clientX - cx;
    const dy = event.clientY - cy;
    const dist = Math.hypot(dx, dy);
    if (dist < 30 * phoneZoom()) { return; } /* dead zone: stay */
    const base = { br: 225, bl: 315, tr: 135, tl: 45 }[state.hold.corner];
    const spread = 90;
    const step = state.hold.items.length > 1 ? spread / (state.hold.items.length - 1) : 0;
    let angle = (Math.atan2(dy, dx) * 180) / Math.PI;
    /* pick nearest item angle */
    let best = state.hold.sel; let bestDist = Number.POSITIVE_INFINITY;
    for (let i = 0; i < state.hold.items.length; i += 1) {
      const itemAngle = base - spread / 2 + i * step;
      const d = Math.abs(((angle - itemAngle + 540) % 360) - 180);
      if (d < bestDist) { bestDist = d; best = i; }
    }
    state.hold.sel = best;
    state.hold.moved = state.hold.moved || best !== before || true;
    if (state.hold.sel !== before) render();
    return;
  }
  const selector = state.hold.kind === "fan" ? ".fan-card" : ".mgs-item";
  const tiles = [...menu.querySelectorAll(selector)];
  let best = state.hold.sel; let bestDist = Number.POSITIVE_INFINITY;
  tiles.forEach((tile, i) => {
    const r = tile.getBoundingClientRect();
    const dx = event.clientX - (r.left + r.width / 2);
    const dy = event.clientY - (r.top + r.height / 2);
    const dist = state.hold.kind === "fan" ? Math.abs(dy) : Math.abs(dx);
    if (dist < bestDist) { bestDist = dist; best = i; }
  });
  if (best !== state.hold.sel) { state.hold.sel = best; state.hold.moved = true; render(); }
  else state.hold.moved = true;
}

function bindShell() {
  const screen = document.getElementById("screen");
  if (!screen) return;

  for (const btn of screen.querySelectorAll(".corner-btn")) {
    btn.addEventListener("pointerdown", (event) => {
      event.preventDefault();
      const corner = btn.dataset.corner;
      pressedCorner = corner;
      state.hold = null;
      const startHold = () => { state.hold = { corner, moved: false, ...holdItemsFor(corner) }; render(); };
      if (!state.vocabInverted) holdTimer = setTimeout(startHold, HOLD_MS);
      else { startHold(); holdTimer = null; }
    });
  }

  if (!window.__cmDragBound) {
    window.__cmDragBound = true;
    window.addEventListener("pointermove", moveSelection);
    window.addEventListener("pointerup", () => {
      if (holdTimer) { clearTimeout(holdTimer); holdTimer = null; }
      if (state.hold) { commitHold(); pressedCorner = null; return; }
      if (pressedCorner) { const corner = pressedCorner; pressedCorner = null; tapAction(corner); }
    });
    window.addEventListener("pointercancel", () => {
      if (holdTimer) { clearTimeout(holdTimer); holdTimer = null; }
      state.hold = null; pressedCorner = null; render();
    });
  }

  screen.addEventListener("click", (event) => {
    const railDot = event.target.closest("[data-rail-index]");
    if (railDot) {
      const index = Number(railDot.dataset.railIndex);
      const msg = document.querySelector(`[data-msg-index="${index}"]`);
      if (msg) msg.scrollIntoView({ block: "center" });
      state.preview = null;
      render();
      return;
    }
    const target = event.target.closest("[data-close],[data-dir],[data-session],[data-tenant],[data-tool],[data-open-tenants],[data-open-status],[data-quick],[data-model],[data-pin],[data-split],[data-new-project],[data-new-session]");
    if (!target) return;
    if (target.dataset.close !== undefined) { state.open = null; state.query = ""; }
    else if (target.dataset.tenant) {
      state.tenantId = target.dataset.tenant;
      state.dirId = tenant().workdirs[0].id;
      state.open = "nav"; state.query = "";
    }
    else if (target.dataset.openTenants !== undefined) { state.open = "tenants"; state.query = ""; }
    else if (target.dataset.newProject !== undefined) { showToast("<b>New project</b> — probe: template picker would open."); return; }
    else if (target.dataset.newSession !== undefined) { showToast("<b>New Session</b> — probe: would start in the current workdir."); return; }
    else if (target.dataset.openStatus !== undefined) { state.query = ""; state.open = state.open === "status" ? null : "status"; }
    else if (target.dataset.model) { state.model = target.dataset.model; state.open = null; }
    else if (target.dataset.tool) { setTool(target.dataset.tool); if (state.variant === "A") state.open = null; else state.open = "tools"; }
    else if (target.dataset.session) {
      if (target.dataset.dir) state.dirId = target.dataset.dir;
      state.seen.add(seenKey(state.tenantId, state.dirId, target.dataset.session));
      state.sessionByDir[state.dirId] = target.dataset.session;
      pushMru(state.tenantId, state.dirId, target.dataset.session);
      state.open = null; state.query = "";
    }
    else if (target.dataset.split !== undefined) { state.classic = !state.classic; }
    render();
  });
}

/* ---------------- rendering ---------------- */

function renderPhone() {
  stage.classList.remove("desktop-mode");
  const navOpen = state.open === "nav";
  stage.innerHTML = `<div class="phone"><div class="notch"></div><div class="screen" id="screen">
    ${topBar()}
    ${navOpen ? navigatorPanel(true) : chat()}
    ${state.open === "tenants" ? tenantsSheet() : ""}
    ${expansion()}
    ${state.open === "models" ? modelsSheet() : ""}
    ${holdMenu()}${railPreviewBubble()}${composerRow()}${bottomBar()}${state.toast ? `<div class="toast">${state.toast}</div>` : ""}
  </div></div>`;
}

function renderDesktop() {
  stage.classList.add("desktop-mode");
  const navOpen = state.pinned.left || state.open === "nav" || state.classic;
  const toolsOpen = state.pinned.right || state.open === "tools" || state.classic;
  stage.innerHTML = `<div class="bp ${state.classic ? "classic" : ""}" id="screen">
    ${topBar()}
    <button class="split-toggle" data-split>${state.classic ? "◧ CORNER MODE" : "◫ 3-WAY SPLIT"}</button>
    <div class="bp-main">
      <div class="bp-edge left" data-open="${navOpen}" style="${navOpen ? "width:min(340px,26vw)" : ""}">
        ${navigatorPanel(false)}
      </div>
      <div class="bp-center">${chat()}</div>
      <div class="bp-edge right" data-open="${toolsOpen}">
        <div class="sheet-head">${glyph("files")}<h2>FILES</h2>
          <button class="pin-btn" data-pin="right" aria-pressed="${state.pinned.right}" aria-label="Pin">${glyph("pin")}</button></div>
        <div class="sheet-list">${TOOLS.map(([id, label]) => `
          <button class="row-item" data-tool="${id}" data-current="${id === state.tool}">${glyph(id)}<span class="grow">${label}</span></button>`).join("")}
        </div>
      </div>
    </div>
    ${holdMenu()}${railPreviewBubble()}${composerRow()}${bottomBar()}${state.toast ? `<div class="toast">${state.toast}</div>` : ""}
  </div>`;
  /* navigator inside an edge: constrain to the edge box */
  const nav = document.querySelector(".bp-edge.left .navigator");
  if (nav) { nav.style.position = "relative"; nav.style.top = "0"; nav.style.left = "0"; nav.style.right = "0"; nav.style.bottom = "0"; }
}

function render() {
  if (state.variant === "D") renderDesktop(); else renderPhone();
  bindShell();
  const prompt = document.getElementById("prompt-input");
  if (prompt) {
    prompt.addEventListener("input", () => { state.input = prompt.value; });
    prompt.addEventListener("keydown", (event) => { if (event.key === "Enter") sendMessage(); });
  }
  const chatScroll = document.getElementById("chat-scroll");
  if (chatScroll) chatScroll.scrollTop = chatScroll.scrollHeight;
  for (const dot of document.querySelectorAll(".rail-dot")) {
    dot.addEventListener("pointerenter", () => railPreview(Number(dot.dataset.railIndex)));
  }
  const rail = document.getElementById("scroll-rail");
  if (rail) rail.addEventListener("pointerleave", () => { state.preview = null; render(); });
  const input = document.getElementById("fuzzy-input");
  if (input) {
    input.addEventListener("input", () => {
      state.query = input.value;
      const keep = input.selectionStart;
      render();
      const again = document.getElementById("fuzzy-input");
      if (again) { again.focus(); again.setSelectionRange(keep, keep); }
    });
  }
  renderControls();
}

/* ---------------- controls ---------------- */

function renderControls() {
  const tabs = document.querySelector(".variant-tabs");
  tabs.innerHTML = Object.entries(VARIANTS).map(([key, v]) => `
    <button type="button" data-variant="${key}" aria-pressed="${state.variant === key}"><b>${key}</b> ${v.label}</button>`).join("");
  for (const btn of tabs.querySelectorAll("button")) {
    btn.addEventListener("click", () => {
      state.variant = btn.dataset.variant;
      state.open = null; state.hold = null; state.query = "";
      render();
    });
  }
  const vocab = document.getElementById("vocab-toggle");
  vocab.setAttribute("aria-pressed", String(state.vocabInverted));
  vocab.textContent = state.vocabInverted ? "press=carousel · release=equip" : "tap=do · hold=choose";
  const pad = document.getElementById("pad-toggle");
  pad.style.display = state.variant === "D" ? "" : "none";
  pad.setAttribute("aria-pressed", String(state.padOverlay));
  document.getElementById("variant-note").innerHTML = VARIANTS[state.variant].note;
}

document.getElementById("vocab-toggle").addEventListener("click", () => {
  state.vocabInverted = !state.vocabInverted; state.open = null; state.hold = null; render();
});
const padRow = document.createElement("div");
padRow.className = "ctl-row";
const padLabel = document.createElement("span");
padLabel.textContent = "Controller";
const padBtn = document.createElement("button");
padBtn.id = "pad-toggle";
padBtn.type = "button";
padBtn.className = "switch";
padBtn.textContent = "PAD overlay";
padBtn.addEventListener("click", () => { state.padOverlay = !state.padOverlay; render(); });
padRow.append(padLabel, padBtn);
document.getElementById("vocab-toggle").closest(".ctl-row").after(padRow);
document.getElementById("ctl-collapse").addEventListener("click", () => {
  document.getElementById("controls").classList.toggle("collapsed");
});
document.addEventListener("keydown", (event) => {
  if (event.target.tagName === "INPUT") {
    if (event.key === "Escape") { state.open = null; state.query = ""; render(); }
    return;
  }
  const map = { 1: "A", 2: "B", 3: "C", 4: "D" };
  if (map[event.key]) { state.variant = map[event.key]; state.open = null; state.hold = null; render(); }
  if (event.key === "Escape") { state.open = null; state.hold = null; render(); }
});

/* Fit the whole phone (incl. bottom corners) into the viewport. */
function fitPhone() {
  const scale = Math.max(0.5, Math.min(1, (window.innerHeight - 48) / 864));
  document.documentElement.style.setProperty("--phone-scale", String(scale));
}
window.addEventListener("resize", fitPhone);
fitPhone();

render();
showToast("Hold the <b>send</b> button for the radial: attach · voice · model · fork.");

window.__cmState = state;
