// Ortak yardımcılar: DOM üretimi, Tauri çağrısı, bildirim ve diyalog.
// Kişi adları gibi veriler her zaman textContent ile yazılır (HTML olarak yorumlanmaz).
"use strict";

const SVG_NS = "http://www.w3.org/2000/svg";

/** h("button", {class: "x", onclick: fn}, "Metin", childNode) */
function h(tag, attrs, ...children) {
  const el = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs || {})) {
    if (v === undefined || v === null || v === false) continue;
    if (k.startsWith("on")) el.addEventListener(k.slice(2), v);
    else if (k === "class") el.className = v;
    else if (v === true) el.setAttribute(k, "");
    else el.setAttribute(k, v);
  }
  for (const c of children.flat()) {
    if (c === undefined || c === null || c === false) continue;
    el.append(c instanceof Node ? c : document.createTextNode(String(c)));
  }
  return el;
}

function s(tag, attrs, ...children) {
  const el = document.createElementNS(SVG_NS, tag);
  for (const [k, v] of Object.entries(attrs || {})) {
    if (v === undefined || v === null) continue;
    if (k.startsWith("on")) el.addEventListener(k.slice(2), v);
    else el.setAttribute(k, v);
  }
  for (const c of children.flat()) {
    if (c === undefined || c === null) continue;
    el.append(c instanceof Node ? c : document.createTextNode(String(c)));
  }
  return el;
}

async function call(cmd, args) {
  try {
    return await window.__TAURI__.core.invoke(cmd, args || {});
  } catch (e) {
    toast(String(e), true);
    throw e;
  }
}

let toastTimer = null;
function toast(msg, error) {
  const t = document.getElementById("toast");
  t.textContent = msg;
  t.className = "toast" + (error ? " error" : "");
  t.hidden = false;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => (t.hidden = true), error ? 7000 : 3500);
}

/**
 * Diyaloğu içerikle açar; `buttons`: [{label, value, primary, danger}]. Basılan düğmenin
 * değeriyle (Esc ile kapatılırsa null) çözülür. `onButton(value)` false döndürürse
 * diyalog kapanmaz (form doğrulama için).
 */
function openDialog(title, body, buttons, onButton) {
  const dlg = document.getElementById("dlg");
  dlg.replaceChildren();
  return new Promise((resolve) => {
    const done = (v) => {
      dlg.close();
      resolve(v);
    };
    const row = h("div", { class: "dlg-buttons" });
    for (const b of buttons) {
      row.append(
        h("button", {
          class: b.primary ? "primary" : b.danger ? "danger" : "",
          onclick: async () => {
            if (onButton && b.value !== null && (await onButton(b.value)) === false) return;
            done(b.value);
          },
        }, b.label),
      );
    }
    dlg.append(h("h2", {}, title), body, row);
    dlg.onclose = () => resolve(null);
    dlg.oncancel = () => resolve(null);
    dlg.showModal();
  });
}

async function confirmBox(title, text, okLabel, danger) {
  const v = await openDialog(title, h("p", {}, text), [
    { label: "Vazgeç", value: null },
    { label: okLabel || "Tamam", value: true, primary: !danger, danger },
  ]);
  return v === true;
}

function fullName(p) {
  const n = `${p.given_name || ""} ${p.surname || ""}`.trim();
  return n || "Bilinmiyor";
}

function yearsOf(p) {
  const y = (d) => {
    const m = /(\d{4})$/.exec(d || "");
    return m ? m[1] : "";
  };
  const b = y(p.birth_date);
  const d = y(p.death_date);
  if (!b && !d) return "";
  return d ? `${b || "?"}–${d}` : b;
}

function sexClass(sex) {
  return sex === "E" ? "male" : sex === "K" ? "female" : "unknown";
}

/** Tek satırlık metin sorar; vazgeçilirse null. */
async function promptBox(title, label, initial, okLabel) {
  const input = h("input", { type: "text", style: "width: 100%" });
  input.value = initial || "";
  setTimeout(() => input.select(), 0);
  input.addEventListener("keydown", (e) => {
    if (e.key === "Enter") document.querySelector("#dlg .dlg-buttons .primary")?.click();
  });
  const v = await openDialog(title, h("label", { class: "note" }, label, h("br"), input), [
    { label: "Vazgeç", value: null },
    { label: okLabel || "Tamam", value: "ok", primary: true },
  ], () => {
    if (!input.value.trim()) {
      toast("Ad boş olamaz", true);
      return false;
    }
    return true;
  });
  return v === "ok" ? input.value.trim() : null;
}

// Görünüm ayarları: yalnızca bu bilgisayarda (tarayıcı deposunda) saklanır
const SETTINGS_KEY = "familytree.ayarlar";
const settings = { death: "cross", collapse: "twins", theme: "system", accent: "green" };
try {
  Object.assign(settings, JSON.parse(localStorage.getItem(SETTINGS_KEY) || "{}"));
} catch {
  /* depo kullanılamıyorsa varsayılanlar */
}

function saveSettings() {
  try {
    localStorage.setItem(SETTINGS_KEY, JSON.stringify(settings));
  } catch {
    /* kaydedilemezse oturum boyunca geçerli */
  }
}

/** Vefat etmiş kişinin adına eklenecek işaret (ayarlardan seçilir) */
function deathMark(alive) {
  if (alive !== false) return "";
  return settings.death === "o" ? " (ö.)" : settings.death === "none" ? "" : " †";
}

/** Vurgu renkleri: [açık tema, koyu tema] */
const ACCENTS = {
  green: { label: "Yeşil", light: "#2e7d5a", dark: "#4caf84" },
  blue: { label: "Mavi", light: "#2f6fd1", dark: "#6fa8ff" },
  purple: { label: "Mor", light: "#7a45c4", dark: "#b98cff" },
  burgundy: { label: "Bordo", light: "#9c2d4a", dark: "#e57a98" },
  orange: { label: "Turuncu", light: "#c2610c", dark: "#ffad42" },
};

const darkQuery = window.matchMedia ? window.matchMedia("(prefers-color-scheme: dark)") : null;

function isDark() {
  if (settings.theme === "dark") return true;
  if (settings.theme === "light") return false;
  return !!(darkQuery && darkQuery.matches);
}

/** Tema ve vurgu rengini belgeye uygular */
function applyTheme() {
  const root = document.documentElement;
  if (settings.theme === "light" || settings.theme === "dark") root.dataset.theme = settings.theme;
  else delete root.dataset.theme;
  const a = ACCENTS[settings.accent] || ACCENTS.green;
  const dark = isDark();
  root.style.setProperty("--accent", dark ? a.dark : a.light);
  root.style.setProperty("--accent-text", dark ? "#0d1310" : "#ffffff");
  root.style.colorScheme = dark ? "dark" : "light";
}

if (darkQuery) {
  try {
    darkQuery.addEventListener("change", applyTheme);
  } catch {
    /* eski motorlar */
  }
}
applyTheme();
