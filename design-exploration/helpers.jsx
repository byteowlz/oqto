/* global React, ReactDOM */
const { useState, useEffect, useRef, useMemo, useCallback, useLayoutEffect } = React;

// ---------- utility ----------
const DATA = window.OQTO_DATA;
const { nodes: NODES, root: ROOT } = DATA;

const fmtTime = (ts) => {
  const d = new Date(ts);
  const now = Date.now();
  const diffMs = now - ts;
  const min = Math.floor(diffMs / 60000);
  if (min < 1) return "just now";
  if (min < 60) return `${min}m ago`;
  const hr = Math.floor(min / 60);
  if (hr < 24) return `${hr}h ago`;
  const day = Math.floor(hr / 24);
  if (day < 7) return `${day}d ago`;
  return d.toISOString().slice(0, 10);
};

const firstWords = (s, n = 8) => {
  const words = (s || "").split(/\s+/).filter(Boolean);
  const head = words.slice(0, n).join(" ");
  return words.length > n ? head + "…" : head;
};

// Ancestors chain root→node (inclusive of node)
const ancestors = (id) => {
  const out = [];
  let cur = id;
  while (cur) {
    out.unshift(cur);
    cur = NODES[cur].parent;
  }
  return out;
};

// Forward path from node along first-child chain (exclusive of starting node)
// If activeBranch is provided (Set of ids on current head path), prefer those children.
const forwardPath = (id, activeBranch) => {
  const out = [];
  let cur = id;
  while (cur && NODES[cur].children.length > 0) {
    const kids = NODES[cur].children;
    const next = activeBranch
      ? kids.find((c) => activeBranch.has(c)) || kids[0]
      : kids[0];
    out.push(next);
    cur = next;
  }
  return out;
};

// Full branch transcript: ancestors + forward continuation from this node
const branchTranscript = (id, activeBranch) => [
  ...ancestors(id),
  ...forwardPath(id, activeBranch),
];

// sibling navigation among parent's children
const siblings = (id) => {
  const p = NODES[id].parent;
  if (!p) return [id];
  return NODES[p].children;
};

// vim-ish nav. h: parent, l: first child (or follow active branch), j: next sibling, k: prev sibling
const navigate = (id, dir, activeBranch) => {
  const n = NODES[id];
  if (!n) return id;
  if (dir === "h" || dir === "ArrowLeft" || dir === "parent") {
    return n.parent || id;
  }
  if (dir === "l" || dir === "ArrowRight" || dir === "child") {
    if (n.children.length === 0) return id;
    // prefer child on the active branch path if available
    const preferred = n.children.find((c) => activeBranch && activeBranch.has(c));
    return preferred || n.children[0];
  }
  if (dir === "j" || dir === "ArrowDown" || dir === "nextSib") {
    const sibs = siblings(id);
    const i = sibs.indexOf(id);
    return sibs[(i + 1) % sibs.length];
  }
  if (dir === "k" || dir === "ArrowUp" || dir === "prevSib") {
    const sibs = siblings(id);
    const i = sibs.indexOf(id);
    return sibs[(i - 1 + sibs.length) % sibs.length];
  }
  return id;
};

// ---------- pan/zoom hook ----------
function usePanZoom(ref, { min = 0.2, max = 3 } = {}) {
  const [t, setT] = useState({ x: 0, y: 0, k: 1 });
  const tRef = useRef(t);
  tRef.current = t;

  useEffect(() => {
    const el = ref.current;
    if (!el) return;

    let dragging = false;
    let last = { x: 0, y: 0 };

    const onMouseDown = (e) => {
      if (e.button !== 0) return;
      if (e.target.closest("[data-node]") || e.target.closest("[data-nopan]")) return;
      dragging = true;
      last = { x: e.clientX, y: e.clientY };
      el.style.cursor = "grabbing";
    };
    const onMouseMove = (e) => {
      if (!dragging) return;
      const dx = e.clientX - last.x;
      const dy = e.clientY - last.y;
      last = { x: e.clientX, y: e.clientY };
      setT((p) => ({ ...p, x: p.x + dx, y: p.y + dy }));
    };
    const onMouseUp = () => {
      dragging = false;
      el.style.cursor = "";
    };

    const onWheel = (e) => {
      e.preventDefault();
      const rect = el.getBoundingClientRect();
      const mx = e.clientX - rect.left;
      const my = e.clientY - rect.top;
      const p = tRef.current;
      if (e.ctrlKey || e.metaKey) {
        // pinch / ctrl-scroll zoom
        const delta = -e.deltaY * 0.01;
        const k2 = Math.max(min, Math.min(max, p.k * Math.exp(delta)));
        const kRatio = k2 / p.k;
        setT({
          k: k2,
          x: mx - (mx - p.x) * kRatio,
          y: my - (my - p.y) * kRatio,
        });
      } else {
        setT((p) => ({ ...p, x: p.x - e.deltaX, y: p.y - e.deltaY }));
      }
    };

    el.addEventListener("mousedown", onMouseDown);
    window.addEventListener("mousemove", onMouseMove);
    window.addEventListener("mouseup", onMouseUp);
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => {
      el.removeEventListener("mousedown", onMouseDown);
      window.removeEventListener("mousemove", onMouseMove);
      window.removeEventListener("mouseup", onMouseUp);
      el.removeEventListener("wheel", onWheel);
    };
  }, [ref, min, max]);

  return [t, setT];
}

Object.assign(window, {
  DATA,
  NODES,
  ROOT,
  fmtTime,
  firstWords,
  ancestors,
  siblings,
  navigate,
  usePanZoom,
});
