const workdirs = [
  {
    id: "oqto", commits: 214, branches: 9, name: "Oqto", path: "~/byteowlz/oqto_refactor", icon: "assets/oqto-icon.webp", banner: "assets/oqto-banner.webp",
    accent: "#3ba77c", glow: "transparent", tagline: "Agent workspace platform", summary: "Rebuilding the agent workspace around durable Sessions, shared files, and portable OqtoUI Views.", branch: "feat/container-placement-production", files: 47,
    sessions: [
      { title: "OqtoUI architecture", blurb: "Portable Views, layout contracts, and native client boundaries.", status: "active", age: "now", progress: 72, agent: "Pi", messages: 184 },
      { title: "Big Picture overview", blurb: "Explore workspace rows and episode-like Session navigation.", status: "active", age: "8m", progress: 38, agent: "Pi", messages: 32 },
      { title: "Session identity repair", blurb: "Public identity mapping and durable oqto-log convergence.", status: "waiting", age: "2h", progress: 64, agent: "Claude", messages: 243 },
      { title: "File watcher recovery", blurb: "Generation-aware snapshots, overflow handling, and resync.", status: "done", age: "yesterday", progress: 100, agent: "Pi", messages: 96 },
      { title: "Interactive output ADR", blurb: "Declarative controls and sandboxed MCP Apps presentation.", status: "done", age: "2d", progress: 100, agent: "Claude", messages: 128 }
    ]
  },
  {
    id: "mmry", commits: 58, branches: 3, name: "mmry", path: "~/byteowlz/mmry", icon: "assets/mmry-icon.webp", banner: "assets/mmry-banner.webp",
    accent: "#3ba7a0", glow: "transparent", tagline: "Durable agent memory", summary: "Structured memory authority for agents, with retrieval, provenance, and carefully bounded write paths.", branch: "main", files: 12,
    sessions: [
      { title: "Hybrid retrieval tuning", blurb: "Balance semantic and lexical results across real memory corpora.", status: "active", age: "24m", progress: 56, agent: "Pi", messages: 77 },
      { title: "Memory provenance", blurb: "Expose source, confidence, and lifecycle without noisy output.", status: "waiting", age: "3h", progress: 44, agent: "Claude", messages: 51 },
      { title: "CLI output contract", blurb: "Stable machine-readable results and useful terminal defaults.", status: "done", age: "1d", progress: 100, agent: "Pi", messages: 109 }
    ]
  },
  {
    id: "eavs", commits: 87, branches: 5, name: "eavs", path: "~/byteowlz/eavs", icon: "assets/eavs-icon.webp", banner: "assets/eavs-banner.webp",
    accent: "#5b8fc9", glow: "transparent", tagline: "Model access and egress", summary: "One local endpoint for model discovery, policy-aware access, aliases, and traceable egress.", branch: "feat/discovery-v2", files: 23,
    sessions: [
      { title: "Provider discovery", blurb: "Zero-config localhost discovery with explicit graceful fallback.", status: "active", age: "41m", progress: 81, agent: "Pi", messages: 61 },
      { title: "Egress policy traces", blurb: "Make allow, deny, and enforcement evidence inspectable.", status: "error", age: "4h", progress: 28, agent: "Claude", messages: 88 },
      { title: "Alias resolution", blurb: "Canonical model aliases across providers and clients.", status: "done", age: "3d", progress: 100, agent: "Pi", messages: 47 },
      { title: "iOS client handshake", blurb: "Native discovery behavior under constrained networking.", status: "waiting", age: "5d", progress: 16, agent: "Pi", messages: 29 }
    ]
  },
  {
    id: "foxline", commits: 41, branches: 2, name: "foxline", path: "~/byteowlz/foxline", icon: "assets/foxline-icon.webp", banner: "assets/foxline-banner.webp",
    accent: "#c98a5a", glow: "transparent", tagline: "Voice-native agent line", summary: "A low-latency voice gateway for natural ongoing conversations with agent runtimes.", branch: "main", files: 8,
    sessions: [
      { title: "Turn detection", blurb: "Reduce interruption errors while keeping response latency low.", status: "waiting", age: "2h", progress: 67, agent: "Pi", messages: 139 },
      { title: "Voice gateway metrics", blurb: "Trace capture and percentile dashboards for the full audio path.", status: "done", age: "1d", progress: 100, agent: "Claude", messages: 82 },
      { title: "Mobile audio session", blurb: "Recover cleanly from route changes and interruptions.", status: "done", age: "4d", progress: 100, agent: "Pi", messages: 73 }
    ]
  },
  {
    id: "h8", commits: 66, branches: 4, name: "h8", path: "~/byteowlz/h8", icon: "assets/h8-icon.webp", banner: "assets/h8-banner.webp",
    accent: "#d96b69", glow: "transparent", tagline: "Hunk-based code review", summary: "Fast keyboard-first code review with agent collaboration, precise comments, and durable review state.", branch: "feat/review-canvas", files: 19,
    sessions: [
      { title: "Review navigation", blurb: "Make file, hunk, and comment movement spatially predictable.", status: "active", age: "1h", progress: 49, agent: "Claude", messages: 94 },
      { title: "Inline agent comments", blurb: "Typed review comments with stable anchors through reloads.", status: "active", age: "6h", progress: 74, agent: "Pi", messages: 118 },
      { title: "Large diff profiling", blurb: "Keep navigation instant across generated and vendored files.", status: "done", age: "2d", progress: 100, agent: "Pi", messages: 45 }
    ]
  },
  {
    id: "tmz", commits: 23, branches: 2, name: "tmz", path: "~/byteowlz/tmz", icon: "assets/tmz-icon.webp", banner: "assets/tmz-banner.webp",
    accent: "#a78bd0", glow: "transparent", tagline: "Agent terminal multiplexing", summary: "Organize long-running terminal and agent processes into calm, resumable workspaces.", branch: "main", files: 5,
    sessions: [
      { title: "Pane recovery", blurb: "Restore process attachment without duplicating ownership.", status: "waiting", age: "3h", progress: 35, agent: "Pi", messages: 64 },
      { title: "Session labels", blurb: "Derive useful labels while preserving explicit user names.", status: "done", age: "6d", progress: 100, agent: "Claude", messages: 36 }
    ]
  },
  {
    id: "lst", commits: 49, branches: 3, name: "lst", path: "~/byteowlz/lst", icon: "assets/lst-icon.webp", banner: "assets/lst-banner.webp",
    accent: "#d4c275", glow: "transparent", tagline: "Lists without ceremony", summary: "A focused list and capture tool with fast native clients and simple synchronization.", branch: "mobile", files: 14,
    sessions: [
      { title: "iOS capture flow", blurb: "One-handed entry with reliable optimistic synchronization.", status: "active", age: "5h", progress: 88, agent: "Pi", messages: 53 },
      { title: "Conflict semantics", blurb: "Make simultaneous edits unsurprising and recoverable.", status: "done", age: "1w", progress: 100, agent: "Claude", messages: 71 }
    ]
  }
];

const concepts = {
  matrix: { number: "01", title: "Spatial Matrix", summary: "Work directories are vertical workspaces. Sessions run horizontally like episodes.", strength: "Best spatial memory", risk: "Risk: wide rows", best: "Keyboard, TV, many parallel projects", idea: "Niri-like two-axis navigation", question: "Can users retain row and episode position?" },
  cinema: { number: "02", title: "Project Cinema", summary: "A selected work directory becomes the hero; Sessions read as seasons and episodes.", strength: "Strong identity + assets", risk: "Risk: less overview", best: "Big screens, branded workspaces", idea: "Streaming-library familiarity", question: "Does the hero consume too much working context?" },
  focus: { number: "03", title: "Focus + Filmstrip", summary: "A compact directory switcher keeps one project and its Session timeline in focus.", strength: "Fast project switching", risk: "Risk: weaker global scan", best: "Desktop, touch, medium screens", idea: "Stable navigator + deep selected context", question: "Is one-directory focus too restrictive?" },
  atlas: { number: "04", title: "Activity Atlas", summary: "Work directories claim space according to urgency, recency, and active work.", strength: "Excellent supervision", risk: "Risk: unstable positions", best: "Overview, triage, blocked work", idea: "Information-weighted workspace map", question: "Does dynamic ranking harm spatial memory?" },
  anchor: { number: "05", title: "Anchor Carousel", summary: "Projects travel vertically through one fixed focus band; Sessions move horizontally inside it.", strength: "Stable focus position", risk: "Risk: one project dominates", best: "Wheel, touch, gamepad, TV", idea: "Fixed viewport focus + moving project rail", question: "Does a constant anchor improve orientation?" },
  hybrid: { number: "06", title: "Stage + Rail", summary: "A stats hero for the selected repo on top; below, an analog rail of identical-height rows whose selected row carries Session tiles.", strength: "Natural continuous scroll", risk: "Risk: split attention", best: "Desktop, TV, quick supervision", idea: "Cinema identity + uniform anchor rail", question: "Do equal-height rows make the wheel feel analog?" }
};

let currentConcept = "matrix";
let activeWorkdir = 0;
let navRow = 0;
let navCol = 0;
let searchIndex = 0;
const root = document.querySelector("#concept-root");
const peekDialog = document.querySelector("#peek-dialog");
const searchDialog = document.querySelector("#search-dialog");
const notesDialog = document.querySelector("#notes-dialog");

const escape = (value) => String(value).replace(/[&<>'"]/g, character => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", "'": "&#39;", '"': "&quot;" })[character]);
const cap = value => value.charAt(0).toUpperCase() + value.slice(1);
const allSessions = () => workdirs.flatMap((workdir, row) => workdir.sessions.map((session, col) => ({ workdir, session, row, col })));
const activeCount = workdir => workdir.sessions.filter(session => session.status === "active" || session.status === "waiting").length;
const status = value => `<span class="status-label"><i class="status-dot ${value}"></i>${cap(value)}</span>`;
const progress = value => `<div class="progress" style="--progress:${value}%"><i></i></div>`;
const agentStack = session => `<span class="agent-stack" aria-label="Agent ${escape(session.agent)}"><span>${escape(session.agent.slice(0, 1))}</span>${session.status === "active" ? "<span>+</span>" : ""}</span>`;

function sessionButton(workdir, session, row, col, className, index) {
  return `<button type="button" class="tile-button ${className}" data-row="${row}" data-col="${col}" data-kind="session" aria-label="${escape(workdir.name)}, Session ${index + 1}: ${escape(session.title)}">
    <div><div class="card-top"><span class="session-index">S${String(index + 1).padStart(2, "0")}</span>${status(session.status)}</div>
    <h3>${escape(session.title)}</h3><p>${escape(session.blurb)}</p></div>
    <div>${progress(session.progress)}<div class="card-foot"><span class="session-meta">${escape(session.age)} · ${session.messages} msgs</span>${agentStack(session)}</div></div>
  </button>`;
}

function matrixView() {
  return `<section class="matrix-view" aria-label="Work directories and Sessions matrix">${workdirs.map((workdir, row) => `
    <article class="matrix-row" aria-labelledby="matrix-${workdir.id}">
      <header class="matrix-workdir"><div><img class="logo-img banner" src="${workdir.banner}" alt="" /><h2 id="matrix-${workdir.id}">${escape(workdir.name)}</h2><p>${escape(workdir.path)}</p></div><div class="matrix-workdir-foot">${status(activeCount(workdir) ? "active" : "done")}<span>${workdir.sessions.length} Sessions</span></div></header>
      <div class="matrix-track" data-track="${row}">
        <button type="button" class="tile-button matrix-card overview" style="--card-accent:${workdir.accent}" data-row="${row}" data-col="0" data-kind="overview" aria-label="Open ${escape(workdir.name)} overview">
          <div><div class="card-top"><img class="overview-icon" src="${workdir.icon}" alt="" /><span class="session-index">OVERVIEW</span></div><h3>${escape(workdir.tagline)}</h3><p>${escape(workdir.summary)}</p></div>
          <div class="card-foot"><span class="session-meta">${workdir.files} changed files</span><span aria-hidden="true">↗</span></div>
        </button>
        ${workdir.sessions.map((session, col) => sessionButton(workdir, session, row, col + 1, "matrix-card", col)).join("")}
      </div>
    </article>`).join("")}</section>`;
}

function cinemaView() {
  const selected = workdirs[activeWorkdir];
  return `<section class="cinema-view" aria-label="Project cinema">
    <header class="cinema-hero" style="--hero-glow:${selected.glow};--hero-logo:url('${selected.icon}')">
      <div class="hero-content"><span class="eyebrow">NOW IN FOCUS · ${escape(selected.path)}</span><img class="logo-img hero-banner" src="${selected.banner}" alt="${escape(selected.name)}" />
      <h2>${escape(selected.tagline)}</h2><p>${escape(selected.summary)}</p><div class="hero-actions"><button type="button" class="primary-action" data-open-overview="${activeWorkdir}">Open workspace</button><button type="button" class="secondary-action" data-cycle-workdir>Next project →</button></div></div>
    </header>
    <div class="cinema-shelves">${workdirs.map((workdir, row) => `<article class="cinema-shelf"><header class="shelf-head"><h3>${escape(workdir.name)}</h3><span>${activeCount(workdir)} in progress · ${workdir.sessions.length} Sessions</span></header>
      <div class="shelf-track" data-track="${row}">
        <button type="button" class="tile-button shelf-card overview" style="background-image:linear-gradient(90deg,rgba(22,38,36,.97),rgba(22,38,36,.72)),url('${workdir.icon}')" data-row="${row}" data-col="0" data-kind="overview"><div><span class="episode">WORKSPACE HOME</span><h4>${escape(workdir.tagline)}</h4></div><div class="card-foot">${status(activeCount(workdir) ? "active" : "done")}<span>Open ↗</span></div></button>
        ${workdir.sessions.map((session, col) => `<button type="button" class="tile-button shelf-card" data-row="${row}" data-col="${col + 1}" data-kind="session"><div><div class="card-top"><span class="episode">EPISODE ${col + 1}</span>${status(session.status)}</div><h4>${escape(session.title)}</h4><span class="session-meta">${escape(session.blurb)}</span></div><div class="card-foot"><span>${session.progress}%</span><span>${escape(session.age)}</span></div></button>`).join("")}
      </div></article>`).join("")}</div>
  </section>`;
}

function focusView() {
  const selected = workdirs[activeWorkdir];
  return `<section class="focus-view" aria-label="Focused work directory and Session filmstrip">
    <aside class="focus-sidebar" aria-label="Work directories">${workdirs.map((workdir, row) => `<button type="button" class="focus-directory" data-select-workdir="${row}" aria-selected="${row === activeWorkdir}"><img src="${workdir.icon}" alt="" /><span><strong>${escape(workdir.name)}</strong><small>${activeCount(workdir)} active · ${workdir.sessions.length} total</small></span><span class="count">${String(row + 1).padStart(2, "0")}</span></button>`).join("")}</aside>
    <div class="focus-stage">
      <header class="focus-head" style="--stage-glow:${selected.glow}"><div class="focus-head-copy"><span class="eyebrow">${escape(selected.path)}</span><img class="logo-img banner" src="${selected.banner}" alt="${escape(selected.name)}" /><h2>${escape(selected.tagline)}</h2><p>${escape(selected.summary)}</p></div><img class="giant-icon" src="${selected.icon}" alt="" /></header>
      <div class="focus-section-title"><h3>Session filmstrip</h3><span>← RECENT ${selected.sessions.length} SESSIONS OLDER →</span></div>
      <div class="filmstrip" data-track="${activeWorkdir}">
        <button type="button" class="tile-button film-card" data-row="${activeWorkdir}" data-col="0" data-kind="overview"><div><span class="film-number">00</span><h4>Workspace overview</h4><p>${escape(selected.summary)}</p></div><div class="card-foot"><span class="session-meta">${escape(selected.branch)}</span><span>Open ↗</span></div></button>
        ${selected.sessions.map((session, col) => `<button type="button" class="tile-button film-card" data-row="${activeWorkdir}" data-col="${col + 1}" data-kind="session"><div><div class="card-top"><span class="film-number">${String(col + 1).padStart(2, "0")}</span>${status(session.status)}</div><h4>${escape(session.title)}</h4><p>${escape(session.blurb)}</p></div><div>${progress(session.progress)}<div class="card-foot"><span class="session-meta">${session.messages} messages</span><span>${escape(session.age)}</span></div></div></button>`).join("")}
      </div>
      <div class="focus-activity"><div class="activity-stat"><strong>${activeCount(selected)}</strong><span>NEED ATTENTION</span></div><div class="activity-stat"><strong>${selected.sessions.reduce((sum, item) => sum + item.messages, 0)}</strong><span>MESSAGES</span></div><div class="activity-stat"><strong>${selected.files}</strong><span>CHANGED FILES</span></div></div>
    </div>
  </section>`;
}

function atlasView() {
  const ordered = [...workdirs].sort((a, b) => activeCount(b) - activeCount(a));
  return `<section class="atlas-view" aria-label="Activity-weighted workspace atlas"><header class="atlas-toolbar"><div><span class="eyebrow">SORTED BY ATTENTION</span><h2>What needs you?</h2></div><div class="atlas-legend"><span><i class="status-dot active"></i>Working</span><span><i class="status-dot waiting"></i>Waiting</span><span><i class="status-dot error"></i>Blocked</span></div></header>
    <div class="atlas-grid">${ordered.map((workdir, atlasIndex) => {
      const row = workdirs.indexOf(workdir); const attention = activeCount(workdir); const cls = atlasIndex === 0 ? "hero" : atlasIndex === 1 ? "wide" : atlasIndex > 4 ? "compact" : atlasIndex === 2 ? "tall" : "";
      return `<button type="button" class="tile-button atlas-card ${cls}" style="--atlas-glow:${workdir.glow}" data-row="${row}" data-col="0" data-kind="overview"><img class="watermark" src="${workdir.icon}" alt="" /><div><div class="card-top"><span class="eyebrow">${escape(workdir.name)}</span>${status(attention ? "active" : "done")}</div><h3>${escape(workdir.tagline)}</h3><p>${escape(workdir.summary)}</p></div><div>${cls === "hero" ? `<div class="session-list">${workdir.sessions.slice(0,3).map(session => `<span class="session-line"><span>${escape(session.title)}</span><span>${escape(session.age)}</span></span>`).join("")}</div>` : `<div class="metric">${attention}</div><div class="metric-label">ITEMS NEED ATTENTION · ${workdir.sessions.length} SESSIONS</div>`}</div></button>`;
    }).join("")}</div>
  </section>`;
}

const compactViewport = () => window.matchMedia("(max-width: 620px)").matches;

function anchorView() {
  const selected = workdirs[activeWorkdir];
  const compact = compactViewport();
  const baseOffset = 170;
  const maxNeighbors = 3;
  return `<section class="anchor-view" aria-label="Fixed-focus vertical project carousel">
    <div class="anchor-axis" aria-hidden="true"><span>${String(activeWorkdir + 1).padStart(2, "0")}</span><i></i><small>${String(workdirs.length).padStart(2, "0")}</small></div>
    <div class="anchor-stage" data-anchor-stage tabindex="0" aria-label="Scroll vertically between projects; use left and right for Sessions">
      <div class="anchor-focus-frame" aria-hidden="true"><span>FOCUS</span></div>
      ${workdirs.map((workdir, row) => {
        const distance = row - activeWorkdir;
        const y = distance * baseOffset;
        if (Math.abs(distance) > maxNeighbors) return "";
        const total = workdir.sessions.length;
        if (distance !== 0) return `<button type="button" class="anchor-project compact-project" style="--anchor-y:${y}px;--anchor-opacity:${Math.max(.18, 1 - Math.abs(distance) * .22)}" data-anchor-select="${row}" aria-label="Focus ${escape(workdir.name)}"><img src="${workdir.banner}" alt="" /><span class="mini-episodes">${workdir.sessions.slice(0, 4).map((session, col) => `<span class="mini-episode"><i class="status-dot ${session.status}"></i>E${total - col}</span>`).join("")}</span><span class="anchor-project-meta">${activeCount(workdir)} active</span></button>`;
        return `<article class="anchor-project focused-project" style="--anchor-y:0px;--anchor-accent:${workdir.accent};--anchor-glow:${workdir.glow}" aria-labelledby="anchor-${workdir.id}">
          <button type="button" class="anchor-identity" data-open-overview="${row}" aria-label="Open ${escape(workdir.name)} overview"><img src="${workdir.banner}" alt="" /><h2 id="anchor-${workdir.id}">${escape(workdir.name)}</h2><span class="anchor-identity-meta">${escape(workdir.branch)}</span><span class="anchor-identity-open">Overview ↗</span></button>
          <div class="anchor-session-track" data-track="${row}">
            <button type="button" class="tile-button anchor-session stats-tile" data-row="${row}" data-col="0" data-kind="overview" aria-label="${escape(workdir.name)} statistics"><div class="card-top"><span class="session-index">STATS</span>${status(activeCount(workdir) ? "active" : "done")}</div><dl class="stats-list"><div><dt>Sessions</dt><dd>${total} <small>· ${activeCount(workdir)} active</small></dd></div><div><dt>Commits</dt><dd>${workdir.commits} <small>· ${workdir.branches} branches</small></dd></div><div><dt>Last message</dt><dd>${escape(workdir.sessions[0].age)}</dd></div><div><dt>Changed files</dt><dd>${workdir.files}</dd></div></dl></button>
            ${workdir.sessions.map((session, col) => `<button type="button" class="tile-button anchor-session" data-row="${row}" data-col="${col + 1}" data-kind="session"><div class="card-top"><span class="session-index">E${total - col}${col === 0 ? " · LATEST" : ""}</span>${status(session.status)}</div><h3>${escape(session.title)}</h3><p>${escape(session.blurb)}</p>${progress(session.progress)}<div class="card-foot"><span>${escape(session.age)}</span><span>${session.progress}%</span></div></button>`).join("")}
          </div>
        </article>`;
      }).join("")}
    </div>
    <div class="anchor-instruction"><span>SCROLL / SWIPE</span><strong>Projects move.<br />Focus stays.</strong></div>
  </section>`;
}

function hybridView() {
  const selected = workdirs[activeWorkdir];
  const rowStep = 116;
  return `<section class="hybrid-view" aria-label="Repository stage with uniform project rail">
    <header class="hybrid-hero">
      <div class="hybrid-identity">
        <img class="logo-img" src="${selected.banner}" alt="${escape(selected.name)}" />
        <div><h2>${escape(selected.tagline)}</h2><p>${escape(selected.summary)}</p></div>
        <button type="button" class="secondary-action" data-open-overview="${activeWorkdir}">Open workspace ↗</button>
      </div>
      <dl class="hybrid-stats">
        <div><dt>Sessions</dt><dd>${selected.sessions.length} <small>· ${activeCount(selected)} active</small></dd></div>
        <div><dt>Commits</dt><dd>${selected.commits} <small>· ${selected.branches} branches</small></dd></div>
        <div><dt>Last message</dt><dd>${escape(selected.sessions[0].age)}</dd></div>
        <div><dt>Changed files</dt><dd>${selected.files}</dd></div>
        <div><dt>Branch</dt><dd class="hybrid-branch">${escape(selected.branch)}</dd></div>
        <div><dt>Path</dt><dd class="hybrid-branch">${escape(selected.path)}</dd></div>
      </dl>
    </header>
    <div class="hybrid-stage" data-anchor-stage tabindex="0" aria-label="Scroll vertically between projects; use left and right for Sessions">
      <div class="anchor-focus-frame" aria-hidden="true"><span>FOCUS</span></div>
      ${workdirs.map((workdir, row) => {
        const distance = row - activeWorkdir;
        if (Math.abs(distance) > 4) return "";
        const y = distance * rowStep;
        const total = workdir.sessions.length;
        if (distance !== 0) return `<button type="button" class="anchor-project hybrid-row" style="--anchor-y:${y}px;--anchor-opacity:${Math.max(.22, 1 - Math.abs(distance) * .2)}" data-anchor-select="${row}" aria-label="Focus ${escape(workdir.name)}"><img class="hybrid-row-logo" src="${workdir.banner}" alt="" /><span class="mini-episodes">${workdir.sessions.slice(0, 4).map((session, col) => `<span class="mini-episode"><i class="status-dot ${session.status}"></i>E${total - col}</span>`).join("")}</span><span class="anchor-project-meta">${activeCount(workdir)} active</span></button>`;
        return `<article class="anchor-project hybrid-row hybrid-row-focused" style="--anchor-y:0px" aria-labelledby="anchor-${workdir.id}"><img class="hybrid-row-logo" src="${workdir.banner}" alt="" id="anchor-${workdir.id}" /><div class="hybrid-session-track" data-track="${row}">${workdir.sessions.map((session, col) => `<button type="button" class="tile-button hybrid-session" data-row="${row}" data-col="${col + 1}" data-kind="session"><span class="card-top"><span class="session-index">E${total - col}</span>${status(session.status)}</span><strong>${escape(session.title)}</strong><span class="hybrid-session-meta">${escape(session.age)} · ${session.progress}%</span></button>`).join("")}</div></article>`;
      }).join("")}
    </div>
  </section>`;
}

function render() {
  root.innerHTML = currentConcept === "matrix" ? matrixView() : currentConcept === "cinema" ? cinemaView() : currentConcept === "focus" ? focusView() : currentConcept === "atlas" ? atlasView() : currentConcept === "anchor" ? anchorView() : hybridView();
  document.body.classList.toggle("concept-anchor", currentConcept === "anchor" || currentConcept === "hybrid");
  if (currentConcept === "anchor" || currentConcept === "hybrid") window.scrollTo(0, 0);
  const concept = concepts[currentConcept];
  document.querySelector("#concept-number").textContent = `CONCEPT ${concept.number}`;
  document.querySelector("#concept-title").textContent = concept.title;
  document.querySelector("#concept-summary").textContent = concept.summary;
  document.querySelectorAll("[data-concept]").forEach(button => button.setAttribute("aria-pressed", String(button.dataset.concept === currentConcept)));
  bindConceptEvents();
  markCurrent(false);
}

function bindConceptEvents() {
  root.querySelectorAll("[data-row][data-col]").forEach(button => {
    button.addEventListener("click", () => {
      navRow = Number(button.dataset.row); navCol = Number(button.dataset.col); activeWorkdir = navRow; markCurrent(false); openItem(navRow, navCol);
    });
    button.addEventListener("keydown", event => {
      if (event.key !== "Enter") return;
      event.preventDefault();
      button.click();
    });
  });
  root.querySelectorAll("[data-select-workdir]").forEach(button => button.addEventListener("click", () => {
    activeWorkdir = Number(button.dataset.selectWorkdir); navRow = activeWorkdir; navCol = 0; render();
  }));
  root.querySelectorAll("[data-anchor-select]").forEach(button => button.addEventListener("click", () => {
    activeWorkdir = Number(button.dataset.anchorSelect); navRow = activeWorkdir; navCol = 0; render();
  }));
  const anchorStage = root.querySelector("[data-anchor-stage]");
  const anchorSection = root.querySelector(".anchor-view, .hybrid-view");
  if (anchorStage && anchorSection) {
    const step = currentConcept === "hybrid" ? 116 : 170;
    const lastRow = workdirs.length - 1;
    let wheelLock = false;
    anchorSection.addEventListener("wheel", event => {
      if (Math.abs(event.deltaY) < 12 || wheelLock) return;
      event.preventDefault(); wheelLock = true;
      activeWorkdir = Math.max(0, Math.min(lastRow, activeWorkdir + Math.sign(event.deltaY)));
      navRow = activeWorkdir; navCol = 0; render();
      window.setTimeout(() => { wheelLock = false; }, 180);
    }, { passive: false });

    let dragStartY = null;
    let dragDelta = 0;
    let dragging = false;
    let suppressClick = false;
    let lastMove = null;
    let velocity = 0;
    const rubberBanded = delta => {
      const maxDown = activeWorkdir * step;
      const maxUp = -(lastRow - activeWorkdir) * step;
      if (delta > maxDown) return maxDown + (delta - maxDown) / 3;
      if (delta < maxUp) return maxUp + (delta - maxUp) / 3;
      return delta;
    };
    anchorStage.addEventListener("pointerdown", event => {
      dragStartY = event.clientY; dragDelta = 0; velocity = 0;
      lastMove = { y: event.clientY, time: event.timeStamp };
    });
    anchorStage.addEventListener("pointermove", event => {
      if (dragStartY === null) return;
      dragDelta = event.clientY - dragStartY;
      if (!dragging && Math.abs(dragDelta) > 8) { dragging = true; anchorStage.classList.add("dragging"); }
      if (!dragging) return;
      const elapsed = event.timeStamp - lastMove.time;
      if (elapsed > 0) velocity = (event.clientY - lastMove.y) / elapsed;
      lastMove = { y: event.clientY, time: event.timeStamp };
      anchorStage.style.setProperty("--drag", `${rubberBanded(dragDelta)}px`);
    });
    const settleDrag = () => {
      if (dragStartY === null) return;
      dragStartY = null;
      if (!dragging) return;
      dragging = false; suppressClick = true;
      let steps = Math.round(-dragDelta / step);
      if (steps === 0 && Math.abs(velocity) > 0.45 && Math.abs(dragDelta) > 24) steps = velocity < 0 ? 1 : -1;
      steps = Math.max(-activeWorkdir, Math.min(lastRow - activeWorkdir, steps));
      anchorStage.classList.remove("dragging");
      anchorStage.style.setProperty("--drag", `${-steps * step}px`);
      window.setTimeout(() => {
        activeWorkdir = Math.max(0, Math.min(lastRow, activeWorkdir + steps));
        navRow = activeWorkdir; navCol = 0; render();
      }, steps === 0 ? 190 : 170);
    };
    anchorStage.addEventListener("pointerup", settleDrag);
    anchorStage.addEventListener("pointercancel", settleDrag);
    anchorStage.addEventListener("click", event => {
      if (!suppressClick) return;
      suppressClick = false; event.preventDefault(); event.stopPropagation();
    }, true);
  }
  root.querySelectorAll("[data-open-overview]").forEach(button => button.addEventListener("click", () => openItem(Number(button.dataset.openOverview), 0)));
  root.querySelectorAll("[data-cycle-workdir]").forEach(button => button.addEventListener("click", () => { activeWorkdir = (activeWorkdir + 1) % workdirs.length; navRow = activeWorkdir; render(); }));
}

function markCurrent(shouldFocus = true) {
  root.querySelectorAll("[data-row][data-col]").forEach(button => button.classList.toggle("is-current", Number(button.dataset.row) === navRow && Number(button.dataset.col) === navCol));
  const current = root.querySelector(`[data-row="${navRow}"][data-col="${navCol}"]`);
  if (current && shouldFocus) { current.focus({ preventScroll: true }); current.scrollIntoView({ block: "nearest", inline: "center" }); }
}

function openItem(row, col) {
  const workdir = workdirs[row];
  const session = col > 0 ? workdir.sessions[col - 1] : null;
  document.querySelector("#peek-content").innerHTML = `<div class="peek-cover" style="--peek-glow:${workdir.glow}"><img src="${workdir.banner}" alt="${escape(workdir.name)}" /></div><div class="peek-body"><span class="eyebrow">${session ? `SESSION ${String(col).padStart(2, "0")} · ${escape(workdir.name)}` : `WORKSPACE OVERVIEW · ${escape(workdir.path)}`}</span><h2 id="peek-title">${escape(session ? session.title : workdir.tagline)}</h2><p>${escape(session ? session.blurb : workdir.summary)}</p><div class="peek-facts"><div><strong>${session ? `${session.progress}%` : workdir.sessions.length}</strong><span>${session ? "TASK PROGRESS" : "SESSIONS"}</span></div><div><strong>${session ? session.messages : activeCount(workdir)}</strong><span>${session ? "MESSAGES" : "NEED ATTENTION"}</span></div><div><strong>${session ? escape(session.agent) : workdir.files}</strong><span>${session ? "AGENT" : "CHANGED FILES"}</span></div><div><strong>${session ? escape(session.age) : escape(workdir.branch)}</strong><span>${session ? "LAST ACTIVITY" : "BRANCH"}</span></div></div><div class="peek-actions"><button class="primary-action" type="button">Enter ${session ? "Session" : "workspace"}</button><button class="secondary-action" type="button">Open in new View</button></div></div>`;
  peekDialog.showModal();
}

function setConcept(id) {
  if (!concepts[id]) return;
  currentConcept = id; navRow = activeWorkdir; navCol = 0; render(); root.focus({ preventScroll: true });
}

function moveNav(key) {
  if (currentConcept === "anchor" || currentConcept === "hybrid") {
    if (key === "ArrowUp" || key === "ArrowDown") {
      activeWorkdir = Math.max(0, Math.min(workdirs.length - 1, activeWorkdir + (key === "ArrowDown" ? 1 : -1)));
      navRow = activeWorkdir; navCol = 0; render(); return;
    }
    if (key === "ArrowLeft") navCol = Math.max(0, navCol - 1);
    if (key === "ArrowRight") navCol = Math.min(workdirs[activeWorkdir].sessions.length, navCol + 1);
    navRow = activeWorkdir; markCurrent(); return;
  }
  if (currentConcept === "atlas") {
    const delta = key === "ArrowLeft" ? -1 : key === "ArrowRight" ? 1 : key === "ArrowUp" ? -2 : 2;
    navRow = Math.max(0, Math.min(workdirs.length - 1, navRow + delta)); navCol = 0;
  } else {
    if (key === "ArrowUp") navRow = Math.max(0, navRow - 1);
    if (key === "ArrowDown") navRow = Math.min(workdirs.length - 1, navRow + 1);
    if (key === "ArrowLeft") navCol = Math.max(0, navCol - 1);
    if (key === "ArrowRight") navCol = Math.min(workdirs[navRow].sessions.length, navCol + 1);
    navCol = Math.min(navCol, workdirs[navRow].sessions.length);
  }
  activeWorkdir = navRow;
  if (currentConcept === "focus" && !root.querySelector(`[data-row="${navRow}"]`)) render();
  else markCurrent();
}

function renderSearch(query = "") {
  const normalized = query.trim().toLowerCase();
  const results = [
    ...workdirs.map((workdir, row) => ({ type: "Workspace", title: workdir.name, subtitle: workdir.tagline, icon: workdir.icon, row, col: 0 })),
    ...allSessions().map(({ workdir, session, row, col }) => ({ type: cap(session.status), title: session.title, subtitle: workdir.name, icon: workdir.icon, row, col: col + 1 }))
  ].filter(item => !normalized || `${item.title} ${item.subtitle} ${item.type}`.toLowerCase().includes(normalized)).slice(0, 12);
  searchIndex = Math.min(searchIndex, Math.max(0, results.length - 1));
  document.querySelector("#search-results").innerHTML = results.length ? results.map((item, index) => `<button type="button" role="option" class="search-result" aria-selected="${index === searchIndex}" data-search-row="${item.row}" data-search-col="${item.col}"><img src="${item.icon}" alt="" /><span><strong>${escape(item.title)}</strong><small>${escape(item.subtitle)}</small></span><span>${escape(item.type)}</span></button>`).join("") : `<p class="empty">No matching work directory or Session.</p>`;
  document.querySelectorAll("[data-search-row]").forEach(button => button.addEventListener("click", () => { searchDialog.close(); navRow = Number(button.dataset.searchRow); navCol = Number(button.dataset.searchCol); activeWorkdir = navRow; render(); openItem(navRow, navCol); }));
}

function openSearch() { searchIndex = 0; document.querySelector("#search-input").value = ""; renderSearch(); searchDialog.showModal(); requestAnimationFrame(() => document.querySelector("#search-input").focus()); }
function renderComparison() {
  document.querySelector("#comparison-grid").innerHTML = Object.entries(concepts).map(([id, concept]) => `<article class="comparison-card ${id === currentConcept ? "current" : ""}"><span class="concept-no">CONCEPT ${concept.number}</span><h3>${escape(concept.title)}</h3><dl><dt>Core model</dt><dd>${escape(concept.idea)}</dd><dt>Best for</dt><dd>${escape(concept.best)}</dd><dt>Primary strength</dt><dd>${escape(concept.strength)}</dd><dt>Research question</dt><dd>${escape(concept.question)}</dd></dl></article>`).join("");
}

const menuToggle = document.querySelector("#menu-toggle");
const menuPanel = document.querySelector("#menu-panel");
const setMenuOpen = open => { menuPanel.hidden = !open; menuToggle.setAttribute("aria-expanded", String(open)); };
menuToggle.addEventListener("click", () => setMenuOpen(menuPanel.hidden));
document.querySelectorAll("[data-concept]").forEach(button => button.addEventListener("click", () => { setConcept(button.dataset.concept); setMenuOpen(false); }));
document.querySelector("#search-button").addEventListener("click", () => { setMenuOpen(false); openSearch(); });
document.querySelector("#compare-button").addEventListener("click", () => { setMenuOpen(false); renderComparison(); notesDialog.showModal(); });
document.querySelector("#search-input").addEventListener("input", event => { searchIndex = 0; renderSearch(event.target.value); });
document.querySelectorAll(".dialog-close").forEach(button => button.addEventListener("click", () => button.closest("dialog").close()));

window.addEventListener("keydown", event => {
  if (event.target.matches("input, textarea")) {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      const count = document.querySelectorAll("[data-search-row]").length; if (!count) return;
      event.preventDefault(); searchIndex = Math.max(0, Math.min(count - 1, searchIndex + (event.key === "ArrowDown" ? 1 : -1))); renderSearch(event.target.value);
    }
    if (event.key === "Enter") document.querySelector(`[data-search-row]:nth-of-type(${searchIndex + 1})`)?.click();
    return;
  }
  if (["1", "2", "3", "4", "5", "6"].includes(event.key) && !document.querySelector("dialog[open]")) { setConcept(["matrix", "cinema", "focus", "atlas", "anchor", "hybrid"][Number(event.key) - 1]); return; }
  if (event.key === "Escape" && !menuPanel.hidden) { setMenuOpen(false); return; }
  if (event.key === "/" && !document.querySelector("dialog[open]")) { event.preventDefault(); openSearch(); return; }
  if (["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"].includes(event.key) && !document.querySelector("dialog[open]")) { event.preventDefault(); moveNav(event.key); }
});

window.matchMedia("(max-width: 620px)").addEventListener("change", () => { if (currentConcept === "anchor") render(); });

render();
