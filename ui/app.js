// Uygulama durumu, kişi listesi, sekmeler ve kişi ayrıntısı.
"use strict";

const state = {
  persons: [],
  byId: new Map(),
  selected: null,
  tab: "ancestors",
  view: "Vertical",
  gens: 4,
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
  if (state.selected === null) return;
  if (text) {
    const up = () => call("outline", { id: state.selected, gens: state.gens, ancestors: true });
    const down = () => call("outline", { id: state.selected, gens: state.gens, ancestors: false });
    $("text-view").textContent = state.tab === "both"
      ? `ATALAR\n${await up()}\nTORUNLAR\n${await down()}`
      : await (state.tab === "ancestors" ? up() : down());
    return;
  }
  const layout = await call("tree_layout", {
    kind: state.tab, id: state.selected, gens: state.gens, projection: state.view, twins: settings.collapse !== "merge",
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
  for (const b of document.querySelectorAll("[data-tab]")) b.classList.toggle("active", b.dataset.tab === tab);
  render();
}

async function select(id) {
  state.selected = id;
  renderList();
  await renderDetail();
  if (state.tab !== "relation") await render();
  else {
    Tree.selectedId = id;
  }
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
  const facts = [
    p.sex === "E" ? "Erkek" : p.sex === "K" ? "Kadın" : null,
    p.birth_place || p.birth_date ? `Doğum: ${[p.birth_place, p.birth_date].filter(Boolean).join(" ")}` : null,
    p.alive === false ? `Vefat${p.death_date ? ": " + p.death_date : ""}` : p.alive === true ? "Hayatta" : null,
    p.province_district ? `Kayıt: ${p.province_district}${p.register_no ? " (" + p.register_no + ")" : ""}` : null,
    p.marital_status || null,
    p.placeholder ? "Yer tutucu: PDF'te satırı yok, bilgileri düzenleyebilirsiniz" : null,
  ].filter(Boolean);
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
  box.replaceChildren(
    h("h2", {}, fullName(p) + deathMark(p.alive)),
    h("div", { class: "facts" }, facts.join(" · ")),
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
      ? h("div", { class: "sources" }, "Kaynak: " + d.sources.map((s) => `${s.file_name} ("${s.relation_text}")`).join(", "))
      : null,
  );
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
  Tree.init((id) => select(id), (id) => {
    state.selected = id;
    if (state.tab === "relation") setTab("ancestors");
    else select(id);
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
      for (const x of document.querySelectorAll("[data-view]")) x.classList.toggle("active", x === b);
      render();
    };
  }
  $("gens").onchange = (e) => {
    state.gens = Number(e.target.value);
    render();
  };
  $("btn-fit").onclick = () => Tree.fit();
  $("btn-settings").onclick = async () => {
    const sel = h("select", {},
      h("option", { value: "cross", selected: settings.death === "cross" }, "† işareti"),
      h("option", { value: "o", selected: settings.death === "o" }, "(ö.) yazısı"),
      h("option", { value: "none", selected: settings.death === "none" }, "Gösterme"));
    const col = h("select", {},
      h("option", { value: "twins", selected: settings.collapse !== "merge" }, "İkiz kart: her kolda yerinde, aynı renk (önerilen)"),
      h("option", { value: "merge", selected: settings.collapse === "merge" }, "Tek kart: ikinci koldan renkli birleşme oku"));
    const body = h("div", { class: "settings" },
      h("label", { class: "note" }, "Vefat etmiş kişilerin adının yanında", h("br"), sel),
      h("label", { class: "note" }, "Akraba evliliğindeki ortak atalar", h("br"), col));
    const v = await openDialog("Ayarlar", body, [
      { label: "Vazgeç", value: null },
      { label: "Kaydet", value: "ok", primary: true },
    ]);
    if (v !== "ok") return;
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
