/* global React, ReactDOM */
const { useState, useEffect, useRef, useMemo, useCallback } = React;

// ===== App shell + keyboard + tweaks =====

const TWEAK_DEFAULTS = /*EDITMODE-BEGIN*/{
  "layout": "vertical",
  "edgeStyle": "ortho",
  "density": "cozy",
  "showMinimap": true,
  "surface": "split"
}/*EDITMODE-END*/;

function TopBar({ filter, setFilter, filterRef, onFit, onGoHead, currentHeadId, setCurrentHeadId, sessionTitle }) {
  return (
    <header className="ob-bar">
      <div className="ob-bar-left">
        <span className="ob-logo" aria-label="oqto">◧ oqto</span>
        <span className="ob-crumb">sessions</span>
        <span className="ob-crumb-sep">/</span>
        <span className="ob-crumb ob-crumb-dim">{sessionTitle}</span>
        <span className="ob-crumb-sep">/</span>
        <span className="ob-crumb ob-crumb-strong">graph</span>
      </div>
      <div className="ob-bar-filter" data-nopan>
        <span className="ob-bar-filter-prefix">/</span>
        <input
          ref={filterRef}
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
          placeholder="filter nodes…   ( / )"
          className="ob-bar-filter-input"
        />
        {filter && (
          <button className="ob-bar-filter-clear" onClick={() => setFilter("")}>
            ×
          </button>
        )}
      </div>
      <div className="ob-bar-right">
        <button className="ob-bar-btn" onClick={onGoHead} title="jump to current head (H)">
          ▸ head
        </button>
        <button className="ob-bar-btn" onClick={onFit} title="fit (f)">
          fit
        </button>
        <span className="ob-bar-sep" />
        <span className="ob-bar-head">
          head: <span className="ob-bar-head-val">{currentHeadId}</span>
        </span>
      </div>
    </header>
  );
}

function Legend() {
  return (
    <div className="ob-legend" data-nopan>
      <div className="ob-legend-row"><span className="ob-legend-sq ob-legend-user" />user</div>
      <div className="ob-legend-row"><span className="ob-legend-ci ob-legend-assistant" />assistant</div>
      <div className="ob-legend-row"><span className="ob-legend-dia ob-legend-system" />system</div>
      <div className="ob-legend-row"><span className="ob-legend-head-mark" />current head</div>
      <div className="ob-legend-row"><span className="ob-legend-active-line" />active branch</div>
    </div>
  );
}

function KeyHints() {
  return (
    <div className="ob-keys" data-nopan>
      <span><kbd>h</kbd><kbd>j</kbd><kbd>k</kbd><kbd>l</kbd> walk</span>
      <span><kbd>↑</kbd><kbd>↓</kbd> scroll transcript</span>
      <span><kbd>/</kbd> filter</span>
      <span><kbd>f</kbd> fit</span>
      <span><kbd>H</kbd> head</span>
    </div>
  );
}

function TweaksPanel({ open, onClose, tweaks, setTweaks }) {
  if (!open) return null;
  const set = (k, v) => {
    const next = { ...tweaks, [k]: v };
    setTweaks(next);
    window.parent?.postMessage(
      { type: "__edit_mode_set_keys", edits: { [k]: v } },
      "*",
    );
  };
  return (
    <div className="ob-tweaks" data-nopan>
      <div className="ob-tweaks-head">
        <span>Tweaks</span>
        <button className="ob-tweaks-close" onClick={onClose}>
          ×
        </button>
      </div>
      <div className="ob-tweaks-body">
        <Row label="layout">
          <Seg value={tweaks.layout} onChange={(v) => set("layout", v)} opts={[
            { v: "vertical", l: "top→down" },
            { v: "horizontal", l: "left→right" },
          ]} />
        </Row>
        <Row label="edges">
          <Seg value={tweaks.edgeStyle} onChange={(v) => set("edgeStyle", v)} opts={[
            { v: "ortho", l: "ortho ┐" },
            { v: "curve", l: "curve ⌒" },
          ]} />
        </Row>
        <Row label="density">
          <Seg value={tweaks.density} onChange={(v) => set("density", v)} opts={[
            { v: "cozy", l: "cozy" },
            { v: "compact", l: "compact" },
          ]} />
        </Row>
        <Row label="surface">
          <Seg value={tweaks.surface} onChange={(v) => set("surface", v)} opts={[
            { v: "split", l: "split" },
            { v: "overlay", l: "overlay" },
            { v: "full", l: "full-bleed" },
          ]} />
        </Row>
        <Row label="minimap">
          <Seg value={tweaks.showMinimap ? "on" : "off"} onChange={(v) => set("showMinimap", v === "on")} opts={[
            { v: "on", l: "on" },
            { v: "off", l: "off" },
          ]} />
        </Row>
      </div>
    </div>
  );
}
function Row({ label, children }) {
  return (
    <div className="ob-tw-row">
      <span className="ob-tw-label">{label}</span>
      <span className="ob-tw-ctl">{children}</span>
    </div>
  );
}
function Seg({ value, onChange, opts }) {
  return (
    <div className="ob-seg">
      {opts.map((o) => (
        <button
          key={o.v}
          className={`ob-seg-btn ${value === o.v ? "ob-seg-active" : ""}`}
          onClick={() => onChange(o.v)}
        >
          {o.l}
        </button>
      ))}
    </div>
  );
}

function App() {
  const [tweaks, setTweaks] = useState(TWEAK_DEFAULTS);
  const [editModeOn, setEditModeOn] = useState(false);
  const [selectedId, setSelectedId] = useState(DATA.currentHead);
  const [openedId, setOpenedId] = useState(null);
  const [previewId, setPreviewId] = useState(null);
  const [currentHeadId, setCurrentHeadId] = useState(DATA.currentHead);
  const [filter, setFilter] = useState("");
  const filterRef = useRef(null);
  const fitFnRef = useRef(null);

  // Edit mode wiring
  useEffect(() => {
    const onMsg = (e) => {
      if (!e.data || typeof e.data !== "object") return;
      if (e.data.type === "__activate_edit_mode") setEditModeOn(true);
      if (e.data.type === "__deactivate_edit_mode") setEditModeOn(false);
    };
    window.addEventListener("message", onMsg);
    window.parent?.postMessage({ type: "__edit_mode_available" }, "*");
    return () => window.removeEventListener("message", onMsg);
  }, []);

  // Active branch (head's ancestors) used to bias navigation
  const activeBranch = useMemo(() => new Set(ancestors(currentHeadId)), [currentHeadId]);

  // Keyboard
  useEffect(() => {
    const onKey = (e) => {
      const tag = e.target.tagName;
      const inInput = tag === "INPUT" || tag === "TEXTAREA";

      // global: '/' focuses filter
      if (e.key === "/" && !inInput) {
        e.preventDefault();
        filterRef.current?.focus();
        filterRef.current?.select();
        return;
      }
      if (inInput) {
        if (e.key === "Escape") {
          filterRef.current?.blur();
        }
        return;
      }
      if (e.key === "Escape") {
        setPreviewId(null);
        setOpenedId(null);
        return;
      }

      if (e.key === "Enter") {
        // open = jump to this node as focal (panel auto-follows). optional: also set as temp-preview
        e.preventDefault();
        return;
      }
      if (e.key === " ") {
        // space: quick toggle — fit on selected
        e.preventDefault();
        return;
      }
      if (e.key === "f") {
        e.preventDefault();
        fitFnRef.current?.();
        return;
      }
      if (e.key === "H") {
        e.preventDefault();
        setSelectedId(currentHeadId);
        return;
      }
      if (e.key === "c" && (e.metaKey || e.ctrlKey)) return;
      if (["h", "j", "k", "l", "ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"].includes(e.key)) {
        e.preventDefault();
        // In horizontal layout, arrow keys should feel spatial: left=parent, right=child, up/down=siblings
        // In vertical layout: up=parent, down=child, left/right=siblings
        let dir = e.key;
        if (tweaks.layout === "vertical") {
          if (e.key === "ArrowUp") dir = "parent";
          else if (e.key === "ArrowDown") dir = "child";
          else if (e.key === "ArrowLeft") dir = "prevSib";
          else if (e.key === "ArrowRight") dir = "nextSib";
        } else {
          if (e.key === "ArrowLeft") dir = "parent";
          else if (e.key === "ArrowRight") dir = "child";
          else if (e.key === "ArrowUp") dir = "prevSib";
          else if (e.key === "ArrowDown") dir = "nextSib";
        }
        setSelectedId((id) => navigate(id, dir, activeBranch));
        setPreviewId(null);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [selectedId, openedId, previewId, currentHeadId, tweaks.layout, activeBranch]);

  // Expose a "fit" function up from GraphCanvas via ref dance
  const handleFitRef = (fn) => {
    fitFnRef.current = fn;
  };

  const surface = tweaks.surface;

  return (
    <div className={`ob-app ob-surface-${surface}`}>
      <TopBar
        filter={filter}
        setFilter={setFilter}
        filterRef={filterRef}
        onFit={() => fitFnRef.current?.()}
        onGoHead={() => setSelectedId(currentHeadId)}
        currentHeadId={currentHeadId}
        sessionTitle="omni-ghostty-web"
      />
      <div className="ob-main">
        <div className="ob-graph">
          <GraphCanvas
            layout={tweaks.layout}
            edgeStyle={tweaks.edgeStyle}
            density={tweaks.density}
            showMinimap={tweaks.showMinimap}
            selectedId={selectedId}
            setSelectedId={setSelectedId}
            openedId={openedId}
            setOpenedId={setOpenedId}
            previewId={previewId}
            setPreviewId={setPreviewId}
            currentHeadId={currentHeadId}
            filter={filter}
            onFitRef={handleFitRef}
          />
          <Legend />
          <KeyHints />
        </div>

        {(surface === "split" || surface === "overlay") && (
          <SidePanel
            selectedId={selectedId}
            setSelectedId={setSelectedId}
            currentHeadId={currentHeadId}
            setCurrentHeadId={setCurrentHeadId}
            activeBranch={activeBranch}
          />
        )}
      </div>

      <TweaksPanel
        open={editModeOn}
        onClose={() => setEditModeOn(false)}
        tweaks={tweaks}
        setTweaks={setTweaks}
      />

      <div className="ob-status">
        <span>sel <b>{selectedId}</b></span>
        <span>·</span>
        <span>head <b>{currentHeadId}</b></span>
        <span>·</span>
        <span>{Object.keys(NODES).length} nodes</span>
        <span>·</span>
        <span>{Object.values(NODES).filter(n => n.children.length > 1).length} fork points</span>
        {filter && <><span>·</span><span>filter: <b>"{filter}"</b></span></>}
      </div>
    </div>
  );
}

window.OqtoApp = App;
