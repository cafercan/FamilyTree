// Rust'ta hesaplanan TreeLayout'u SVG olarak çizer; yakınlaştırma ve kaydırma.
"use strict";

const NODE_W = 180;
const NODE_H = 64;

const Tree = {
  scale: 1,
  tx: 0,
  ty: 0,
  layout: null,
  onSelect: null,
  onOpen: null,
  selectedId: null,

  init(onSelect, onOpen) {
    this.onSelect = onSelect;
    this.onOpen = onOpen;
    const svg = document.getElementById("svg");
    svg.addEventListener("wheel", (e) => {
      e.preventDefault();
      const r = svg.getBoundingClientRect();
      const mx = e.clientX - r.left;
      const my = e.clientY - r.top;
      const k = Math.exp(-e.deltaY * 0.0015);
      const ns = Math.min(4, Math.max(0.1, this.scale * k));
      this.tx = mx - ((mx - this.tx) * ns) / this.scale;
      this.ty = my - ((my - this.ty) * ns) / this.scale;
      this.scale = ns;
      this.apply();
    }, { passive: false });
    let drag = null;
    svg.addEventListener("pointerdown", (e) => {
      if (e.button !== 0) return;
      drag = { x: e.clientX, y: e.clientY, tx: this.tx, ty: this.ty, moved: false };
      svg.setPointerCapture(e.pointerId);
    });
    svg.addEventListener("pointermove", (e) => {
      if (!drag) return;
      const dx = e.clientX - drag.x;
      const dy = e.clientY - drag.y;
      if (Math.abs(dx) + Math.abs(dy) > 3) {
        drag.moved = true;
        svg.classList.add("dragging");
      }
      this.tx = drag.tx + dx;
      this.ty = drag.ty + dy;
      this.apply();
    });
    const end = () => {
      if (drag && drag.moved) this.suppressClick = true;
      drag = null;
      svg.classList.remove("dragging");
    };
    svg.addEventListener("pointerup", end);
    svg.addEventListener("pointercancel", end);
  },

  apply() {
    document.getElementById("viewport").style.transform = `translate(${this.tx}px,${this.ty}px) scale(${this.scale})`;
  },

  fit() {
    const l = this.layout;
    if (!l) return;
    const svg = document.getElementById("svg");
    const r = svg.getBoundingClientRect();
    const pad = 30;
    const k = Math.min((r.width - pad * 2) / Math.max(l.width, 1), (r.height - pad * 2) / Math.max(l.height, 1), 1.4);
    this.scale = Math.max(0.1, k);
    this.tx = (r.width - l.width * this.scale) / 2;
    this.ty = (r.height - l.height * this.scale) / 2;
    this.apply();
  },

  /** Kök kişiye (veya tepe düğümüne) odaklanır */
  draw(layout, keepView) {
    this.layout = layout;
    const vp = document.getElementById("viewport");
    vp.replaceChildren();
    if (!layout || !layout.nodes.length) return;
    vp.setAttribute("class", "proj-" + layout.projection);
    const byKey = new Map(layout.nodes.map((n) => [n.key, n]));
    const proj = layout.projection;
    const center = proj === "Radial" ? layout.nodes.find((n) => n.depth === 0) : null;
    // Katmanlar: normal çizgiler < düğümler < ortak atalara giden renkli çizgiler (en üstte,
    // zemin renginde kenarlıkla: hiçbir yazının altında kalmaz)
    const edges = s("g", { class: "edges" });
    const nodes = s("g", { class: "nodes" });
    const merges = s("g", { class: "merges" });
    const links = s("g", { class: "twin-links" });
    const groups = new Set(layout.nodes.filter((n) => n.shared_group !== null && n.shared_group !== undefined).map((n) => n.shared_group));
    const defs = s("defs", {},
      s("marker", { id: "arrow", viewBox: "0 0 10 10", refX: "8", refY: "5", markerWidth: "6", markerHeight: "6", orient: "auto-start-reverse" },
        s("path", { d: "M0,0 L10,5 L0,10 z", class: "spouse-arrow-head" })),
      [...groups].map((g) =>
        s("marker", { id: `arrow-g${g}`, viewBox: "0 0 10 10", refX: "9", refY: "5", markerWidth: "5", markerHeight: "5", orient: "auto-start-reverse" },
          s("path", { d: "M0,0 L10,5 L0,10 z", style: `fill: ${groupColor(g)}` }))));
    vp.append(defs, edges, nodes, merges, links);
    this.links = links;
    this.byKey = byKey;
    // Dairesel görünümde yalnızca en dıştaki düğümlerin adı yarıçap doğrultusunda döner;
    // iç düğümlerinki yatay yazılır (aynı doğrultudaki adlar üst üste binmesin)
    const inner = new Set(layout.edges.map((e) => e.from));
    for (const n of layout.nodes) nodes.append(this.nodeEl(proj, n, center, inner.has(n.key)));
    // Yatayda çizgiler adın bittiği yerden başlar: ad genişliklerini çizimden ölç
    const labelW = new Map();
    if (proj === "Horizontal") {
      for (const t of nodes.querySelectorAll("text.lbl")) {
        let w = 0;
        try {
          w = t.getComputedTextLength();
        } catch {
          w = t.textContent.length * 7;
        }
        labelW.set(t.dataset.key, w);
      }
    }
    for (const e of layout.edges) {
      const a = byKey.get(e.from);
      const b = byKey.get(e.to);
      if (!a || !b) continue;
      const grouped = e.group !== null && e.group !== undefined;
      const d = this.edgePath(proj, a, b, center, labelW, e.merge);
      if (!e.merge) {
        // Ağaç çizgisi normal katmanda; ortak kişiye gidiyorsa onun renginde
        edges.append(s("path", {
          class: "edge" + (e.highlight ? " highlight" : "") + (grouped ? " grouped" : ""),
          d,
          style: grouped ? `stroke: ${groupColor(e.group)}` : null,
        }));
        continue;
      }
      const color = groupColor(e.group);
      merges.append(
        s("path", { class: "merge-casing", d, "data-group": e.group }),
        s("path", {
          class: "merge-edge" + (e.merge ? " cross" : ""),
          d,
          "data-group": e.group,
          style: `stroke: ${color}`,
          "marker-end": `url(#arrow-g${e.group})`,
        }),
      );
    }
    // Üzerine gelme: birleşme okları şeffaflaşır (altındaki yazı okunsun); ortak kişide
    // aynı kişinin tüm kartları parlar, ikiz kartlar arasında geçici bağlantı belirir
    for (const g of nodes.querySelectorAll(".node")) {
      g.addEventListener("mouseenter", () => this.hover(g, true));
      g.addEventListener("mouseleave", () => this.hover(g, false));
    }
    const legend = document.getElementById("legend");
    if (legend) {
      const twins = layout.nodes.some((n) => n.twin_of);
      legend.hidden = !groups.size;
      legend.textContent = twins
        ? "Aynı renkli kartlar aynı kişidir (akraba evliliğinde iki koldan gelen ortak ata). Kesikli çerçeveli kart ikizdir: üzerine gelince ikisi arasında bağlantı belirir, tıklayınca asıl karta gidilir."
        : "Renkli çizgiler ortak atalara (akraba evliliği) iki koldan gelen bağlardır; kesikli olan ikinci koldur. Bir kişinin üzerine gelince oklar şeffaflaşır.";
    }
    if (!keepView) this.fit();
  },

  hover(g, on) {
    const vp = document.getElementById("viewport");
    vp.classList.toggle("hovering", on);
    const grp = g.dataset.group;
    if (this.links) this.links.replaceChildren();
    if (grp === undefined) return;
    vp.querySelectorAll(`.node[data-group="${grp}"]`).forEach((x) => x.classList.toggle("glow", on));
    vp.querySelectorAll(`.merges [data-group="${grp}"]`).forEach((x) => x.classList.toggle("focus", on));
    if (!on || !this.layout) return;
    // İkiz kartlarla asıl kart arasında geçici, hareketli bağlantı
    const same = this.layout.nodes.filter((n) => String(n.shared_group) === grp);
    const main = same.find((n) => !n.twin_of);
    for (const t of same.filter((n) => n.twin_of)) {
      if (!main) break;
      const d = this.twinLinkPath(this.layout.projection, t, main);
      this.links.append(
        s("path", { class: "merge-casing", d }),
        s("path", { class: "twin-link", d, style: `stroke: ${groupColor(Number(grp))}` }),
      );
    }
  },

  /** İkiz kartla asıl kartı, yazıların üzerinden geçmeden bağlayan kavis */
  twinLinkPath(proj, a, b) {
    if (proj === "Vertical") {
      // Kartların üst kenarından, satırın üstünden kavis
      const y1 = a.y - NODE_H / 2 - 3;
      const y2 = b.y - NODE_H / 2 - 3;
      const top = Math.min(y1, y2) - Math.max(36, Math.abs(a.x - b.x) * 0.12);
      return `M${a.x},${y1} C${a.x},${top} ${b.x},${top} ${b.x},${y2}`;
    }
    if (proj === "Horizontal") {
      // Noktaların solundaki boşluktan
      const left = Math.min(a.x, b.x) - Math.max(28, Math.abs(a.y - b.y) * 0.25);
      return `M${a.x - 8},${a.y} C${left},${a.y} ${left},${b.y} ${b.x - 8},${b.y}`;
    }
    // Dairesel: merkeze doğru içeriden
    const root = this.layout.nodes.find((n) => n.depth === 0) || { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 };
    const pull = (p) => [p.x + (root.x - p.x) * 0.45, p.y + (root.y - p.y) * 0.45];
    const [c1x, c1y] = pull(a);
    const [c2x, c2y] = pull(b);
    return `M${a.x},${a.y} C${c1x},${c1y} ${c2x},${c2y} ${b.x},${b.y}`;
  },

  /** Seçili kişiyi ağacı yeniden çizmeden işaretler */
  markSelected(id) {
    this.selectedId = id;
    document.querySelectorAll("#viewport .node").forEach((g) => g.classList.toggle("selected", Number(g.dataset.person) === id));
  },

  /** Görünümü bir düğüme yumuşakça kaydırır */
  centerOn(key) {
    const n = this.layout && this.layout.nodes.find((x) => x.key === key);
    if (!n) return;
    const r = document.getElementById("svg").getBoundingClientRect();
    this.tx = r.width / 2 - n.x * this.scale;
    this.ty = r.height / 2 - n.y * this.scale;
    const vp = document.getElementById("viewport");
    vp.classList.add("panning");
    this.apply();
    setTimeout(() => vp.classList.remove("panning"), 450);
  },

  /** a → b çizgisi. `merge`: ikinci koldan ortak düğüme bağlanan kavisli çizgi. */
  edgePath(proj, a, b, center, labelW, merge) {
    if (proj === "Vertical") {
      const up = b.y < a.y; // ata ağacında çocuk altta
      const y1 = a.y + (up ? -NODE_H / 2 : NODE_H / 2);
      const y2 = b.y + (up ? NODE_H / 2 : -NODE_H / 2) + (up ? 4 : -4);
      if (merge) {
        // Ağaç çizgilerinin yatay çubuğuyla çakışmasın: kavisli, doğrudan kutuya
        const k = Math.max(40, Math.abs(y2 - y1) * 0.5);
        return `M${a.x},${y1} C${a.x},${y1 + (up ? -k : k)} ${b.x},${y2 + (up ? k : -k)} ${b.x},${y2}`;
      }
      const my = (y1 + y2) / 2;
      return `M${a.x},${y1} V${my} H${b.x} V${y2}`;
    }
    if (proj === "Horizontal") {
      // Soldaki düğümün adının bittiği yerden sağdakinin noktasına
      const [L, R] = a.x <= b.x ? [a, b] : [b, a];
      const x1 = L.x + 10 + (labelW.get(L.key) || 0) + 14;
      const x2 = R.x - 8;
      const mx = (x1 + x2) / 2;
      const path = `M${x1},${L.y} C${mx},${L.y} ${mx},${R.y} ${x2},${R.y}`;
      if (a.x <= b.x) return path;
      // Ok b'de bitsin diye ters yön
      return `M${x2},${R.y} C${mx},${R.y} ${mx},${L.y} ${x1},${L.y}`;
    }
    if (center) {
      // Dairesel: iki düğümün yarıçaplarının ortasından geçen eğri
      const rA = Math.hypot(a.x - center.x, a.y - center.y);
      const rB = Math.hypot(b.x - center.x, b.y - center.y);
      const rm = (rA + rB) / 2;
      const p = (ang) => [center.x + rm * Math.cos(ang), center.y + rm * Math.sin(ang)];
      const [c1x, c1y] = a.depth === 0 ? [a.x, a.y] : p(a.angle);
      const [c2x, c2y] = p(b.angle);
      return `M${a.x},${a.y} C${c1x},${c1y} ${c2x},${c2y} ${b.x},${b.y}`;
    }
    return `M${a.x},${a.y} L${b.x},${b.y}`;
  },

  nodeEl(proj, n, center, isInner) {
    const cls = ["node", n.sex === "E" ? "male" : n.sex === "K" ? "female" : "unknown"];
    if (n.placeholder) cls.push("placeholder");
    if (n.highlight) cls.push("highlight");
    if (n.shared) cls.push("shared");
    if (n.twin_of) cls.push("twin");
    if (n.person_id === this.selectedId) cls.push("selected");
    const name = n.label + deathMark(n.alive) + (proj !== "Vertical" && n.twin_of ? " ↔" : "");
    const spouses = n.spouse_labels.join(", ");
    const sub = [n.years, spouses ? "⚭ " + spouses : ""].filter(Boolean).join("  ");
    const g = s("g", {
      class: cls.join(" "),
      "data-person": n.person_id,
      "data-group": n.shared ? n.shared_group : null,
      style: n.shared ? `--gc: ${groupColor(n.shared_group)}` : null,
      onclick: () => {
        if (this.suppressClick) {
          this.suppressClick = false;
          return;
        }
        this.onSelect && this.onSelect(n.person_id);
        if (n.twin_of) this.centerOn(n.twin_of);
      },
      ondblclick: () => this.onOpen && this.onOpen(n.person_id),
    });
    g.append(s("title", {}, n.twin_of
      ? `${n.label} — ikiz kart: bu kişi ağaçta başka bir kolda da var (akraba evliliği). Tıklayınca asıl karta gidilir.`
      : n.shared ? `${n.label} — ortak ata: birden çok koldan bağlı (akraba evliliği)` : n.label));
    const badge = n.twin_of ? "↔ diğer kolda" : "ortak ata";
    if (proj === "Vertical") {
      g.append(
        s("rect", { x: n.x - NODE_W / 2, y: n.y - NODE_H / 2, width: NODE_W, height: NODE_H, rx: 8 }),
        s("text", { class: "lbl", "data-key": n.key, x: n.x, y: n.y - 4, "text-anchor": "middle" }, clip(name, 22)),
        s("text", { class: "sub", x: n.x, y: n.y + 14, "text-anchor": "middle" }, clip(sub, 28)),
      );
      if (n.shared) g.append(s("text", { class: "shared-badge", x: n.x + NODE_W / 2 - 6, y: n.y - NODE_H / 2 + 12, "text-anchor": "end" }, badge));
      return g;
    }
    g.append(s("circle", { cx: n.x, cy: n.y, r: 6 }));
    if (proj === "Horizontal") {
      if (!spouses) {
        g.append(s("text", { class: "lbl", "data-key": n.key, x: n.x + 10, y: n.y + 4 }, name, s("tspan", { class: "sub", dx: 6 }, n.years)));
        return g;
      }
      // Eş adı alta: noktadan çıkan kavisli ok ve ⚭
      g.append(
        s("text", { class: "lbl", "data-key": n.key, x: n.x + 10, y: n.y - 2 }, name, s("tspan", { class: "sub", dx: 6 }, n.years)),
        s("path", { class: "spouse-arrow", d: `M${n.x},${n.y + 6} Q${n.x},${n.y + 16} ${n.x + 12},${n.y + 16}`, "marker-end": "url(#arrow)" }),
        s("text", { class: "spouse", x: n.x + 16, y: n.y + 20 }, "⚭ " + spouses),
      );
      return g;
    }
    // Dairesel: metin yarıçap doğrultusunda, sol yarıda ters çevrilir
    if (n.depth === 0) {
      g.append(s("text", { x: n.x, y: n.y - 12, "text-anchor": "middle" }, name));
      return g;
    }
    if (isInner) {
      g.append(s("text", { x: n.x + 9, y: n.y - 7 }, name));
      return g;
    }
    let deg = (n.angle * 180) / Math.PI;
    const flip = deg > 90 && deg < 270;
    if (flip) deg -= 180;
    g.append(
      s("text", {
        x: n.x,
        y: n.y,
        dx: flip ? -10 : 10,
        dy: 4,
        "text-anchor": flip ? "end" : "start",
        transform: `rotate(${deg} ${n.x} ${n.y})`,
      }, name),
    );
    return g;
  },
};

function clip(t, n) {
  return t.length > n ? t.slice(0, n - 1) + "…" : t;
}

/** Ortak ata renk grubu → CSS rengi (açık/koyu temada ayrı tonlar) */
function groupColor(g) {
  return `var(--g${g % 6})`;
}
