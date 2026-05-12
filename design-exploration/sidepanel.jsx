/* global React, ReactDOM */
const { useState, useEffect, useRef, useMemo, useCallback, useLayoutEffect } = React;

// ===== Side panel: always-visible branch transcript, scroll-synced with graph =====

function RolePill({ role, model }) {
  return (
    <span className={`op-role op-role-${role}`}>
      <span className="op-role-dot" />
      {role}
      {role === "assistant" && model ? (
        <span className="op-role-model"> · {model}</span>
      ) : null}
    </span>
  );
}

function MessageBubble({ node, active, onClick, onFork, hasForks, forkCount }) {
  return (
    <article
      className={`op-msg op-msg-${node.role} ${active ? "op-msg-active" : ""}`}
      data-node-id={node.id}
      onClick={onClick}
    >
      <div className="op-msg-rail" aria-hidden="true" />
      <div className="op-msg-head">
        <RolePill role={node.role} model={node.model} />
        <span className="op-msg-ts">{fmtTime(node.ts)}</span>
        {node.branchLabel && node.branchLabel !== "main" && (
          <span className="op-msg-branch">↳ {node.branchLabel}</span>
        )}
        <span className="op-msg-flex" />
        <span className="op-msg-id">{node.id}</span>
      </div>
      <div className="op-msg-body">{node.text}</div>
      {node.toolCalls && (
        <div className="op-msg-tools">
          {node.toolCalls.map((t, i) => (
            <span key={i} className="op-tool">
              ◆ {t}
            </span>
          ))}
        </div>
      )}
      {hasForks && (
        <button
          className="op-msg-fork"
          onClick={(e) => {
            e.stopPropagation();
            onFork && onFork();
          }}
          title="this message has alternative continuations"
        >
          ⎇ {forkCount} fork{forkCount > 1 ? "s" : ""} from here
        </button>
      )}
    </article>
  );
}

function SidePanel({
  selectedId,
  setSelectedId,
  currentHeadId,
  setCurrentHeadId,
  activeBranch, // Set of ids on current head's path (for forward-path preference)
}) {
  const scrollRef = useRef(null);
  const suppressScrollSync = useRef(false); // block scroll->select during programmatic scroll

  // The branch transcript shown depends on the selected node
  const transcript = useMemo(
    () => branchTranscript(selectedId, activeBranch),
    [selectedId, activeBranch],
  );

  // Scroll to active message when selectedId changes from outside
  useLayoutEffect(() => {
    const scroller = scrollRef.current;
    if (!scroller) return;
    const el = scroller.querySelector(`[data-node-id="${selectedId}"]`);
    if (!el) return;
    suppressScrollSync.current = true;
    // center-ish scroll
    const r = el.getBoundingClientRect();
    const sr = scroller.getBoundingClientRect();
    const offset = r.top - sr.top - sr.height * 0.32;
    scroller.scrollBy({ top: offset, behavior: "smooth" });
    const t = setTimeout(() => {
      suppressScrollSync.current = false;
    }, 450);
    return () => clearTimeout(t);
  }, [selectedId]);

  // Scroll → select: whichever article has its top crossing the focus band becomes selected
  useEffect(() => {
    const scroller = scrollRef.current;
    if (!scroller) return;
    let raf = 0;
    const onScroll = () => {
      if (suppressScrollSync.current) return;
      cancelAnimationFrame(raf);
      raf = requestAnimationFrame(() => {
        const sr = scroller.getBoundingClientRect();
        const focusY = sr.top + sr.height * 0.32;
        const arts = scroller.querySelectorAll("[data-node-id]");
        let best = null;
        let bestDist = Infinity;
        arts.forEach((a) => {
          const r = a.getBoundingClientRect();
          // use the mid-point of the article's head
          const y = r.top + 16;
          const d = Math.abs(y - focusY);
          if (d < bestDist) {
            bestDist = d;
            best = a.getAttribute("data-node-id");
          }
        });
        if (best && best !== selectedId) {
          // mark that this selection change came from scroll — so we don't re-scroll
          suppressScrollSync.current = true;
          setSelectedId(best);
          requestAnimationFrame(() => {
            suppressScrollSync.current = false;
          });
        }
      });
    };
    scroller.addEventListener("scroll", onScroll, { passive: true });
    return () => scroller.removeEventListener("scroll", onScroll);
  }, [selectedId, setSelectedId]);

  const n = NODES[selectedId];
  if (!n) return <aside className="op-side op-side-empty" />;

  const selIdx = transcript.indexOf(selectedId);
  const isHead = selectedId === currentHeadId;

  return (
    <aside className="op-side">
      <header className="op-side-head">
        <div className="op-side-head-row">
          <span className="op-side-title">BRANCH</span>
          <span className="op-side-id">
            {selIdx + 1} / {transcript.length}
          </span>
          <span className="op-side-flex" />
          {isHead ? (
            <span className="op-meta-head">▶ head</span>
          ) : (
            <button
              className="op-side-btn"
              onClick={() => setSelectedId(currentHeadId)}
              title="jump to current head"
            >
              ▸ jump to head
            </button>
          )}
        </div>
        <div className="op-side-head-row op-side-head-meta">
          <span className="op-meta-node">{n.id}</span>
          <span className="op-meta-sep">·</span>
          <span className="op-meta-depth">depth {DATA.depth[n.id] ?? 0}</span>
          <span className="op-meta-sep">·</span>
          <span className="op-meta-kids">
            {n.children.length} child{n.children.length === 1 ? "" : "ren"}
          </span>
          <span className="op-side-flex" />
          <span className="op-side-hint">scroll ↕ walks the tree</span>
        </div>
      </header>

      <div className="op-side-body" ref={scrollRef}>
        <div className="op-side-spacer-top" />
        {transcript.map((id) => {
          const node = NODES[id];
          const forks = node.children.length;
          return (
            <MessageBubble
              key={id}
              node={node}
              active={id === selectedId}
              onClick={() => setSelectedId(id)}
              hasForks={forks > 1}
              forkCount={forks}
              onFork={() => setSelectedId(id)}
            />
          );
        })}
        <div className="op-side-spacer-bot" />
      </div>

      <footer className="op-side-foot">
        <div className="op-side-foot-stats">
          <span>{transcript.length} messages in this branch</span>
        </div>
        <div className="op-side-foot-actions">
          <button
            className="op-foot-btn"
            disabled={isHead}
            onClick={() => setCurrentHeadId(selectedId)}
            title="make this the active head"
          >
            checkout
          </button>
          <button className="op-foot-btn" title="fork from here">
            branch
          </button>
        </div>
      </footer>
    </aside>
  );
}

window.SidePanel = SidePanel;
