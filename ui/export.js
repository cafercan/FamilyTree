// Açık ağaç görünümünü PNG, SVG, PDF ya da metin olarak kaydetme.
"use strict";

// Kaydedilen SVG'de korunacak görünüm özellikleri (CSS değişkenleri çözülmüş haliyle)
const EXPORT_PROPS = [
  "fill", "fill-opacity", "stroke", "stroke-width", "stroke-dasharray", "stroke-linecap",
  "stroke-linejoin", "opacity", "font-size", "font-family", "font-weight", "paint-order",
];

/** Görünümü, stilleri gömülü bağımsız bir SVG metnine çevirir. `light`: açık temayla. */
function standaloneSvg(light) {
  const layout = Tree.layout;
  if (!layout) return null;
  if (Tree.links) Tree.links.replaceChildren();
  const saved = { theme: settings.theme, accent: settings.accent };
  if (light && isDark()) {
    settings.theme = "forest-light";
    applyTheme();
  }
  const vp = document.getElementById("viewport");
  const clone = vp.cloneNode(true);
  const src = vp.querySelectorAll("*");
  const dst = clone.querySelectorAll("*");
  src.forEach((el, i) => {
    const cs = getComputedStyle(el);
    const st = EXPORT_PROPS.map((p) => [p, cs.getPropertyValue(p)]).filter(([, v]) => v).map(([p, v]) => `${p}:${v}`);
    dst[i].setAttribute("style", st.join(";"));
    dst[i].removeAttribute("class");
  });
  clone.removeAttribute("style");
  clone.removeAttribute("class");
  clone.removeAttribute("id");
  const bg = getComputedStyle(document.body).backgroundColor;
  Object.assign(settings, saved);
  applyTheme();
  const w = Math.ceil(layout.width);
  const hgt = Math.ceil(layout.height);
  const body = new XMLSerializer().serializeToString(clone);
  return {
    text: `<?xml version="1.0" encoding="UTF-8"?>\n<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${hgt}" viewBox="0 0 ${w} ${hgt}"><rect width="100%" height="100%" fill="${bg}"/>${body}</svg>`,
    width: w,
    height: hgt,
  };
}

/** SVG'yi tuvale çizip PNG ya da JPEG baytlarına çevirir. */
function rasterize(svg, type) {
  const scale = Math.max(0.25, Math.min(2, 8000 / svg.width, 8000 / svg.height, Math.sqrt(40e6 / (svg.width * svg.height))));
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.onload = () => {
      const canvas = document.createElement("canvas");
      canvas.width = Math.round(svg.width * scale);
      canvas.height = Math.round(svg.height * scale);
      const ctx = canvas.getContext("2d");
      ctx.drawImage(img, 0, 0, canvas.width, canvas.height);
      try {
        canvas.toBlob(async (blob) => {
          if (!blob) return reject(new Error("Görüntü oluşturulamadı"));
          resolve({ bytes: new Uint8Array(await blob.arrayBuffer()), w: canvas.width, h: canvas.height });
        }, type, 0.92);
      } catch (e) {
        reject(e);
      }
    };
    img.onerror = () => reject(new Error("Çizim görüntüye çevrilemedi"));
    img.src = "data:image/svg+xml;charset=utf-8," + encodeURIComponent(svg.text);
  });
}

function safeFileName(s) {
  return s.replace(/[\\/:*?"<>|]+/g, "-").replace(/\s+/g, " ").trim();
}

async function sendExport(kind, bytes, name, extra) {
  const headers = Object.assign({ "x-kind": kind, "x-name": encodeURIComponent(name) }, extra || {});
  try {
    return await window.__TAURI__.core.invoke("save_export", bytes, { headers });
  } catch (e) {
    toast(String(e), true);
    return false;
  }
}

/** "Kaydet" düğmesi: biçim seçtirip kaydeder. */
async function exportFlow() {
  const tabNames = { ancestors: "ata ağacı", descendants: "soy ağacı", both: "ata ve soy", relation: "akrabalık" };
  const who = state.tab === "relation"
    ? [state.relA, state.relB].map((id) => fullName(state.byId.get(id) || {})).join(" - ")
    : fullName(state.byId.get(state.selected) || {});
  const name = safeFileName(`${state.tree} - ${who} - ${tabNames[state.tab] || ""}`);
  if (state.view === "Text") {
    const text = $("text-view").textContent;
    if (!text) return toast("Kaydedilecek metin yok", true);
    if (await sendExport("txt", new TextEncoder().encode(text), name)) toast("Kaydedildi");
    return;
  }
  if (!Tree.layout) return toast("Kaydedilecek ağaç yok", true);
  const opt = (v, label, note, checked) => h("label", {},
    h("input", { type: "radio", name: "fmt", value: v, checked }), h("span", {}, h("b", {}, label), " — ", note));
  const light = h("input", { type: "checkbox", checked: true });
  const body = h("div", { class: "export-options" },
    opt("png", "PNG görüntü", "paylaşmak ve mesajla göndermek için", true),
    opt("pdf", "PDF", "yazdırmak için; sayfa ağacın boyutunda", false),
    opt("svg", "SVG", "vektörel; sınırsız büyütülebilir, yazılar seçilebilir", false),
    h("label", { class: "note" }, light, "Açık tema ile kaydet (yazdırmaya uygun)"));
  const v = await openDialog("Ağacı kaydet", body, [
    { label: "Vazgeç", value: null },
    { label: "Kaydet", value: "ok", primary: true },
  ]);
  if (v !== "ok") return;
  const kind = body.querySelector('input[name="fmt"]:checked').value;
  const svg = standaloneSvg(light.checked);
  try {
    let ok = false;
    if (kind === "svg") ok = await sendExport("svg", new TextEncoder().encode(svg.text), name);
    else if (kind === "png") ok = await sendExport("png", (await rasterize(svg, "image/png")).bytes, name);
    else {
      const jpg = await rasterize(svg, "image/jpeg");
      ok = await sendExport("pdf", jpg.bytes, name, {
        "x-px-w": String(jpg.w), "x-px-h": String(jpg.h), "x-css-w": String(svg.width), "x-css-h": String(svg.height),
      });
    }
    if (ok) toast("Kaydedildi");
  } catch (e) {
    toast(`Görüntü oluşturulamadı (${e.message || e}); SVG olarak kaydetmeyi deneyin`, true);
  }
}
