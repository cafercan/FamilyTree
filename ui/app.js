// Uygulama durumu, kişi listesi, sekmeler ve kişi ayrıntısı.
"use strict";

const state = {
  persons: [],
  byId: new Map(),
  selected: null,
  tab: "both",
  view: "Horizontal",
  gens: 8,
  relA: null,
  relB: null,
  search: "",
};

const $ = (id) => document.getElementById(id);

state.tree = "";

function renderTrees(list) {
  state.tree = list.current;
  const sel = $("tree-select");
  sel.replaceChildren(...list.names.map((n) => h("option", { value: n, selected: n === list.current }, n)));
  document.title = `FamilyTree — ${list.current}`;
}

/** Ağaç değişince kişi seçimleri sıfırlanır: ağaçlar birbirinden bağımsızdır. */
async function afterTreeChange(list) {
  renderTrees(list);
  state.selected = null;
  state.root = null;
  state.relA = null;
  state.relB = null;
  state.search = "";
  $("search").value = "";
  await refresh();
}

async function treeMenu() {
  const list = await call("list_trees");
  const others = list.names.filter((n) => n !== list.current);
  const body = h("div", {},
    h("p", {}, "Açık ağaç: ", h("b", {}, list.current)),
    h("button", {
      onclick: async () => {
        document.getElementById("dlg").close();
        const name = await promptBox("Ağacı yeniden adlandır", "Yeni ad", list.current, "Adlandır");
        if (name && name !== list.current) {
          renderTrees(await call("rename_tree", { name }));
          toast(`Ağacın adı "${name}" oldu`);
        }
      },
    }, "Yeniden adlandır"),
    " ",
    h("button", {
      title: "PDF'te satırı olmayan ama baba/ana adı bilinen en üst kuşak kişilerini ekler",
      onclick: async () => {
        document.getElementById("dlg").close();
        const n = await call("complete_top_parents");
        toast(n ? `${n} kişi baba/ana adlarından eklendi` : "Eklenecek kişi bulunamadı");
        if (n) await refresh(true);
      },
    }, "Baba/ana adlarından en üst kuşağı tamamla"),
    others.length ? h("h3", {}, "Diğer ağaçlar") : null,
    h("ul", {}, others.map((n) => h("li", {}, n, " ",
      h("button", {
        class: "danger",
        onclick: async () => {
          document.getElementById("dlg").close();
          if (!(await confirmBox("Ağacı sil", `"${n}" ağacı ve içindeki tüm kişiler kalıcı olarak silinsin mi? Önce yedek almak isteyebilirsiniz.`, "Sil", true))) return;
          renderTrees(await call("delete_tree", { name: n }));
          toast(`"${n}" silindi`);
        },
      }, "Sil")))),
    h("p", { class: "note" }, "Açık olan ağaç silinemez; silmek için önce başka bir ağaca geçin."));
  await openDialog("Soy ağaçları", body, [{ label: "Kapat", value: null }]);
}

async function refresh(keepView) {
  state.persons = await call("list_persons");
  state.byId = new Map(state.persons.map((p) => [p.id, p]));
  if (state.selected !== null && !state.byId.has(state.selected)) state.selected = null;
  if (state.root !== null && state.root !== undefined && !state.byId.has(state.root)) state.root = null;
  if (state.selected === null && state.persons.length) state.selected = state.persons.find((p) => !p.placeholder)?.id ?? state.persons[0].id;
  renderList();
  renderRelationSelects();
  await Promise.all([renderDuplicates(), renderConsanguinity()]);
  await render(keepView);
  await renderDetail();
}

function renderList() {
  const q = state.search.toLocaleLowerCase("tr");
  const ul = $("person-list");
  ul.replaceChildren();
  for (const p of state.persons) {
    const text = `${fullName(p)} ${p.birth_date || ""}`;
    if (q && !fullName(p).toLocaleLowerCase("tr").includes(q)) continue;
    ul.append(
      h("li", {
        class: p.id === state.selected ? "selected" : "",
        title: text,
        onclick: () => select(p.id),
      }, fullName(p) + deathMark(p.alive),
      h("small", {}, [yearsOf(p), p.placeholder ? "yer tutucu" : "", p.province_district].filter(Boolean).join(" · "))),
    );
  }
}

async function renderDuplicates() {
  const list = await call("duplicate_suggestions");
  const ul = $("dup-list");
  ul.replaceChildren();
  $("dup-section").hidden = !list.length;
  for (const [a, b, score] of list) {
    const pa = state.byId.get(a);
    const pb = state.byId.get(b);
    if (!pa || !pb) continue;
    ul.append(h("li", {},
      `${fullName(pa)} (${yearsOf(pa) || "?"}) ↔ ${fullName(pb)} (${yearsOf(pb) || "?"}) — %${score}`,
      h("div", { class: "row" },
        h("button", {
          onclick: async () => {
            if (!(await confirmBox("Kişileri birleştir", `${fullName(pa)} ve ${fullName(pb)} tek kişi olarak birleştirilsin mi? Bağları ve bilgileri tek kayıtta toplanır.`, "Birleştir"))) return;
            await call("merge_persons", { keep: a, drop: b });
            state.selected = a;
            await refresh();
          },
        }, "Birleştir"),
        h("button", {
          onclick: async () => {
            await call("mark_not_same", { a, b });
            await refresh(true);
          },
        }, "Farklı kişiler"))));
  }
}

async function renderConsanguinity() {
  const list = await call("consanguinity_report");
  const btn = $("btn-consang");
  btn.hidden = !list.length;
  btn.textContent = `⚠ Akraba evlilikleri (${list.length})`;
  btn.onclick = async () => {
    const body = h("ul", {}, list.map((it) => {
      const hu = state.byId.get(it.husband);
      const wi = state.byId.get(it.wife);
      const anc = it.relationship.closest.map((c) => fullName(state.byId.get(c.ancestor) || {})).join(", ");
      return h("li", {},
        h("a", {
          href: "#",
          onclick: (e) => {
            e.preventDefault();
            document.getElementById("dlg").close();
            showRelation(it.husband, it.wife);
          },
        }, `${fullName(hu)} ile ${fullName(wi)}`),
        ` — ${fullName(wi)}, ${fullName(hu)} kişisinin ${it.relationship.label.toLocaleLowerCase("tr")}. Ortak ata: ${anc}.`);
    }));
    await openDialog("Akraba evlilikleri", body, [{ label: "Kapat", value: null }]);
  };
}

function renderRelationSelects() {
  for (const [id, key] of [["rel-a", "relA"], ["rel-b", "relB"]]) {
    const sel = $(id);
    sel.replaceChildren(h("option", { value: "" }, "Kişi seçin…"));
    for (const p of state.persons) {
      sel.append(h("option", { value: String(p.id), selected: p.id === state[key] }, `${fullName(p)} ${yearsOf(p) ? "(" + yearsOf(p) + ")" : ""}`));
    }
  }
}

async function render(keepView) {
  const empty = !state.persons.length;
  $("empty").hidden = !empty;
  $("relation-bar").hidden = state.tab !== "relation";
  const text = state.view === "Text";
  $("svg").style.display = text || empty ? "none" : "";
  $("text-view").hidden = !text || empty;
  $("btn-fit").disabled = text;
  if (empty) return;
  if (state.tab === "relation") return renderRelation(keepView);
  // Ağacın kökü: listeden seçilen ya da çift tıklanan kişi; ağaçta tek tıklama kökü değiştirmez
  if (state.root === null || state.root === undefined) state.root = state.selected;
  const root = state.root;
  if (root === null || root === undefined) return;
  if (text) {
    const up = () => call("outline", { id: root, gens: state.gens, ancestors: true });
    const down = () => call("outline", { id: root, gens: state.gens, ancestors: false });
    $("text-view").textContent = state.tab === "both"
      ? `ATALAR\n${await up()}\nTORUNLAR\n${await down()}`
      : await (state.tab === "ancestors" ? up() : down());
    return;
  }
  const layout = await call("tree_layout", {
    kind: state.tab, id: root, gens: state.gens, projection: state.view, twins: settings.collapse !== "merge",
  });
  Tree.selectedId = state.selected;
  Tree.draw(layout, keepView);
}

async function renderRelation(keepView) {
  const label = $("rel-label");
  label.textContent = "";
  Tree.draw(null);
  $("text-view").textContent = "";
  if (state.relA === null || state.relB === null) {
    label.textContent = "İki kişi seçin.";
    return;
  }
  const proj = state.view === "Text" ? "Vertical" : state.view;
  const res = await call("relationship_layout", { a: state.relA, b: state.relB, projection: proj });
  const a = state.byId.get(state.relA);
  const b = state.byId.get(state.relB);
  if (!res) {
    label.textContent = "Ortak ata bulunamadı (kayıtlı bilgilere göre akraba değiller).";
    return;
  }
  const anc = res.relationship.closest.map((c) => fullName(state.byId.get(c.ancestor) || {})).join(", ");
  label.textContent = `${fullName(b)}, ${fullName(a)} kişisinin ${res.relationship.label.toLocaleLowerCase("tr")}. Ortak ata: ${anc}.`;
  if (state.view === "Text") {
    const lines = res.relationship.closest.map((c) => {
      const pa = c.path_a.map((id) => fullName(state.byId.get(id) || {})).join(" → ");
      const pb = c.path_b.map((id) => fullName(state.byId.get(id) || {})).join(" → ");
      return `* ${fullName(state.byId.get(c.ancestor) || {})}\n** ${pa}\n** ${pb}`;
    });
    $("text-view").textContent = lines.join("\n\n");
    return;
  }
  Tree.selectedId = null;
  Tree.draw(res.layout, keepView);
}

function showRelation(a, b) {
  state.relA = a;
  state.relB = b;
  setTab("relation");
  renderRelationSelects();
}

function setTab(tab) {
  state.tab = tab;
  if (tab !== "relation") {
    settings.tab = tab;
    saveSettings();
  }
  for (const b of document.querySelectorAll("[data-tab]")) b.classList.toggle("active", b.dataset.tab === tab);
  render();
}

/** Listeden ya da bağlantıdan seçim: kişi hem seçilir hem ağacın kökü olur. */
async function select(id) {
  state.selected = id;
  state.root = id;
  renderList();
  await renderDetail();
  if (state.tab !== "relation") await render();
  else Tree.markSelected(id);
}

/** Ağaçta tek tıklama: kişi seçilir, bilgileri gösterilir; ağaç yerinde kalır. */
async function selectInTree(id) {
  state.selected = id;
  renderList();
  Tree.markSelected(id);
  await renderDetail();
  const li = document.querySelector("#person-list li.selected");
  if (li) li.scrollIntoView({ block: "nearest" });
}

function parseDate(s) {
  const m = /^(\d{1,2})[./](\d{1,2})[./](\d{4})$/.exec((s || "").trim());
  return m ? new Date(Number(m[3]), Number(m[2]) - 1, Number(m[1])) : null;
}

/** Yaş (hayattaysa bugüne, vefat ettiyse ölüm tarihine göre) */
function ageText(p) {
  const b = parseDate(p.birth_date);
  if (!b) return null;
  const end = p.alive === false ? parseDate(p.death_date) : p.alive === true ? new Date() : null;
  if (!end) return null;
  let age = end.getFullYear() - b.getFullYear();
  if (end.getMonth() < b.getMonth() || (end.getMonth() === b.getMonth() && end.getDate() < b.getDate())) age--;
  if (age < 0 || age > 130) return null;
  return p.alive === false ? `${age} yaşında vefat` : `${age}`;
}

/** Bilgi tablosu etiketinin alan adı (elle düzeltme işareti için) */
function infoField(label) {
  return {
    "Cinsiyet": "sex", "Doğum yeri": "birth_place", "Doğum tarihi": "birth_date", "Durumu": "alive",
    "Ölüm tarihi": "death_date", "Baba adı": "father_name", "Ana adı": "mother_name",
    "Medeni hali": "marital_status", "Nüfusa kayıtlı yer": "province_district",
    "Cilt-Hane-Birey No": "register_no", "Mahalle/Köy": "neighborhood", "T.C. kimlik no": "tc",
  }[label] || "";
}

function linkList(title, persons, onRemove) {
  return h("div", {}, h("b", {}, title),
    persons.length
      ? persons.map((p) => h("span", {},
        h("a", { onclick: () => select(p.id) }, fullName(p) + (p.placeholder ? " (?)" : "")),
        onRemove ? h("button", { title: "Bağı kaldır", onclick: () => onRemove(p) }, "×") : null, " "))
      : "—");
}

async function renderDetail() {
  const box = $("detail");
  if (state.selected === null) {
    box.hidden = true;
    return;
  }
  const d = await call("person_detail", { id: state.selected });
  const p = d.person;
  const age = ageText(p);
  const info = [
    ["Cinsiyet", p.sex === "E" ? "Erkek" : p.sex === "K" ? "Kadın" : ""],
    ["Doğum yeri", p.birth_place],
    ["Doğum tarihi", p.birth_date],
    ["Durumu", p.alive === false ? "Vefat etmiş" : p.alive === true ? "Hayatta" : ""],
    ["Ölüm tarihi", p.death_date],
    [p.alive === false ? "Vefat yaşı" : "Yaşı", age ? age.replace(" yaşında vefat", "") : ""],
    ["Baba adı", p.father_name],
    ["Ana adı", p.mother_name],
    ["Medeni hali", p.marital_status],
    ["Nüfusa kayıtlı yer", p.province_district],
    ["Cilt-Hane-Birey No", p.register_no],
    ["Mahalle/Köy", p.neighborhood],
    ["T.C. kimlik no", p.tc],
  ].filter(([, v]) => v);
  const manual = new Set(p.manual_fields || []);
  const id = p.id;
  const unlinkParent = (role) => async () => {
    await call("remove_parent", { child: id, role });
    await refresh(true);
  };
  const add = (kind, label, preset) => h("button", {
    onclick: async () => {
      const input = await personForm(`${fullName(p)} — ${label}`, null, preset);
      if (!input) return;
      const newId = await call("create_person", { input, relation: { to: id, kind } });
      toast(`${fullName(input)} eklendi`);
      state.selected = newId;
      await refresh(true);
    },
  }, label);
  box.replaceChildren(...[
    h("div", { class: "detail-head" },
      h("h2", {}, fullName(p) + deathMark(p.alive)),
      (state.root ?? state.selected) !== id && state.tab !== "relation"
        ? h("button", { onclick: () => select(id), title: "Ağacı bu kişiden yeniden çiz (ya da kişiye çift tıklayın)" }, "Ağacı bu kişiden başlat")
        : null),
    p.placeholder ? h("p", { class: "warn" }, "Yer tutucu: bu kişinin PDF'te kendi satırı yok; bilgilerini Düzenle ile girebilirsiniz.") : null,
    info.length
      ? h("dl", { class: "info" }, info.map(([k, v]) =>
        h("div", { title: manual.has(infoField(k)) ? "Elle düzeltildi" : "" }, h("dt", {}, k, manual.has(infoField(k)) ? " ✎" : ""), h("dd", {}, v))))
      : null,
    p.notes ? h("div", { class: "notes" }, h("b", {}, "Notlar: "), p.notes) : null,
    h("div", { class: "family" },
      linkList("Babası", d.father ? [d.father] : [], d.father ? unlinkParent("E") : null),
      linkList("Annesi", d.mother ? [d.mother] : [], d.mother ? unlinkParent("K") : null),
      linkList("Eşleri", d.spouses, async (s) => {
        await call("remove_spouse", { a: id, b: s.id });
        await refresh(true);
      }),
      linkList("Kardeşleri", d.siblings),
      linkList("Çocukları", d.children)),
    h("div", { class: "buttons" },
      h("button", {
        onclick: async () => {
          const input = await personForm(`${fullName(p)} — düzenle`, p);
          if (!input) return;
          await call("update_person", { id, input });
          await refresh(true);
        },
      }, "Düzenle"),
      d.father ? null : add("father", "Baba ekle", { sex: "E", given_name: p.father_name }),
      d.mother ? null : add("mother", "Anne ekle", { sex: "K", given_name: p.mother_name }),
      add("sibling", "Kardeş ekle", { father_name: p.father_name, mother_name: p.mother_name }),
      add("child", "Çocuk ekle", p.sex === "E" ? { father_name: p.given_name } : p.sex === "K" ? { mother_name: p.given_name } : {}),
      add("spouse", "Eş ekle", { sex: p.sex === "E" ? "K" : p.sex === "K" ? "E" : null }),
      h("button", { onclick: () => linkExisting(p) }, "Mevcut kişiyle bağla"),
      h("button", {
        onclick: () => {
          state.relA = id;
          if (state.relB === id) state.relB = null;
          setTab("relation");
          renderRelationSelects();
        },
      }, "Akrabalığını bul…"),
      h("button", {
        class: "danger",
        onclick: async () => {
          if (!(await confirmBox("Kişiyi sil", `${fullName(p)} ve tüm bağları silinsin mi? Bu işlem geri alınamaz.`, "Sil", true))) return;
          await call("delete_person", { id });
          state.selected = null;
          await refresh();
        },
      }, "Sil")),
    d.sources.length
      ? h("div", { class: "sources" }, h("b", {}, "Geçtiği PDF'ler: "),
        [...new Map(d.sources.map((x) => [`${x.file_name}|${x.relation_text}`, x])).values()]
          .map((x) => `${x.file_name} — "${x.relation_text}"`).join("; "))
      : h("div", { class: "sources" }, "Elle eklendi (PDF kaynağı yok)"),
  ].filter(Boolean));
  box.hidden = false;
  if (p.sex !== "E" && p.sex !== "K") {
    box.querySelector(".buttons").append(h("span", { class: "note" }, "Çocuk eklemek için cinsiyeti girin."));
  }
}

/** Var olan iki kişi arasında ebeveyn ya da eş bağı kurar. */
async function linkExisting(p) {
  const kind = h("select", {},
    h("option", { value: "father" }, "babası"),
    h("option", { value: "mother" }, "annesi"),
    h("option", { value: "child" }, "çocuğu"),
    h("option", { value: "spouse" }, "eşi"));
  const other = h("select", {}, state.persons.filter((x) => x.id !== p.id).map((x) =>
    h("option", { value: String(x.id) }, `${fullName(x)} ${yearsOf(x) ? "(" + yearsOf(x) + ")" : ""}`)));
  const body = h("div", {}, h("p", {}, other, " kişisi, ", fullName(p), " kişisinin ", kind, " olsun."));
  const v = await openDialog("Mevcut kişiyle bağla", body, [
    { label: "Vazgeç", value: null },
    { label: "Bağla", value: "ok", primary: true },
  ]);
  if (v !== "ok" || !other.value) return;
  const o = Number(other.value);
  const op = state.byId.get(o);
  if (kind.value === "father") await call("set_parent", { child: p.id, parent: o, role: "E" });
  else if (kind.value === "mother") await call("set_parent", { child: p.id, parent: o, role: "K" });
  else if (kind.value === "spouse") await call("set_spouse", { a: p.id, b: o });
  else {
    if (p.sex !== "E" && p.sex !== "K") return toast("Önce cinsiyeti girin", true);
    await call("set_parent", { child: o, parent: p.id, role: p.sex });
  }
  toast(`${fullName(op)} bağlandı`);
  await refresh(true);
}

function wire() {
  $("tree-select").onchange = async (e) => afterTreeChange(await call("switch_tree", { name: e.target.value }));
  $("btn-tree-new").onclick = async () => {
    const name = await promptBox("Yeni soy ağacı", "Ağacın adı (ör. Eşimin ağacı). Yeni ağaç boş açılır; diğer ağaçlarla karışmaz.", "", "Oluştur");
    if (!name) return;
    await afterTreeChange(await call("create_tree", { name }));
    toast(`"${name}" açıldı. Şimdi PDF içe aktarabilirsiniz.`);
  };
  $("btn-tree-more").onclick = treeMenu;
  // Ağaçta tek tıklama bilgileri gösterir, çift tıklama ağacı o kişiden başlatır
  Tree.init((id) => selectInTree(id), (id) => {
    if (state.tab === "relation") {
      state.selected = id;
      state.root = id;
      setTab("ancestors");
    } else select(id);
  });
  $("btn-import").onclick = async () => {
    if (await importFlow()) await refresh();
  };
  $("btn-add").onclick = async () => {
    const input = await personForm("Yeni kişi");
    if (!input) return;
    state.selected = await call("create_person", { input, relation: null });
    await refresh();
  };
  $("search").oninput = (e) => {
    state.search = e.target.value;
    renderList();
  };
  for (const b of document.querySelectorAll("[data-tab]")) b.onclick = () => setTab(b.dataset.tab);
  for (const b of document.querySelectorAll("[data-view]")) {
    b.onclick = () => {
      state.view = b.dataset.view;
      settings.view = state.view;
      saveSettings();
      for (const x of document.querySelectorAll("[data-view]")) x.classList.toggle("active", x === b);
      render();
    };
  }
  // Açılış görünümü: son seçilen (varsayılan Ata + Soy, Yatay)
  if (["ancestors", "descendants", "both"].includes(settings.tab)) state.tab = settings.tab;
  if (["Vertical", "Horizontal", "Radial", "Text"].includes(settings.view)) state.view = settings.view;
  for (const b of document.querySelectorAll("[data-tab]")) b.classList.toggle("active", b.dataset.tab === state.tab);
  for (const b of document.querySelectorAll("[data-view]")) b.classList.toggle("active", b.dataset.view === state.view);
  // Kuşak sayısı: 2-15, son seçim hatırlanır
  state.gens = Math.min(15, Math.max(2, Number(settings.gens) || 8));
  for (let g = 2; g <= 15; g++) $("gens").append(h("option", { value: String(g), selected: g === state.gens }, String(g)));
  $("gens").onchange = (e) => {
    state.gens = Number(e.target.value);
    settings.gens = state.gens;
    saveSettings();
    render();
  };
  $("btn-fit").onclick = () => Tree.fit();
  $("btn-export-view").onclick = exportFlow;
  // Üst çubuk tema seçicisi
  const sw = $("theme-switch");
  for (const [id, label, bg, ac] of THEMES) {
    sw.append(h("button", {
      class: "theme-swatch" + (id === "system" ? " system" : ""),
      "data-theme": id,
      title: label,
      style: bg ? `--sw-bg:${bg};--sw-ac:${ac}` : "--sw-ac:transparent",
      onclick: () => {
        settings.theme = id;
        saveSettings();
        applyTheme();
        render(true);
      },
    }));
  }
  $("btn-theme-menu").onclick = (e) => {
    e.stopPropagation();
    sw.classList.toggle("open");
  };
  document.addEventListener("click", (e) => {
    if (!e.target.closest("#theme-group")) sw.classList.remove("open");
  });
  applyTheme();
  $("btn-settings").onclick = async () => {
    const sel = h("select", {},
      h("option", { value: "cross", selected: settings.death === "cross" }, "† işareti"),
      h("option", { value: "o", selected: settings.death === "o" }, "(ö.) yazısı"),
      h("option", { value: "none", selected: settings.death === "none" }, "Gösterme"));
    const col = h("select", {},
      h("option", { value: "twins", selected: settings.collapse !== "merge" }, "İkiz kart: her kolda yerinde, aynı renk (önerilen)"),
      h("option", { value: "merge", selected: settings.collapse === "merge" }, "Tek kart: ikinci koldan renkli birleşme oku"));
    const theme = h("select", {}, THEMES.map(([id, label]) => h("option", { value: id, selected: settings.theme === id }, label)));
    const accent = h("select", {}, Object.entries(ACCENTS).map(([k, a]) =>
      h("option", { value: k, selected: settings.accent === k }, a.label)));
    // Seçimler anında önizlenir; vazgeçilirse eski hale döner
    const before = { theme: settings.theme, accent: settings.accent };
    const preview = () => {
      settings.theme = theme.value;
      settings.accent = accent.value;
      applyTheme();
    };
    theme.onchange = preview;
    accent.onchange = preview;
    const body = h("div", { class: "settings" },
      h("label", { class: "note" }, "Tema", h("br"), theme),
      h("label", { class: "note" }, "Vurgu rengi", h("br"), accent),
      h("label", { class: "note" }, "Vefat etmiş kişilerin adının yanında", h("br"), sel),
      h("label", { class: "note" }, "Akraba evliliğindeki ortak atalar", h("br"), col));
    const v = await openDialog("Ayarlar", body, [
      { label: "Vazgeç", value: null },
      { label: "Kaydet", value: "ok", primary: true },
    ]);
    if (v !== "ok") {
      Object.assign(settings, before);
      applyTheme();
      return;
    }
    settings.theme = theme.value;
    settings.accent = accent.value;
    settings.death = sel.value;
    settings.collapse = col.value;
    saveSettings();
    renderList();
    await renderDetail();
    await render();
  };
  $("rel-a").onchange = (e) => {
    state.relA = e.target.value ? Number(e.target.value) : null;
    render();
  };
  $("rel-b").onchange = (e) => {
    state.relB = e.target.value ? Number(e.target.value) : null;
    render();
  };
  $("btn-export").onclick = async () => {
    if (await call("export_backup")) toast("Yedek kaydedildi");
  };
  $("btn-restore").onclick = async () => {
    if (!(await confirmBox("Yedekten yükle", `Yedek, açık ağacın ("${state.tree}") tüm kayıtlarının yerini alır; diğer ağaçlar etkilenmez. Başka bir ağaca yüklemek için önce Yeni ile boş bir ağaç açın. Devam edilsin mi?`, "Devam", true))) return;
    if (await call("import_backup")) {
      toast("Yedek yüklendi");
      state.selected = null;
      await refresh();
    }
  };
  window.addEventListener("resize", () => Tree.layout && Tree.fit());
}

wire();
call("list_trees").then((l) => {
  renderTrees(l);
  refresh();
});
