// Mock branching chat tree.
// Each node: {id, role: 'user'|'assistant'|'system', text, model?, ts, parent, children, toolCalls?, starred?, muted?}
// Branches are created by explicit "Branch from here" — so a node can have multiple children.

(function () {
  const MODELS = [
    "claude-sonnet-4.5",
    "claude-opus-4.1",
    "gpt-5.1",
    "gpt-5-mini",
    "gemini-2.5-pro",
    "deepseek-v3.2",
    "llama-4-maverick",
    "qwen-3-235b",
  ];

  const USER_PROMPTS = [
    "Refactor the ws-manager to use exponential backoff",
    "Why does the chat state machine drop tool_use deltas?",
    "Write a vitest for the dedup helper",
    "Draft a PR description for the session-rekey change",
    "Summarize the opencode-sdk-reference doc",
    "Explain the difference between bus-client and ws-client",
    "Find all places we throttle streaming events",
    "How would we add branching to the chat state machine?",
    "Port the Tauri fetch polyfill to use fetch streams",
    "Audit useEffect usage in features/chat",
    "Design a graph view for session history",
    "Propose a schema for branch metadata in JSONL",
    "Rewrite ChatEntry to avoid the 1300-line file",
    "Add read-aloud for assistant messages",
    "Sketch keyboard shortcuts for spotlight",
    "Trace how a dictation result reaches send-flow",
    "What breaks if we drop JetBrains Mono?",
    "Mock a settings page for voice",
    "Propose a minimap component for the chat timeline",
    "Compare approaches to storing branch heads",
    "Why is message-buffer regression flaky?",
    "Inline the bus-client tests into the reducer file",
    "Translate all strings in sessions view to de",
    "Drop radius:0 globally — what would it look like?",
    "Convert app-context to zustand",
    "What's the smallest change to land branching?",
    "Add a 'copy as terminal transcript' action",
    "Explore a ghostty-backed preview pane",
    "Write the onboarding copy for Default Chat",
    "Bench JSONL parse speed on 10k lines",
  ];

  const ASSISTANT_SNIPPETS = [
    "I'd start by isolating the reconnect logic into a small state machine with explicit retry counts.",
    "Looking at the reducer, the drop happens when a delta arrives for a tool_use whose id hasn't been registered yet.",
    "Here's a test that covers the merge-on-same-id path and the collision case.",
    "Short version: bus-client is transport-agnostic and speaks canonical events; ws-client owns the socket lifecycle.",
    "I can see three hot spots: streaming-throttle.ts, use-chat-send-reliability, and the canonical-event-reducer.",
    "Branching needs a parent pointer on each message and a 'head' pointer per branch in the session file header.",
    "Minimap: render all nodes at 1/8 scale, highlight viewport, click to jump.",
    "Keyboard map: j/k for prev/next sibling, h/l for parent/child, / for filter, b to branch, Enter to open.",
    "We could store branches as a list of (branch_id, head_message_id) in the session header and keep the graph implicit.",
    "Reading the file, the flakiness is because we assume delta order — let me sketch a fix.",
    "Radius-0 is load-bearing for the terminal aesthetic; softening corners would require rethinking the whole type scale.",
    "Smallest-change answer: add parent_id to MessageFile, keep current head semantics, branch = fork at a parent.",
    "I'd expose a `branches` view that reads heads from the header and builds the tree on mount.",
    "ghostty-backed previews would give us real ANSI. The tradeoff is WASM bundle size.",
    "Here's a rough copy pass. Tone: quiet, declarative, no exclamation marks.",
    "Let me map the dictation path: mic → use-dictation → chat-state-machine → send-flow → ws-client.",
  ];

  let idCounter = 0;
  const nid = () => `n${String(idCounter++).padStart(4, "0")}`;

  // Seedable PRNG so the graph is stable across reloads
  let seed = 42;
  const rnd = () => {
    seed = (seed * 1664525 + 1013904223) >>> 0;
    return seed / 0xffffffff;
  };
  const pick = (arr) => arr[Math.floor(rnd() * arr.length)];
  const pickInt = (a, b) => a + Math.floor(rnd() * (b - a + 1));

  const nodes = {};
  const addNode = (n) => {
    nodes[n.id] = n;
    if (n.parent) nodes[n.parent].children.push(n.id);
    return n;
  };

  const NOW = Date.parse("2026-04-17T14:20:00Z");
  let tsCursor = NOW - 1000 * 60 * 60 * 36; // start ~36h ago

  const makeUser = (parent, textOverride) => {
    tsCursor += pickInt(20_000, 180_000);
    return addNode({
      id: nid(),
      role: "user",
      text: textOverride || pick(USER_PROMPTS),
      ts: tsCursor,
      parent,
      children: [],
    });
  };

  const makeAssistant = (parent, opts = {}) => {
    tsCursor += pickInt(3_000, 45_000);
    const hasTools = rnd() < 0.35;
    return addNode({
      id: nid(),
      role: "assistant",
      text: opts.text || pick(ASSISTANT_SNIPPETS),
      model: opts.model || pick(MODELS),
      ts: tsCursor,
      parent,
      children: [],
      toolCalls: hasTools
        ? pickInt(1, 4) === 1
          ? ["read"]
          : ["read", "grep", "edit"].slice(0, pickInt(1, 3))
        : undefined,
      starred: rnd() < 0.08,
      muted: rnd() < 0.06,
    });
  };

  // Root
  const root = addNode({
    id: nid(),
    role: "system",
    text: "session started",
    ts: tsCursor,
    parent: null,
    children: [],
  });

  // Build a tree that's a real tree: many branches, bounded depth per branch.
  const TARGET_NODES = 120;
  const MAX_DEPTH = 22;

  // Helper to extend a linear chain from a parent by N exchanges
  const extendChain = (parent, n) => {
    let cur = parent;
    for (let i = 0; i < n && Object.keys(nodes).length < TARGET_NODES; i++) {
      const u = makeUser(cur.id);
      const a = makeAssistant(u.id);
      cur = a;
      if (DATA_DEPTH(cur.id) >= MAX_DEPTH) break;
    }
    return cur;
  };
  const DATA_DEPTH = (id) => {
    let d = 0, cur = id;
    while (nodes[cur] && nodes[cur].parent) { cur = nodes[cur].parent; d++; }
    return d;
  };

  // Start with a short main trunk
  let trunk = extendChain(root, pickInt(3, 5));

  // From the trunk, spawn several branches
  const forkable = [];
  {
    let cur = root.id;
    while (cur) {
      forkable.push(cur);
      cur = nodes[cur].children[0];
    }
  }

  // Now iteratively: pick a random forkable assistant node, branch off with a chain
  let guard = 0;
  while (Object.keys(nodes).length < TARGET_NODES && guard++ < 400) {
    // weight toward assistant nodes not too deep
    const candidates = Object.values(nodes).filter(
      (n) => n.role === "assistant" && DATA_DEPTH(n.id) < MAX_DEPTH - 4,
    );
    if (candidates.length === 0) break;
    const parent = candidates[Math.floor(rnd() * candidates.length)];
    // branch chance proportional to how few children it already has
    if (parent.children.length > 2 && rnd() < 0.7) continue;
    const chainLen = pickInt(2, 6);
    extendChain(parent, chainLen);
  }


  // Give a few nodes explicit "branch label" metadata so we can show edge labels
  const branchLabels = [
    "main",
    "try/sonnet",
    "try/opus",
    "try/gpt-5",
    "alt-approach",
    "cleanup",
    "quickfix",
    "tangent",
    "with-tools",
    "refactor",
  ];
  let labelI = 0;
  Object.values(nodes).forEach((n) => {
    if (n.children.length > 1) {
      n.children.forEach((cid, i) => {
        const c = nodes[cid];
        c.branchLabel =
          i === 0
            ? "main"
            : branchLabels[(labelI++ % (branchLabels.length - 1)) + 1];
      });
    }
  });

  // "Current" branch head — the user's active position
  const allHeads = Object.values(nodes).filter((n) => n.children.length === 0);
  const currentHead = allHeads[Math.floor(allHeads.length * 0.4)];

  // Pre-compute depth (distance from root) and a stable "x order" via DFS
  const depth = {};
  const dfsOrder = [];
  (function dfs(id, d) {
    depth[id] = d;
    dfsOrder.push(id);
    nodes[id].children.forEach((c) => dfs(c, d + 1));
  })(root.id, 0);

  window.OQTO_DATA = {
    root: root.id,
    nodes,
    currentHead: currentHead.id,
    depth,
    dfsOrder,
    models: MODELS,
  };
})();
