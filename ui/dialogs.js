// Kişi formu, PDF içe aktarma önizlemesi ve düzen dökümü diyalogları.
"use strict";

const PERSON_FIELDS = [
  ["given_name", "Adı"],
  ["surname", "Soyadı"],
  ["sex", "Cinsiyet"],
  ["alive", "Hayatta mı?"],
  ["father_name", "Baba adı"],
  ["mother_name", "Ana adı"],
  ["birth_place", "Doğum yeri"],
  ["birth_date", "Doğum tarihi (gg/aa/yyyy)"],
  ["death_date", "Ölüm tarihi"],
  ["marital_status", "Medeni hali"],
  ["province_district", "İl/İlçe/Mahalle-Köy", "wide"],
  ["register_no", "Cilt-Hane-Birey Sıra No"],
  ["tc", "T.C. kimlik no (isteğe bağlı)"],
  ["notes", "Notlar", "wide"],
];

const EMPTY_PERSON = {
  given_name: "", surname: "", sex: null, father_name: "", mother_name: "", birth_place: "",
  birth_date: "", death_date: "", alive: null, province_district: "", neighborhood: "",
  register_no: "", marital_status: "", tc: null, notes: "",
};

/** Kişi formu; kaydedilirse PersonInput, vazgeçilirse null. `preset`: öntanımlı değerler. */
async function personForm(title, initial, preset) {
  const p = Object.assign({}, EMPTY_PERSON, initial || {}, preset || {});
  const inputs = {};
  const form = h("form", { class: "grid", onsubmit: (e) => e.preventDefault() });
  for (const [key, label, wide] of PERSON_FIELDS) {
    let input;
    if (key === "sex") {
      input = h("select", {},
        h("option", { value: "" }, "Bilinmiyor"),
        h("option", { value: "E", selected: p.sex === "E" }, "Erkek"),
        h("option", { value: "K", selected: p.sex === "K" }, "Kadın"));
    } else if (key === "alive") {
      input = h("select", {},
        h("option", { value: "" }, "Bilinmiyor"),
        h("option", { value: "1", selected: p.alive === true }, "Evet"),
        h("option", { value: "0", selected: p.alive === false }, "Hayır"));
    } else if (key === "notes") {
      input = h("textarea", { rows: 2 });
      input.value = p.notes || "";
    } else {
      input = h("input", { type: "text" });
      input.value = p[key] || "";
    }
    inputs[key] = input;
    form.append(h("label", { class: wide ? "wide" : "" }, label, input));
  }
  const read = () => {
    const out = Object.assign({}, EMPTY_PERSON, initial || {});
    for (const [key] of PERSON_FIELDS) {
      const v = inputs[key].value.trim();
      if (key === "sex") out.sex = v || null;
      else if (key === "alive") out.alive = v === "" ? null : v === "1";
      else if (key === "tc") out.tc = v || null;
      else out[key] = v;
    }
    return out;
  };
  setTimeout(() => inputs.given_name.focus(), 0);
  const v = await openDialog(title, form, [
    { label: "Vazgeç", value: null },
    { label: "Kaydet", value: "save", primary: true },
  ], () => {
    if (!inputs.given_name.value.trim()) {
      toast("Adı boş olamaz", true);
      return false;
    }
    return true;
  });
  return v === "save" ? read() : null;
}

/** PDF seç, önizle, kullanıcının kararlarıyla içe aktar. İçe aktarılırsa true. */
async function importFlow() {
  let preview;
  try {
    preview = await window.__TAURI__.core.invoke("pick_and_preview_pdf");
  } catch (e) {
    await parseErrorDialog(String(e));
    return false;
  }
  if (!preview) return false;
  const rows = [];
  const tbody = h("tbody");
  for (const n of preview.nodes) {
    const r = n.record;
    const sel = h("select");
    if (n.duplicate_of !== null && n.duplicate_of !== undefined) {
      const orig = preview.nodes[n.duplicate_of];
      sel.append(
        h("option", { value: "dup" }, `Bu PDF'teki "${orig.path_text}" ile aynı kişi`),
        h("option", { value: "split" }, "Ayrı kişi"),
      );
    } else {
      sel.append(h("option", { value: "" }, "Yeni kişi"));
      for (const c of n.candidates) {
        sel.append(h("option", { value: String(c.person_id), selected: c.person_id === n.suggested },
          `Mevcut: ${c.label} — %${c.score}`));
      }
    }
    rows.push({ node: n, sel });
    const why = n.candidates.length ? n.candidates[0].reasons.join(", ") : "";
    tbody.append(
      h("tr", { class: r ? "" : "placeholder" },
        h("td", {}, n.path_text),
        h("td", {}, r ? `${r.given_name} ${r.surname}`.trim() : "(PDF'te satırı yok)"),
        h("td", {}, r ? [r.birth_place, r.birth_date].filter(Boolean).join(" ") : ""),
        h("td", {}, sel, why ? h("div", { class: "note" }, why) : null,
          n.warning ? h("div", { class: "warn" }, n.warning) : null)),
    );
  }
  const suggestedCount = preview.nodes.filter((n) => n.suggested).length;
  const body = h("div", {},
    h("p", { class: "note" },
      `${preview.file_name}: ${preview.nodes.filter((n) => n.record).length} satır. `,
      suggestedCount
        ? `${suggestedCount} kişi veritabanındaki biriyle eşleşiyor ve önceden seçildi; lütfen kontrol edin.`
        : "Veritabanında eşleşen kişi bulunamadı; hepsi yeni kişi olarak eklenecek."),
    h("div", { class: "scroll" },
      h("table", {},
        h("thead", {}, h("tr", {}, h("th", {}, "Yakınlık"), h("th", {}, "Adı Soyadı"), h("th", {}, "Doğum"), h("th", {}, "Eşleşme"))),
        tbody)));
  const v = await openDialog("PDF içe aktarma önizlemesi", body, [
    { label: "Vazgeç", value: null },
    { label: "İçe aktar", value: "go", primary: true },
  ]);
  if (v !== "go") return false;
  const decisions = rows.map(({ node, sel }) => ({
    index: node.index,
    merge_into: sel.value && sel.value !== "dup" && sel.value !== "split" ? Number(sel.value) : null,
    split_duplicate: sel.value === "split",
  }));
  const sum = await call("commit_import", { preview, decisions });
  const parts = [`${sum.created} yeni kişi`, `${sum.merged} kişi birleştirildi`, `${sum.links_added} yeni bağ`];
  if (sum.parents_from_names) parts.push(`en üst kuşağa baba/ana adından ${sum.parents_from_names} kişi`);
  if (sum.conflicts.length) {
    await openDialog("İçe aktarıldı — bazı bağlar eklenemedi",
      h("div", {}, h("p", {}, parts.join(", ") + "."),
        h("p", { class: "warn" }, "Aşağıdaki ebeveyn bağları mevcut kayıtlarla çeliştiği için eklenmedi:"),
        h("ul", {}, sum.conflicts.map((c) => h("li", {}, c)))),
      [{ label: "Tamam", value: true, primary: true }]);
  } else {
    toast("İçe aktarıldı: " + parts.join(", "));
  }
  return true;
}

async function parseErrorDialog(message) {
  const v = await openDialog("PDF okunamadı",
    h("div", {},
      h("p", { class: "warn" }, message),
      h("p", {}, "Bu PDF'in düzeni beklenenden farklı olabilir. İsterseniz kişisel bilgi içermeyen bir ",
        h("b", {}, "düzen dökümü"), " oluşturabilirsiniz: her kelime yalnızca x, her sayı 0 olarak görünür (uzunlukları bile gizlenir); yalnızca sütun başlıkları okunur kalır. ",
        "Bu dökümü geliştiriciyle paylaşarak sorunun çözülmesine yardım edebilirsiniz.")),
    [{ label: "Kapat", value: null }, { label: "Düzen dökümünü göster", value: "dump", primary: true }]);
  if (v !== "dump") return;
  const dump = await call("layout_dump_last");
  if (!dump) return;
  const pre = h("pre", {}, dump);
  await openDialog("Düzen dökümü (kişisel bilgi içermez)", h("div", {}, pre), [
    { label: "Kapat", value: null },
    { label: "Kopyala", value: "copy", primary: true },
  ], async (val) => {
    if (val === "copy") {
      try {
        await navigator.clipboard.writeText(dump);
        toast("Kopyalandı");
      } catch {
        const r = document.createRange();
        r.selectNodeContents(pre);
        getSelection().removeAllRanges();
        getSelection().addRange(r);
        toast("Metin seçildi; Ctrl+C ile kopyalayın");
      }
      return false;
    }
    return true;
  });
}
