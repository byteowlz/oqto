/* OqtoUI corner-mode concept gallery (oqto-m5sp). Fixture-only design probe. */
"use strict";

/* ---------------- fixtures ---------------- */

const OQTO_LOGO = `<svg class="ws-face" viewBox="0 0 32 32" aria-hidden="true"><circle cx="16" cy="13" r="7" fill="#f2f5f3"/><path d="M9 13c-2 7-4 9-6 10 3 1 5 0 6-1 0 2-1 4-3 5 3 1 5-1 6-3 1 2 1 4 0 6 2-1 4-3 4-6 1 3 3 5 6 5-2-2-2-4-2-6 2 2 4 2 6 1-2-1-3-3-3-5 2 1 4 1 6-1-3-1-5-3-6-10z" fill="#f2f5f3"/><rect x="11" y="10" width="10" height="4" fill="#0f1412"/></svg>`;
const SLDR_LOGO = `<svg class="ws-face" viewBox="0 0 32 32" aria-hidden="true"><rect x="4" y="7" width="24" height="15" fill="none" stroke="#d4c275" stroke-width="2.4"/><path d="M9 25h14" stroke="#d4c275" stroke-width="2.4"/><path d="M8 17l5-5 4 3 6-6" fill="none" stroke="#d4c275" stroke-width="2.4"/></svg>`;

const WORKSPACES = [
  { id: "oqto", name: "oqto_refactor", path: "~/byteowlz/oqto_refactor", logo: OQTO_LOGO },
  { id: "sldr", name: "sldr", path: "~/byteowlz/sldr", logo: SLDR_LOGO },
  { id: "ctx", name: "ctx", path: "~/byteowlz/ctx" },
  { id: "mmry", name: "mmry", path: "~/byteowlz/mmry" },
  { id: "tmpltr", name: "tmpltr", path: "~/byteowlz/tmpltr" },
  { id: "skills", name: "skillissues", path: "~/byteowlz/skillissues" },
  { id: "hypr", name: "hyprland-config", path: "~/dotfiles/hyprland" },
  { id: "wiki", name: "wiki", path: "~/wiki" },
];

const SESSION_NAMES = [
  ["Corner-mode gallery build", "working"], ["Chat persistence diagnosis", "blocked"],
  ["Prepare v0.5.1", "done"], ["Gallery App vertical slice", "working"],
  ["Speaker view polish", "idle"], ["Audit browser skills", "idle"],
  ["Egress tier rollout", "done"], ["Memory dedup sweep", "working"],
  ["Template JSON pipeline", "idle"], ["Theme contrast pass", "done"],
];

const STATUS_COLOR = { working: "var(--accent)", blocked: "var(--red)", done: "var(--blue)", idle: "var(--faint)" };

function seededSessions(ws) {
  const n = 2 + (hash(ws.id) % 4);
  const out = [];
  for (let i = 0; i < n; i += 1) {
    const [name, status] = SESSION_NAMES[(hash(ws.id) + i * 3) % SESSION_NAMES.length];
    out.push({ id: `${ws.id}-s${i}`, name, status, updated: `2026/08/${10 + i} - ${9 + i}:2${i}` });
  }
  return out;
}

const CHAT = [
  ["user", "Let's explore the four-corner interaction model for mobile.", null],
  ["agent", "Corners are the strongest touch targets we have. I'll keep the title and status bars and hang the corner buttons off their ends.", "Reading docs/adr/0037-oqto-ui-portable-rearrangeable-views.md"],
  ["agent", "Hold a corner for the item carousel — release to equip, exactly like the MGS menus.", "Editing corner-mode/app.js"],
  ["user", "Left expansions should keep the dark rail identity, right ones the lighter pane.", null],
  ["agent", "Done. Left sheets inherit the sidebar surface, right strips the files-pane surface, so the desktop split and corner mode share one spatial memory.", null],
];

const MODELS = [
  ["opus-4.7", "deep · slow"], ["sonnet-4.6", "balanced"], ["haiku-4.5", "fast"], ["o4-mini", "cheap"],
];

const TOOLS = [
  ["files", "Files"], ["editor", "Editor"], ["terminal", "Terminal"], ["gallery", "Gallery"], ["diff", "Changes"],
];

/* ---------------- utilities ---------------- */

function hash(text) {
  let h = 2166136261;
  for (let i = 0; i < text.length; i += 1) { h ^= text.charCodeAt(i); h = Math.imul(h, 16777619); }
  return h >>> 0;
}

const ICON_HUES = ["#3ba77c", "#3ba7a0", "#d4c275", "#5b8fc9", "#9b7fc9", "#d96b69", "#5bc79a"];

/* Deterministic flat geometric identicon from the workspace name. */
function proceduralIcon(name) {
  const h = hash(name);
  const a = ICON_HUES[h % ICON_HUES.length];
  const b = ICON_HUES[(h >> 3) % ICON_HUES.length];
  const parts = [];
  const grid = 4;
  for (let cell = 0; cell < grid * grid; cell += 1) {
    const bit = (h >> (cell % 27)) & 1;
    if (!bit) continue;
    const x = (cell % grid) * 8; const y = Math.floor(cell / grid) * 8;
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

function wsIcon(ws) { return ws.logo ?? proceduralIcon(ws.name); }

/* fzf-style subsequence match; returns highlighted HTML or null. */
function fuzzy(query, text) {
  if (!query) return { score: 0, html: escapeHtml(text) };
  const q = query.toLowerCase(); const t = text.toLowerCase();
  let qi = 0; let score = 0; let last = -2; const marks = new Set();
  for (let ti = 0; ti < t.length && qi < q.length; ti += 1) {
    if (t[ti] === q[qi]) {
      marks.add(ti);
      score += last === ti - 1 ? 3 : 1;
      last = ti; qi += 1;
    }
  }
  if (qi < q.length) return null;
  let html = "";
  for (let i = 0; i < text.length; i += 1) {
    html += marks.has(i) ? `<span class="match">${escapeHtml(text[i])}</span>` : escapeHtml(text[i]);
  }
  return { score, html };
}

function escapeHtml(value) {
  return value.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);
}

const GLYPHS = {
  sessions: '<path d="M4 5h16v11H8l-4 4z"/>',
  files: '<path d="M4 6a2 2 0 0 1 2-2h4l2 2h6a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2z"/>',
  editor: '<path d="M8 4l-6 8 6 8"/><path d="M16 4l6 8-6 8"/>',
  terminal: '<path d="M4 5h16v14H4z"/><path d="M7 9l3 3-3 3"/><path d="M12 15h5"/>',
  gallery: '<rect x="3" y="5" width="18" height="14"/><circle cx="9" cy="10" r="1.6"/><path d="M4 17l5-5 4 3 4-4 3 3"/>',
  diff: '<path d="M7 4v16"/><path d="M17 4v10"/><circle cx="17" cy="18" r="2"/><path d="M5 8l2-2 2 2"/>',
  plus: '<path d="M12 5v14M5 12h14"/>',
  palette: '<circle cx="12" cy="12" r="9"/><circle cx="9" cy="9" r="1.4"/><circle cx="15" cy="9" r="1.4"/><circle cx="9" cy="15" r="1.4"/>',
  send: '<path d="M4 12l16-8-6 16-2.5-6z"/>',
  search: '<circle cx="10" cy="10" r="6"/><path d="M15 15l5 5"/>',
  pin: '<path d="M9 4h6l-1 7 3 3H7l3-3z"/><path d="M12 14v6"/>',
  bolt: '<path d="M13 3L5 14h6l-1 7 8-11h-6z"/>',
  mic: '<rect x="9" y="4" width="6" height="11" rx="3"/><path d="M6 11a6 6 0 0 0 12 0"/><path d="M12 17v3"/>',
  clip: '<path d="M8 12l8-8 4 4-9 9-4-4 8-8"/>',
  branch: '<circle cx="7" cy="6" r="2.4"/><circle cx="7" cy="18" r="2.4"/><circle cx="17" cy="8" r="2.4"/><path d="M7 8.4v7.2"/><path d="M9.2 6.8c4 0 5.4 1.2 5.6 3.6"/>',
  cpu: '<rect x="7" y="7" width="10" height="10"/><path d="M10 3v4M14 3v4M10 17v4M14 17v4M3 10h4M3 14h4M17 10h4M17 14h4"/>',
  swap: '<path d="M7 4v12"/><path d="M4 13l3 3 3-3"/><path d="M17 20V8"/><path d="M14 11l3-3 3 3"/>',
  agent: '<rect x="5" y="7" width="14" height="11"/><circle cx="10" cy="12" r="1.4"/><circle cx="14" cy="12" r="1.4"/><path d="M12 4v3"/>',
  grid: '<rect x="4" y="4" width="7" height="7"/><rect x="13" y="4" width="7" height="7"/><rect x="4" y="13" width="7" height="7"/><rect x="13" y="13" width="7" height="7"/>',
};

function glyph(name) {
  return `<svg class="gl" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${GLYPHS[name]}</svg>`;
}

/* ---------------- state ---------------- */

const state = {
  variant: "A",
  vocabInverted: false,
  wsId: "oqto",
  sessionByWs: {},
  open: null,          // "tl" | "tr" | "bl" | "br" | null (mobile exclusive)
  tool: "files",
  hold: null,          // { corner, items, sel, kind }
  pinned: { left: false, right: false },
  classic: false,
  query: "",
  input: "",
  chat: CHAT.map((row) => [...row]),
  toast: null,
  model: "opus-4.7",
};

function ws() { return WORKSPACES.find((w) => w.id === state.wsId); }
function sessions() { return seededSessions(ws()); }
function session() {
  const list = sessions();
  return list.find((s) => s.id === state.sessionByWs[state.wsId]) ?? list[0];
}
function recentSessions() {
  const all = [];
  for (const w of WORKSPACES.slice(0, 5)) {
    for (const s of seededSessions(w).slice(0, 2)) all.push({ ...s, ws: w });
  }
  return all.slice(0, 5);
}

/* ---------------- variants ---------------- */

const VARIANTS = {
  A: {
    label: "Edge strips + send corner",
    note: "Prompt is always visible above the bottom bar. <b>Tap</b> corners for edge surfaces (left = dark rail, right = lighter pane). Bottom-<b>right = send on tap</b>; hold for the radial (attach / voice / model / fork). Bottom-<b>left = quick switch</b> (hold = MRU fan). Status strip in the middle expands agent state. Exclusive: one surface at a time.",
  },
  B: {
    label: "Quadrant sheets",
    note: "Same bars and vocabulary, but top corners open <b>quarter sheets</b> with full content and fuzzy search instead of narrow strips. Feel whether the extra surface is worth covering the chat.",
  },
  C: {
    label: "MGS carousels",
    note: "Everything frequent is a <b>hold-carousel</b>: top-left workspaces, top-right tools, bottom-right quick-switch. Drag while holding, release to equip. Tap still opens the full sheet. Pure item-menu feel.",
  },
  D: {
    label: "Big picture (desktop)",
    note: "Fullscreen content with the same corners. Expansions <b>pin</b> open (composable, non-exclusive) and morph into the classic 3-way split with the toggle at the top — one spatial model across both.",
  },
};

/* ---------------- rendering ---------------- */

const stage = document.getElementById("stage");

function cornerButton(corner, side, content, label) {
  return `<button class="corner-btn side-${side}" data-corner="${corner}" data-open="${state.open === corner}" aria-label="${label}">${content}<span class="hold-hint"></span></button>`;
}

function topBar() {
  const s = session();
  return `<div class="cm-top">
    ${cornerButton("tl", "left", wsIcon(ws()), "Workspace and sessions")}
    <div class="cm-title"><strong>${escapeHtml(s.name)}</strong><small>${escapeHtml(ws().name)} [${s.id}]</small></div>
    ${cornerButton("tr", "right", glyph(state.tool), "Workspace tools")}
  </div>`;
}

function bottomBar() {
  return `<div class="cm-bottom">
    ${cornerButton("bl", "left", glyph("swap"), "Session quick switch")}
    <button class="cm-status" data-open-status aria-label="Agent state" data-open="${state.open === "status"}"><span><b>●</b> working</span><span>${state.model}</span><span>24.1k · 12%</span><span>v0.5.0</span></button>
    ${cornerButton("br", "right", glyph("send"), "Send (hold: attach, voice, more)")}
  </div>`;
}

function composerRow() {
  return `<div class="cm-composer"><input id="prompt-input" placeholder="Ask ${escapeHtml(ws().name)}…" value="${escapeHtml(state.input)}" autocomplete="off" /></div>`;
}

function chat() {
  return `<div class="cm-content" id="chat-scroll">${state.chat.map(([author, text, tool], i) => `
    <article class="msg" data-author="${author}">
      <header><b>${author === "user" ? "You" : escapeHtml(ws().name)}</b><span>15:0${i}</span></header>
      <p>${escapeHtml(text)}</p>${tool ? `<div class="tool">${escapeHtml(tool)}</div>` : ""}
    </article>`).join("")}</div>`;
}

function sessionRows(list, query) {
  const rows = [];
  for (const s of list) {
    const m = fuzzy(query, s.name);
    if (!m) continue;
    rows.push({ ...s, html: m.html, score: m.score });
  }
  rows.sort((a, b) => b.score - a.score);
  if (rows.length === 0) return `<div class="sheet-empty">No sessions match "${escapeHtml(query)}".</div>`;
  return rows.map((s) => `
    <button class="row-item" data-session="${s.id}" data-current="${s.id === session().id}">
      <span class="status-dot" style="background:${STATUS_COLOR[s.status]}"></span>
      <span class="grow"><span>${s.html}</span><small>${s.updated} · ${s.status}</small></span>
    </button>`).join("");
}

function quickSwitchRows(query) {
  const rows = [];
  for (const s of recentSessions()) {
    const m = fuzzy(query, s.name);
    if (!m) continue;
    rows.push({ ...s, html: m.html, score: m.score });
  }
  rows.sort((a, b) => b.score - a.score);
  if (rows.length === 0) return `<div class="sheet-empty">No sessions match "${escapeHtml(query)}".</div>`;
  return rows.map((s) => `
    <button class="row-item" data-quick="${s.id}" data-quick-ws="${s.ws.id}">
      <span class="status-dot" style="background:${STATUS_COLOR[s.status]}"></span>
      <span class="grow"><span>${s.html}</span><small>${escapeHtml(s.ws.name)} · ${s.updated}</small></span>
      ${glyph("swap")}
    </button>`).join("");
}

function modelRows() {
  return MODELS.map(([id, hint]) => `
    <button class="row-item" data-model="${id}" data-current="${id === state.model}">
      ${glyph("cpu")}<span class="grow">${id}<small>${hint}</small></span>
    </button>`).join("");
}

function workspaceRows(query) {
  const rows = [];
  for (const w of WORKSPACES) {
    const m = fuzzy(query, w.name);
    if (!m) continue;
    rows.push({ w, html: m.html, score: m.score });
  }
  rows.sort((a, b) => b.score - a.score);
  if (rows.length === 0) return `<div class="sheet-empty">No workspaces match "${escapeHtml(query)}".</div>`;
  return rows.map(({ w, html }) => `
    <button class="row-item" data-workspace="${w.id}" data-current="${w.id === state.wsId}">
      ${wsIcon(w)}<span class="grow"><span>${html}</span><small>${escapeHtml(w.path)}</small></span>
    </button>`).join("");
}

function fuzzyBox(placeholder) {
  return `<div class="fuzzy">${glyph("search")}<input id="fuzzy-input" placeholder="${placeholder}" value="${escapeHtml(state.query)}" autocomplete="off" /></div>`;
}

function expansion() {
  if (!state.open) return "";
  const quad = state.variant === "B";
  const scrim = `<button class="scrim" data-close aria-label="Close"></button>`;
  if (state.open === "tl") {
    const cls = quad ? "quad-tl" : "from-left";
    return `${scrim}<section class="sheet ${cls}">
      <div class="sheet-head">${wsIcon(ws())}<h2>SESSIONS · ${escapeHtml(ws().name.toUpperCase())}</h2>
        <button class="row-item" data-open-workspaces style="padding:4px 8px">${glyph("grid")}</button></div>
      ${fuzzyBox("Fuzzy search sessions…")}
      <div class="sheet-list">${sessionRows(sessions(), state.query)}</div>
    </section>`;
  }
  if (state.open === "tl-ws") {
    const cls = quad ? "quad-tl" : "from-left";
    return `${scrim}<section class="sheet ${cls}">
      <div class="sheet-head">${glyph("grid")}<h2>WORKSPACES</h2></div>
      ${fuzzyBox("Fuzzy search workspaces…")}
      <div class="sheet-list">${workspaceRows(state.query)}</div>
    </section>`;
  }
  if (state.open === "tr") {
    if (quad) {
      return `${scrim}<section class="sheet quad-tr">
        <div class="sheet-head">${glyph(state.tool)}<h2>WORKSPACE TOOLS</h2></div>
        ${fuzzyBox("Fuzzy search files…")}
        <div class="sheet-list">${TOOLS.map(([id, label]) => `
          <button class="row-item" data-tool="${id}" data-current="${id === state.tool}">${glyph(id)}<span class="grow">${label}</span></button>`).join("")}
        </div>
      </section>`;
    }
    return `${scrim}<nav class="strip-v" aria-label="Tools">${TOOLS.map(([id, label]) => `
      <button data-tool="${id}" data-current="${id === state.tool}" aria-label="${label}">${glyph(id)}</button>`).join("")}
    </nav>`;
  }
  if (state.open === "status") {
    return `${scrim}<div class="strip-h left-id">
      <button aria-label="Tasks">${glyph("agent")}</button>
      <span class="grow" style="font-size:11px;color:var(--muted)">Task 3/5 · Build the responsive surfaces</span>
      <span class="meta"><span><b>24.1k</b> / 200k</span><span>runner 0/26</span></span>
    </div>`;
  }
  if (state.open === "bl") {
    const cls = quad ? "quad-tl" : "from-left";
    return `${scrim}<section class="sheet ${cls}">
      <div class="sheet-head">${glyph("swap")}<h2>QUICK SWITCH · RECENT</h2></div>
      ${fuzzyBox("Fuzzy search sessions…")}
      <div class="sheet-list">${quickSwitchRows(state.query)}</div>
    </section>`;
  }
  return "";
}

const RADIAL_ACTIONS = [
  ["clip", "Attach", "file · image · bundle"],
  ["mic", "Voice", "hold to talk"],
  ["cpu", "Model", "switch model"],
  ["branch", "Fork", "branch the session"],
];

function radialMenu() {
  if (!state.hold || state.hold.kind !== "radial") return "";
  const btn = document.querySelector(`.corner-btn[data-corner="${state.hold.corner}"]`);
  if (!btn) return "";
  const rect = btn.getBoundingClientRect();
  const root = document.getElementById("screen").getBoundingClientRect();
  const cx = rect.left + rect.width / 2 - root.left;
  const cy = rect.top + rect.height / 2 - root.top;
  const { items, sel } = state.hold;
  const start = -90; const spread = 100; const radius = 96;
  const step = items.length > 1 ? spread / (items.length - 1) : 0;
  const tiles = items.map((item, i) => {
    const angle = ((start + i * step) * Math.PI) / 180;
    const x = cx + Math.cos(angle) * radius;
    const y = cy + Math.sin(angle) * radius;
    return `<div class="radial-item" data-sel="${i === sel}" style="left:${Math.round(x)}px;top:${Math.round(y)}px">${glyph(item.glyph)}</div>`;
  }).join("");
  const current = items[sel];
  const lx = cx + Math.cos(((start + sel * step) * Math.PI) / 180) * (radius + 46);
  const ly = cy + Math.sin(((start + sel * step) * Math.PI) / 180) * (radius + 46);
  return `<div class="radial" id="hold-menu">${tiles}
    <div class="radial-label" style="left:${Math.round(lx)}px;top:${Math.round(ly)}px">${current.label}<small>RELEASE TO ${current.sub ?? "SELECT"}</small></div>
  </div>`;
}

function holdMenu() {
  if (!state.hold) return "";
  const { kind, items, sel } = state.hold;
  if (kind === "radial") return radialMenu();
  if (kind === "fan") {
    return `<div class="fan" id="hold-menu">
      <div class="fan-hint">QUICK SWITCH · RELEASE TO OPEN</div>
      ${items.map((s, i) => `<div class="fan-card" data-sel="${i === sel}"><b>${escapeHtml(s.name)}</b><small>${escapeHtml(s.ws.name)} · ${s.updated}</small></div>`).join("")}
    </div>`;
  }
  const at = kind === "tools" ? "at-top" : state.hold.corner === "tl" ? "at-top" : "at-bottom";
  const current = items[sel];
  return `<div class="mgs ${at}" id="hold-menu">
    <div class="mgs-row">${items.map((item, i) => `<div class="mgs-item" data-sel="${i === sel}">${item.face}</div>`).join("")}</div>
    <div class="mgs-label">${escapeHtml(current.label)}<small>${escapeHtml(current.sub ?? "RELEASE TO EQUIP")}</small></div>
  </div>`;
}

function modelsSheet() {
  return `<button class="scrim" data-close aria-label="Close"></button><section class="sheet from-bottom">
    <div class="sheet-head">${glyph("cpu")}<h2>MODEL</h2></div>
    <div class="sheet-list">${modelRows()}</div>
  </section>`;
}

function renderPhone() {
  stage.classList.remove("desktop-mode");
  stage.innerHTML = `<div class="phone"><div class="notch"></div><div class="screen" id="screen">
    ${topBar()}${chat()}${expansion()}${state.open === "models" ? modelsSheet() : ""}${holdMenu()}${composerRow()}${state.toast ? `<div class="toast">${state.toast}</div>` : ""}${bottomBar()}
  </div></div>`;
}

function renderDesktop() {
  stage.classList.add("desktop-mode");
  const left = state.pinned.left || state.open === "tl";
  const right = state.pinned.right || state.open === "tr";
  stage.innerHTML = `<div class="bp ${state.classic ? "classic" : ""}" id="screen">
    ${topBar()}
    <button class="split-toggle" data-split>${state.classic ? "◧ CORNER MODE" : "◫ 3-WAY SPLIT"}</button>
    <div class="bp-main">
      <div class="bp-edge left" data-open="${left || state.classic}">
        <div class="sheet-head">${wsIcon(ws())}<h2>SESSIONS</h2>
          <button class="pin-btn" data-pin="left" aria-pressed="${state.pinned.left}" aria-label="Pin">${glyph("pin")}</button></div>
        ${fuzzyBox("Fuzzy search sessions…")}
        <div class="sheet-list">${sessionRows(sessions(), state.query)}</div>
      </div>
      <div class="bp-center">${chat()}</div>
      <div class="bp-edge right" data-open="${right || state.classic}">
        <div class="sheet-head">${glyph("files")}<h2>FILES</h2>
          <button class="pin-btn" data-pin="right" aria-pressed="${state.pinned.right}" aria-label="Pin">${glyph("pin")}</button></div>
        <div class="sheet-list">${TOOLS.map(([id, label]) => `
          <button class="row-item" data-tool="${id}" data-current="${id === state.tool}">${glyph(id)}<span class="grow">${label}</span></button>`).join("")}
        </div>
      </div>
    </div>
    ${holdMenu()}${composerRow()}${state.toast ? `<div class="toast">${state.toast}</div>` : ""}${bottomBar()}
  </div>`;
}

function render() {
  if (state.variant === "D") renderDesktop(); else renderPhone();
  bindShell();
  const prompt = document.getElementById("prompt-input");
  if (prompt) {
    prompt.addEventListener("input", () => { state.input = prompt.value; });
    prompt.addEventListener("keydown", (event) => {
      if (event.key === "Enter") sendMessage();
    });
  }
  const chatScroll = document.getElementById("chat-scroll");
  if (chatScroll) chatScroll.scrollTop = chatScroll.scrollHeight;
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

/* ---------------- gestures ---------------- */

const HOLD_MS = 320;
let holdTimer = null;
let pressedCorner = null;

function holdItemsFor(corner) {
  if (corner === "tl") {
    return {
      kind: "ws",
      items: WORKSPACES.map((w) => ({ face: wsIcon(w), label: w.name, id: w.id })),
      sel: WORKSPACES.findIndex((w) => w.id === state.wsId),
    };
  }
  if (corner === "tr") {
    return {
      kind: "tools",
      items: TOOLS.map(([id, label]) => ({ face: glyph(id), label, id })),
      sel: TOOLS.findIndex(([id]) => id === state.tool),
    };
  }
  if (corner === "br") {
    return {
      kind: "radial",
      items: RADIAL_ACTIONS.map(([g, label, sub]) => ({ glyph: g, label, sub, id: g })),
      sel: 0,
    };
  }
  return { kind: "fan", items: recentSessions(), sel: 0 };
}

function tapAction(corner) {
  if (corner === "br") { sendMessage(); return; }
  state.query = "";
  state.open = state.open === corner ? null : corner;
  render();
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
  const { kind, items, sel, corner } = state.hold;
  state.hold = null;
  if (kind === "radial") {
    const action = items[sel];
    if (action.id === "cpu") { state.open = "models"; state.query = ""; }
    else if (action.id === "clip") showToast("<b>Attach</b> — probe: file/context picker would open.");
    else if (action.id === "mic") showToast("<b>Voice</b> — probe: hold-to-talk would engage.");
    else if (action.id === "branch") showToast("<b>Fork</b> — probe: fork confirmation would open.");
  } else if (kind === "ws") { state.wsId = items[sel].id; state.open = null; }
  else if (kind === "tools") { state.tool = items[sel].id; state.open = null; }
  else if (kind === "fan") {
    const target = items[sel];
    state.wsId = target.ws.id;
    state.sessionByWs[target.ws.id] = target.id;
    state.open = null;
  }
  render();
}

function moveSelection(event) {
  if (!state.hold) return;
  const menu = document.getElementById("hold-menu");
  if (!menu) return;
  if (state.hold.kind === "radial") {
    const tiles = [...menu.querySelectorAll(".radial-item")];
    let best = state.hold.sel; let bestDist = Number.POSITIVE_INFINITY;
    tiles.forEach((tile, i) => {
      const rect = tile.getBoundingClientRect();
      const dx = event.clientX - (rect.left + rect.width / 2);
      const dy = event.clientY - (rect.top + rect.height / 2);
      const dist = Math.hypot(dx, dy);
      if (dist < bestDist) { bestDist = dist; best = i; }
    });
    if (best !== state.hold.sel) { state.hold.sel = best; render(); }
    return;
  }
  if (state.hold.kind === "fan") {
    const cards = [...menu.querySelectorAll(".fan-card")];
    let best = state.hold.sel;
    cards.forEach((card, i) => {
      const rect = card.getBoundingClientRect();
      if (event.clientY >= rect.top - 4 && event.clientY <= rect.bottom + 4) best = i;
    });
    if (best !== state.hold.sel) { state.hold.sel = best; render(); }
    return;
  }
  const tiles = [...menu.querySelectorAll(".mgs-item")];
  let best = state.hold.sel; let bestDist = Number.POSITIVE_INFINITY;
  tiles.forEach((tile, i) => {
    const rect = tile.getBoundingClientRect();
    const dist = Math.abs(event.clientX - (rect.left + rect.width / 2));
    if (dist < bestDist) { bestDist = dist; best = i; }
  });
  if (best !== state.hold.sel) { state.hold.sel = best; render(); }
}

function bindShell() {
  const screen = document.getElementById("screen");
  if (!screen) return;

  for (const btn of screen.querySelectorAll(".corner-btn")) {
    btn.addEventListener("pointerdown", (event) => {
      event.preventDefault();
      const corner = btn.dataset.corner;
      pressedCorner = corner;
      const holdOpens = !state.vocabInverted;
      const startHold = () => {
        state.hold = { corner, ...holdItemsFor(corner) };
        render();
      };
      if (holdOpens) holdTimer = setTimeout(startHold, HOLD_MS);
      else startHold(), holdTimer = null; /* inverted: press opens carousel immediately, tap commits below */
    });
  }

  /* drag tracking lives on window: re-renders during a drag replace the
     pressed button (implicit pointer capture) but never the window. */
  if (!window.__cmDragBound) {
    window.__cmDragBound = true;
    window.addEventListener("pointermove", moveSelection);
    window.addEventListener("pointerup", () => {
      if (holdTimer) { clearTimeout(holdTimer); holdTimer = null; }
      if (state.hold) { commitHold(); pressedCorner = null; return; }
      if (pressedCorner) {
        const corner = pressedCorner; pressedCorner = null;
        tapAction(corner);
      }
    });
    window.addEventListener("pointercancel", () => {
      if (holdTimer) { clearTimeout(holdTimer); holdTimer = null; }
      state.hold = null; pressedCorner = null; render();
    });
  }

  screen.addEventListener("click", (event) => {
    const target = event.target.closest("[data-close],[data-session],[data-workspace],[data-tool],[data-open-workspaces],[data-pin],[data-split],[data-open-status],[data-quick],[data-model]");
    if (!target) return;
    if (target.dataset.close !== undefined) { state.open = null; state.query = ""; }
    else if (target.dataset.openStatus !== undefined) { state.query = ""; state.open = state.open === "status" ? null : "status"; }
    else if (target.dataset.quick) {
      state.wsId = target.dataset.quickWs;
      state.sessionByWs[state.wsId] = target.dataset.quick;
      state.open = null; state.query = "";
    }
    else if (target.dataset.model) { state.model = target.dataset.model; state.open = null; }
    else if (target.dataset.session) { state.sessionByWs[state.wsId] = target.dataset.session; state.open = null; state.query = ""; }
    else if (target.dataset.workspace) { state.wsId = target.dataset.workspace; state.open = "tl"; state.query = ""; }
    else if (target.dataset.tool) { state.tool = target.dataset.tool; if (state.variant !== "D") state.open = null; }
    else if (target.dataset.openWorkspaces !== undefined) { state.open = "tl-ws"; state.query = ""; }
    else if (target.dataset.pin) { const side = target.dataset.pin; state.pinned[side] = !state.pinned[side]; }
    else if (target.dataset.split !== undefined) { state.classic = !state.classic; }
    render();
  });
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
  vocab.textContent = state.vocabInverted ? "press=carousel · release=equip" : "tap=open · hold=carousel";
  document.getElementById("variant-note").innerHTML = VARIANTS[state.variant].note;
}

document.getElementById("vocab-toggle").addEventListener("click", () => {
  state.vocabInverted = !state.vocabInverted;
  state.open = null; state.hold = null;
  render();
});
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

render();

window.__cmState = state;
