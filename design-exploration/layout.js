// Tree layout: Reingold–Tilford-ish, simplified.
// Produces {x,y} per node id for vertical (top→down) and horizontal (left→right) layouts.
// Units are abstract; consumers scale to pixels.

(function () {
  const { nodes, root } = window.OQTO_DATA;

  // --- vertical layout: x = in-order index among leaves; y = depth ---
  // Use Walker-lite: assign each subtree a horizontal extent; center parents over children.
  function layoutVertical() {
    const pos = {}; // id -> {x, y}
    const COL = 1; // unit spacing
    const ROW = 1;

    function assign(id, depth) {
      const n = nodes[id];
      if (n.children.length === 0) {
        pos[id] = { x: null, y: depth * ROW };
        return { width: 1 };
      }
      let totalW = 0;
      const childWidths = [];
      n.children.forEach((cid) => {
        const w = assign(cid, depth + 1).width;
        childWidths.push(w);
        totalW += w;
      });
      pos[id] = { x: null, y: depth * ROW, _w: totalW, _cw: childWidths };
      return { width: Math.max(1, totalW) };
    }
    assign(root, 0);

    // Second pass: place using offsets
    function place(id, leftX) {
      const n = nodes[id];
      const p = pos[id];
      if (n.children.length === 0) {
        p.x = leftX * COL;
        return 1;
      }
      let offset = leftX;
      const widths = [];
      n.children.forEach((cid) => {
        const w = place(cid, offset);
        widths.push(w);
        offset += w;
      });
      // center parent over children
      const first = pos[n.children[0]].x;
      const last = pos[n.children[n.children.length - 1]].x;
      p.x = (first + last) / 2;
      return widths.reduce((a, b) => a + b, 0);
    }
    place(root, 0);
    // cleanup
    Object.values(pos).forEach((p) => {
      delete p._w;
      delete p._cw;
    });
    return pos;
  }

  // --- horizontal layout: swap axes ---
  function layoutHorizontal() {
    const v = layoutVertical();
    const pos = {};
    Object.keys(v).forEach((id) => {
      pos[id] = { x: v[id].y, y: v[id].x };
    });
    return pos;
  }

  // Compute bbox of a pos map
  function bbox(pos) {
    let minX = Infinity,
      minY = Infinity,
      maxX = -Infinity,
      maxY = -Infinity;
    Object.values(pos).forEach((p) => {
      if (p.x < minX) minX = p.x;
      if (p.x > maxX) maxX = p.x;
      if (p.y < minY) minY = p.y;
      if (p.y > maxY) maxY = p.y;
    });
    return { minX, minY, maxX, maxY, w: maxX - minX, h: maxY - minY };
  }

  window.OQTO_LAYOUT = {
    vertical: layoutVertical,
    horizontal: layoutHorizontal,
    bbox,
  };
})();
