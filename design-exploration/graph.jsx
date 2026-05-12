/* global React, ReactDOM */
const { useState, useEffect, useRef, useMemo, useCallback, useLayoutEffect } = React;

// === Graph View ===
// Props: layout ('vertical'|'horizontal'), edgeStyle ('ortho'|'curve'), density ('cozy'|'compact'),
//   selectedId, setSelectedId, openedId, setOpenedId, previewId, setPreviewId,
//   currentHeadId, filter, onKeyHandled

function GraphCanvas(props) {
  const {
    layout,
    edgeStyle,
    density,
    selectedId,
    setSelectedId,
    openedId,
    setOpenedId,
    previewId,
    setPreviewId,
    currentHeadId,
    filter,
    showMinimap,
    onFitRef,
  } = props;

  const canvasRef = useRef(null);
  const [vw, setVw] = useState(1200);
  const [vh, setVh] = useState(800);
  const [t, setT] = usePanZoom(canvasRef);

  useLayoutEffect(() => {
    const el = canvasRef.current;
    if (!el) return;
    const ro = new ResizeObserver(() => {
      setVw(el.clientWidth);
      setVh(el.clientHeight);
    });
    ro.observe(el);
    setVw(el.clientWidth);
    setVh(el.clientHeight);
    return () => ro.disconnect();
  }, []);

  // unit → px spacing depends on density + orientation
  const SX = density === "compact" ? 34 : 52;
  const SY = density === "compact" ? 46 : 66;
  const HSX = density === "compact" ? 140 : 210; // horizontal: depth step
  const HSY = density === "compact" ? 30 : 42; // horizontal: sibling step

  const pos = useMemo(() => {
    const raw =
      layout === "vertical"
        ? window.OQTO_LAYOUT.vertical()
        : window.OQTO_LAYOUT.horizontal();
    const scaled = {};
    const sx = layout === "vertical" ? SX : HSX;
    const sy = layout === "vertical" ? SY : HSY;
    Object.keys(raw).forEach((id) => {
      scaled[id] = { x: raw[id].x * sx, y: raw[id].y * sy };
    });
    return scaled;
  }, [layout, density, SX, SY, HSX, HSY]);

  const bbox = useMemo(() => window.OQTO_LAYOUT.bbox(pos), [pos]);

  // ancestors of current head (for highlighting the active branch)
  const activeBranchSet = useMemo(() => {
    const s = new Set(ancestors(currentHeadId));
    return s;
  }, [currentHeadId]);

  // ancestors of selected → dim path trail
  const selectedTrail = useMemo(() => {
    return new Set(selectedId ? ancestors(selectedId) : []);
  }, [selectedId]);

  // Filter: mark matches and their ancestors
  const filterMatchIds = useMemo(() => {
    if (!filter) return null;
    const q = filter.toLowerCase();
    const matched = new Set();
    Object.values(NODES).forEach((n) => {
      const hay = (n.text || "") + " " + (n.model || "") + " " + n.role;
      if (hay.toLowerCase().includes(q)) matched.add(n.id);
    });
    // include ancestors so the path to each match is visible
    const expand = new Set(matched);
    matched.forEach((id) => ancestors(id).forEach((a) => expand.add(a)));
    return { matched, visible: expand };
  }, [filter]);

  // Fit the whole graph on mount / layout change
  const fitTo = useCallback(
    (targetIds, pad = 120) => {
      if (!vw || !vh) return;
      const ids = targetIds || Object.keys(pos);
      if (ids.length === 0) return;
      let minX = Infinity,
        minY = Infinity,
        maxX = -Infinity,
        maxY = -Infinity;
      ids.forEach((id) => {
        const p = pos[id];
        if (!p) return;
        minX = Math.min(minX, p.x);
        minY = Math.min(minY, p.y);
        maxX = Math.max(maxX, p.x);
        maxY = Math.max(maxY, p.y);
      });
      const w = maxX - minX || 1;
      const h = maxY - minY || 1;
      const k = Math.max(0.5, Math.min((vw - pad * 2) / w, (vh - pad * 2) / h, 1.2));
      const cx = (minX + maxX) / 2;
      const cy = (minY + maxY) / 2;
      setT({ k, x: vw / 2 - cx * k, y: vh / 2 - cy * k });
    },
    [pos, vw, vh, setT],
  );

  useLayoutEffect(() => {
    if (!vw || !vh) return;
    // Center on current head at a comfortable zoom instead of fitting everything.
    const p = pos[currentHeadId];
    if (!p) { fitTo(); return; }
    const k = 0.9;
    setT({ k, x: vw / 2 - p.x * k, y: vh / 2 - p.y * k });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [layout, vw > 0 && vh > 0]);

  useEffect(() => {
    if (onFitRef) onFitRef(() => fitTo());
  }, [onFitRef, fitTo]);

  // Pan the selected node into view with margin
  const ensureVisible = useCallback(
    (id) => {
      const p = pos[id];
      if (!p) return;
      const sx = p.x * t.k + t.x;
      const sy = p.y * t.k + t.y;
      const margin = 120;
      let nx = t.x,
        ny = t.y;
      if (sx < margin) nx += margin - sx;
      if (sx > vw - margin) nx -= sx - (vw - margin);
      if (sy < margin) ny += margin - sy;
      if (sy > vh - margin) ny -= sy - (vh - margin);
      if (nx !== t.x || ny !== t.y) setT((p) => ({ ...p, x: nx, y: ny }));
    },
    [pos, t, vw, vh, setT],
  );

  useEffect(() => {
    if (selectedId) ensureVisible(selectedId);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedId]);

  // Build edges
  const edges = useMemo(() => {
    const out = [];
    Object.values(NODES).forEach((n) => {
      const p = pos[n.id];
      if (!p) return;
      n.children.forEach((cid) => {
        const c = pos[cid];
        if (!c) return;
        out.push({ from: n.id, to: cid, x1: p.x, y1: p.y, x2: c.x, y2: c.y });
      });
    });
    return out;
  }, [pos]);

  // Edge path
  const edgePath = (e) => {
    if (edgeStyle === "curve") {
      if (layout === "vertical") {
        const mid = (e.y1 + e.y2) / 2;
        return `M ${e.x1} ${e.y1} C ${e.x1} ${mid}, ${e.x2} ${mid}, ${e.x2} ${e.y2}`;
      }
      const mid = (e.x1 + e.x2) / 2;
      return `M ${e.x1} ${e.y1} C ${mid} ${e.y1}, ${mid} ${e.y2}, ${e.x2} ${e.y2}`;
    }
    // ortho ("ghostty-style right-angle")
    if (layout === "vertical") {
      const mid = (e.y1 + e.y2) / 2;
      return `M ${e.x1} ${e.y1} L ${e.x1} ${mid} L ${e.x2} ${mid} L ${e.x2} ${e.y2}`;
    }
    const mid = (e.x1 + e.x2) / 2;
    return `M ${e.x1} ${e.y1} L ${mid} ${e.y1} L ${mid} ${e.y2} L ${e.x2} ${e.y2}`;
  };

  // Node sizing
  const nodeSize = density === "compact" ? 8 : 10;

  const renderNode = (n) => {
    const p = pos[n.id];
    if (!p) return null;

    const isSelected = n.id === selectedId;
    const isOpen = n.id === openedId;
    const isHead = n.id === currentHeadId;
    const isRoot = n.id === ROOT;
    const isAncestor = selectedTrail.has(n.id) && !isSelected;
    const isOnActiveBranch = activeBranchSet.has(n.id);
    const dimmed = filterMatchIds && !filterMatchIds.visible.has(n.id);
    const matched = filterMatchIds && filterMatchIds.matched.has(n.id);

    const cls = [
      "ogn",
      `ogn-${n.role}`,
      isSelected && "ogn-selected",
      isOpen && "ogn-open",
      isHead && "ogn-head",
      isAncestor && "ogn-ancestor",
      isOnActiveBranch && "ogn-active-branch",
      dimmed && "ogn-dim",
      matched && "ogn-match",
      n.starred && "ogn-starred",
      isRoot && "ogn-root",
    ]
      .filter(Boolean)
      .join(" ");

    const showLabel = density !== "compact" || isSelected || isHead || matched;

    return (
      <g
        key={n.id}
        data-node={n.id}
        className={cls}
        transform={`translate(${p.x} ${p.y})`}
        onClick={(e) => {
          e.stopPropagation();
          setSelectedId(n.id);
          setPreviewId(null);
        }}
        onDoubleClick={(e) => {
          e.stopPropagation();
          setSelectedId(n.id);
          setOpenedId(n.id);
        }}
      >
        {/* hit area */}
        <rect
          x={-24}
          y={-14}
          width={48}
          height={28}
          fill="transparent"
          style={{ cursor: "pointer" }}
        />
        {/* outer ring for selected/head */}
        {(isSelected || isHead) && (
          <rect
            className="ogn-ring"
            x={-nodeSize - 3}
            y={-nodeSize - 3}
            width={nodeSize * 2 + 6}
            height={nodeSize * 2 + 6}
          />
        )}
        {n.role === "user" ? (
          <rect
            className="ogn-shape"
            x={-nodeSize / 2}
            y={-nodeSize / 2}
            width={nodeSize}
            height={nodeSize}
          />
        ) : n.role === "assistant" ? (
          <circle className="ogn-shape" r={nodeSize / 2} />
        ) : (
          <rect
            className="ogn-shape"
            x={-nodeSize / 2}
            y={-nodeSize / 2}
            width={nodeSize}
            height={nodeSize}
            transform="rotate(45)"
          />
        )}
        {n.starred && (
          <text className="ogn-star" x={nodeSize / 2 + 4} y={-nodeSize / 2 - 2}>
            ★
          </text>
        )}
        {showLabel && (
          <text
            className="ogn-label"
            x={layout === "vertical" ? 0 : nodeSize + 6}
            y={layout === "vertical" ? nodeSize + 12 : 3}
            textAnchor={layout === "vertical" ? "middle" : "start"}
          >
            {n.role === "system"
              ? "session start"
              : firstWords(
                  n.text,
                  density === "compact" ? 3 : layout === "horizontal" ? 4 : 6,
                )}
          </text>
        )}
        {n.role === "assistant" && n.model && density !== "compact" && (
          <text
            className="ogn-sub"
            x={layout === "vertical" ? 0 : nodeSize + 6}
            y={layout === "vertical" ? nodeSize + 24 : 15}
            textAnchor={layout === "vertical" ? "middle" : "start"}
          >
            {n.model}
          </text>
        )}
      </g>
    );
  };

  const renderEdge = (e) => {
    const c = NODES[e.to];
    const p = NODES[e.from];
    const onActive = activeBranchSet.has(e.to) && activeBranchSet.has(e.from);
    const onTrail = selectedTrail.has(e.to) && selectedTrail.has(e.from);
    const dimmed =
      filterMatchIds &&
      !(filterMatchIds.visible.has(e.to) && filterMatchIds.visible.has(e.from));
    const cls = [
      "oge",
      onActive && "oge-active",
      onTrail && "oge-trail",
      dimmed && "oge-dim",
    ]
      .filter(Boolean)
      .join(" ");
    return (
      <g key={`${e.from}-${e.to}`} className={cls}>
        <path d={edgePath(e)} />
        {c.branchLabel && p.children.length > 1 && density !== "compact" && (
          <text
            className="oge-label"
            x={(e.x1 + e.x2) / 2}
            y={(e.y1 + e.y2) / 2}
            textAnchor="middle"
          >
            {c.branchLabel}
          </text>
        )}
      </g>
    );
  };

  // Minimap
  const miniW = 200;
  const miniH = 140;
  const miniPad = 6;
  const miniScale = Math.min(
    (miniW - miniPad * 2) / (bbox.w || 1),
    (miniH - miniPad * 2) / (bbox.h || 1),
  );
  const miniOX = miniPad - bbox.minX * miniScale;
  const miniOY = miniPad - bbox.minY * miniScale;

  // viewport rect in mini coords
  const viewBoxInMini = {
    x: miniOX + (-t.x / t.k) * miniScale,
    y: miniOY + (-t.y / t.k) * miniScale,
    w: (vw / t.k) * miniScale,
    h: (vh / t.k) * miniScale,
  };

  const onMiniMouseDown = (e) => {
    const rect = e.currentTarget.getBoundingClientRect();
    const startX = e.clientX;
    const startY = e.clientY;
    const onMove = (ev) => {
      const mx = ev.clientX - rect.left;
      const my = ev.clientY - rect.top;
      // convert mini coord back to world
      const wx = (mx - miniOX) / miniScale;
      const wy = (my - miniOY) / miniScale;
      setT((p) => ({ ...p, x: vw / 2 - wx * p.k, y: vh / 2 - wy * p.k }));
    };
    onMove(e);
    const onUp = () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
  };

  return (
    <div className="ogc-root" ref={canvasRef} tabIndex={0}>
      <svg width="100%" height="100%" className="ogc-svg" data-layout={layout}>
        <g transform={`translate(${t.x} ${t.y}) scale(${t.k})`}>
          <g className="ogc-edges">{edges.map(renderEdge)}</g>
          <g className="ogc-nodes">{Object.values(NODES).map(renderNode)}</g>
        </g>
      </svg>

      {/* zoom indicator + fit button */}
      <div className="ogc-zoom" data-nopan>
        <button
          onClick={() => fitTo()}
          title="fit all (f)"
          className="ogc-btn"
        >
          fit
        </button>
        <button
          onClick={() =>
            setT((p) => ({ ...p, k: Math.min(3, p.k * 1.2) }))
          }
          className="ogc-btn"
        >
          +
        </button>
        <button
          onClick={() =>
            setT((p) => ({ ...p, k: Math.max(0.2, p.k / 1.2) }))
          }
          className="ogc-btn"
        >
          −
        </button>
        <span className="ogc-zoom-val">{Math.round(t.k * 100)}%</span>
      </div>

      {/* Minimap */}
      {showMinimap && (
        <div className="ogc-mini" data-nopan>
          <div className="ogc-mini-label">
            <span>minimap</span>
            <span>{Object.keys(NODES).length} nodes</span>
          </div>
          <svg
            width={miniW}
            height={miniH}
            onMouseDown={onMiniMouseDown}
            className="ogc-mini-svg"
          >
            {edges.map((e, i) => (
              <path
                key={i}
                d={`M ${e.x1 * miniScale + miniOX} ${e.y1 * miniScale + miniOY} L ${e.x2 * miniScale + miniOX} ${e.y2 * miniScale + miniOY}`}
                stroke="var(--mini-edge)"
                strokeWidth={0.6}
                fill="none"
              />
            ))}
            {Object.values(NODES).map((n) => {
              const p = pos[n.id];
              if (!p) return null;
              const isHead = n.id === currentHeadId;
              const isSel = n.id === selectedId;
              return (
                <circle
                  key={n.id}
                  cx={p.x * miniScale + miniOX}
                  cy={p.y * miniScale + miniOY}
                  r={isHead || isSel ? 2 : 1.1}
                  fill={
                    isSel
                      ? "var(--mini-sel)"
                      : isHead
                        ? "var(--mini-head)"
                        : "var(--mini-node)"
                  }
                />
              );
            })}
            <rect
              x={viewBoxInMini.x}
              y={viewBoxInMini.y}
              width={viewBoxInMini.w}
              height={viewBoxInMini.h}
              fill="rgba(59,167,124,0.08)"
              stroke="var(--mini-view)"
              strokeWidth={1}
            />
          </svg>
        </div>
      )}
    </div>
  );
}

window.GraphCanvas = GraphCanvas;
