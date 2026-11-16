//! Önizlemeli PDF içe aktarma ve kişi eşleştirme.

use crate::db::{self, PersonInput};
use crate::relation::Step;
use crate::{tr_fold, Record, Result, Sex};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Candidate {
    pub person_id: i64,
    /// Arayüzde gösterilecek kısa tanım: "RIZA (01/07/1895, Çankırı/Ilgaz/…)"
    pub label: String,
    pub score: u8,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreviewNode {
    pub index: usize,
    /// `None`: PDF'te satırı olmayan yer tutucu
    pub record: Option<Record>,
    pub sex: Option<Sex>,
    /// PDF sahibine göre yakınlık ("Babasının Annesi"); yer tutucu için türetilmiş
    pub path_text: String,
    pub candidates: Vec<Candidate>,
    pub suggested: Option<i64>,
    /// Aynı PDF'te birebir aynı bilgilerle daha önce geçen satırın `index`'i
    pub duplicate_of: Option<usize>,
    /// Kullanıcıya gösterilecek uyarı (ör. ağaca bağlanamadı)
    pub warning: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImportPreview {
    pub file_name: String,
    pub nodes: Vec<PreviewNode>,
    /// (çocuk, ebeveyn, rol) — `nodes` indeksleri
    pub parent_edges: Vec<(usize, usize, Sex)>,
    pub spouse_edges: Vec<(usize, usize)>,
    /// PDF sahibinin (Kendisi) indeksi
    pub root: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportDecision {
    pub index: usize,
    pub merge_into: Option<i64>,
    pub split_duplicate: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportSummary {
    pub source_id: i64,
    pub created: usize,
    pub merged: usize,
    pub links_added: usize,
    pub conflicts: Vec<String>,
}

pub fn preview_import(conn: &Connection, file_name: &str, pdf_bytes: &[u8]) -> Result<ImportPreview> {
    let text = crate::pdf::extract(pdf_bytes)?;
    let records = crate::table::parse_records(&text)?;
    preview_records(conn, file_name, &records)
}

/// Ayrıştırılmış kayıtlardan önizleme (PDF okuma dışındaki her şey).
pub fn preview_records(conn: &Connection, file_name: &str, records: &[Record]) -> Result<ImportPreview> {
    let graph = crate::relation::build_graph(records)?;
    let persons = db::list_persons(conn)?;
    let mut nodes: Vec<PreviewNode> = graph
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| PreviewNode {
            index: i,
            record: n.record.clone(),
            sex: n.sex,
            path_text: match &n.record {
                Some(r) => r.relation.clone(),
                None => path_text(&n.path),
            },
            candidates: Vec::new(),
            suggested: None,
            duplicate_of: None,
            warning: graph.unlinked.contains(&i).then(|| {
                "Hangi kişiye bağlı olduğu belirlenemedi; içe aktarıldıktan sonra elle bağlayın".to_string()
            }),
        })
        .collect();
    // Aynı PDF içindeki kopyalar (akraba evliliğinde aynı ata iki yoldan gelir)
    for j in 0..nodes.len() {
        let Some(rj) = &nodes[j].record else { continue };
        let kj = identity_key(rj);
        if kj.is_none() {
            continue;
        }
        if let Some(i) = (0..j).find(|&i| {
            nodes[i].duplicate_of.is_none() && nodes[i].record.as_ref().map(identity_key) == Some(kj.clone())
        }) {
            nodes[j].duplicate_of = Some(i);
        }
    }
    // Kayıt bilgisine göre adaylar
    for n in nodes.iter_mut() {
        let Some(r) = &n.record else { continue };
        let input = PersonInput::from(r);
        n.candidates = persons
            .iter()
            .filter_map(|p| score(&input, &p.info, &p.manual_fields).map(|(s, reasons)| (p, s, reasons)))
            .filter(|(_, s, _)| *s >= 40)
            .map(|(p, s, reasons)| Candidate { person_id: p.id, label: label(&p.info), score: s, reasons })
            .collect();
    }
    // Bağlam: eşleşen çocuğun aynı roldeki ebeveyni, bu düğümün ebeveyn adayıdır
    for _ in 0..8 {
        for n in nodes.iter_mut() {
            n.candidates.sort_by(|a, b| b.score.cmp(&a.score).then(a.person_id.cmp(&b.person_id)));
            n.suggested = match n.candidates.as_slice() {
                [first, rest @ ..] if first.score >= SUGGEST_AT && rest.first().is_none_or(|s| s.score < first.score) => {
                    Some(first.person_id)
                }
                _ => None,
            };
        }
        let mut changed = false;
        for (child, parent, role) in &graph.parent_edges {
            let Some(matched_child) = nodes[*child].suggested else { continue };
            let (f, m) = db::parents(conn, matched_child)?;
            let Some(existing) = (if *role == Sex::Male { f } else { m }) else { continue };
            let node = &mut nodes[*parent];
            let reason = "Çocuğu eşleşen kişinin aynı ebeveyni".to_string();
            match node.candidates.iter_mut().find(|c| c.person_id == existing) {
                Some(c) if !c.reasons.contains(&reason) => {
                    c.score = (c.score + 20).min(100);
                    c.reasons.push(reason);
                    changed = true;
                }
                Some(_) => {}
                None => {
                    let p = db::get_person(conn, existing)?;
                    // Yer tutucunun bilgisi yok: bağlam tek başına yeterli
                    let base = if node.record.is_none() { 90 } else { score(&PersonInput::from(node.record.as_ref().expect("kayıt")), &p.info, &p.manual_fields).map(|s| s.0).unwrap_or(0) };
                    if node.record.is_none() || base >= 40 {
                        node.candidates.push(Candidate {
                            person_id: existing,
                            label: label(&p.info),
                            score: if node.record.is_none() { base } else { (base + 20).min(100) },
                            reasons: vec![reason],
                        });
                        changed = true;
                    }
                }
            }
        }
        // Ters yön: ebeveyni eşleşen düğüm, o kişinin mevcut çocuklarından biridir.
        // Önceki bir PDF'teki boşluktan doğan yer tutucu böylece gerçek kişiyle birleşir.
        for (child, parent, _) in &graph.parent_edges {
            let Some(matched_parent) = nodes[*parent].suggested else { continue };
            let Some(rec) = nodes[*child].record.clone() else { continue };
            let input = PersonInput::from(&rec);
            for y in db::children(conn, matched_parent)? {
                let p = db::get_person(conn, y)?;
                if let (Some(a), Some(b)) = (input.sex, p.info.sex) {
                    if a != b {
                        continue;
                    }
                }
                let (score_new, reason) = if p.placeholder {
                    if !p.info.given_name.is_empty() && tr_fold(&p.info.given_name) != tr_fold(&input.given_name) {
                        continue;
                    }
                    (90, "Ebeveyni eşleşen kişinin yer tutucu çocuğu")
                } else {
                    match score(&input, &p.info, &p.manual_fields) {
                        Some((b, _)) if b >= 40 => ((b + 20).min(100), "Ebeveyni eşleşen kişinin çocuğu"),
                        _ => continue,
                    }
                };
                let node = &mut nodes[*child];
                let reason = reason.to_string();
                match node.candidates.iter_mut().find(|c| c.person_id == y) {
                    Some(c) if !c.reasons.contains(&reason) => {
                        c.score = c.score.max(score_new);
                        c.reasons.push(reason);
                        changed = true;
                    }
                    Some(_) => {}
                    None => {
                        node.candidates.push(Candidate { person_id: y, label: label(&p.info), score: score_new, reasons: vec![reason] });
                        changed = true;
                    }
                }
            }
        }
        if !changed {
            break;
        }
    }
    let root = graph.nodes.iter().position(|n| n.path.is_empty()).expect("kök");
    Ok(ImportPreview {
        file_name: file_name.to_string(),
        nodes,
        parent_edges: graph.parent_edges.clone(),
        spouse_edges: graph.spouse_edges.clone(),
        root,
    })
}

const SUGGEST_AT: u8 = 85;

/// Yakınlık yolunu Türkçe ifadeye çevirir (yer tutucular için).
fn path_text(path: &[Step]) -> String {
    if path.is_empty() {
        return "Kendisi".into();
    }
    let words: Vec<&str> = path
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let last = i + 1 == path.len();
            match (s, last) {
                (Step::Father, true) => "Babası",
                (Step::Father, false) => "Babasının",
                (Step::Mother, true) => "Annesi",
                (Step::Mother, false) => "Annesinin",
                (Step::Son, true) => "Oğlu",
                (Step::Son, false) => "Oğlunun",
                (Step::Daughter, true) => "Kızı",
                (Step::Daughter, false) => "Kızının",
                (Step::Spouse, true) => "Eşi",
                (Step::Spouse, false) => "Eşinin",
                (Step::Sibling, true) => "Kardeşi",
                (Step::Sibling, false) => "Kardeşinin",
            }
        })
        .collect();
    words.join(" ")
}

/// Aynı PDF içinde iki satırın aynı kişi olduğunu gösteren anahtar.
fn identity_key(r: &Record) -> Option<Vec<String>> {
    if r.given_name.is_empty() || (r.register_no.is_empty() && r.birth_date.is_empty()) {
        return None;
    }
    Some(
        [&r.given_name, &r.surname, &r.father_name, &r.mother_name, &r.birth_place, &r.birth_date, &r.register_no, &r.province_district]
            .iter()
            .map(|s| tr_fold(s))
            .collect(),
    )
}

fn label(p: &PersonInput) -> String {
    let name = format!("{} {}", p.given_name, p.surname).trim().to_string();
    let extra: Vec<&str> =
        [p.birth_date.as_str(), p.province_district.as_str()].into_iter().filter(|s| !s.is_empty()).collect();
    if extra.is_empty() {
        name
    } else {
        format!("{name} ({})", extra.join(", "))
    }
}

/// Yeni kayıt `a` ile mevcut kişi `b`'nin aynı olma puanı (0-100) ve gerekçeler; cinsiyet
/// çelişirse `None`. `ignore`: `b`'de elle düzeltilmiş alanlar; bunlar karşılaştırılmaz ve
/// puan kalan alanlara göre ölçeklenir (elle düzeltilen ad ya da tarih kopya üretmesin).
fn score(a: &PersonInput, b: &PersonInput, ignore: &[String]) -> Option<(u8, Vec<String>)> {
    if let (Some(x), Some(y)) = (a.sex, b.sex) {
        if x != y {
            return None;
        }
    }
    if let (Some(x), Some(y)) = (&a.tc, &b.tc) {
        if !x.is_empty() && x == y {
            return Some((100, vec!["T.C. kimlik no aynı".into()]));
        }
    }
    let skip = |f: &str| ignore.iter().any(|i| i == f);
    let eq = |x: &str, y: &str| !x.is_empty() && tr_fold(x) == tr_fold(y);
    let mut s: u32 = 0;
    let mut max: u32 = 0;
    let mut reasons = Vec::new();
    for (field, x, y, pts, why) in [
        ("given_name", &a.given_name, &b.given_name, 25, "Adı"),
        ("surname", &a.surname, &b.surname, 10, "Soyadı"),
        ("father_name", &a.father_name, &b.father_name, 15, "Baba adı"),
        ("mother_name", &a.mother_name, &b.mother_name, 15, "Ana adı"),
        ("birth_date", &a.birth_date, &b.birth_date, 20, "Doğum tarihi"),
        ("birth_place", &a.birth_place, &b.birth_place, 5, "Doğum yeri"),
    ] {
        // Eski kayıtlarda soyadı yoktur: iki tarafta da boşsa karşılaştırma dışı
        if skip(field) || (field == "surname" && x.is_empty() && y.is_empty()) {
            continue;
        }
        max += pts;
        if eq(x, y) {
            s += pts;
            reasons.push(format!("{why} aynı"));
        }
    }
    if !skip("register_no") && !skip("province_district") {
        max += 10;
        if eq(&a.register_no, &b.register_no) && eq(&a.province_district, &b.province_district) {
            s += 10;
            reasons.push("Nüfus kaydı aynı".into());
        }
    }
    // Adı tutmayan kişi aynı kişi sayılmaz (ad elle düzeltilmediyse)
    if !skip("given_name") && !eq(&a.given_name, &b.given_name) {
        return None;
    }
    // Karşılaştırılacak yeterli alan kalmadıysa karar verme
    if max < 50 {
        return None;
    }
    Some(((s * 100 / max).min(100) as u8, reasons))
}

pub fn commit_import(conn: &Connection, preview: &ImportPreview, decisions: &[ImportDecision]) -> Result<ImportSummary> {
    let tx = crate::db::Atomic::begin(conn)?;
    let decision = |i: usize| decisions.iter().find(|d| d.index == i).copied();
    let mut summary = ImportSummary::default();
    let mut ids: Vec<i64> = Vec::with_capacity(preview.nodes.len());
    for n in &preview.nodes {
        let d = decision(n.index).unwrap_or(ImportDecision { index: n.index, merge_into: None, split_duplicate: false });
        if let (Some(orig), false) = (n.duplicate_of, d.split_duplicate) {
            ids.push(ids[orig]);
            continue;
        }
        let input = match &n.record {
            Some(r) => PersonInput::from(r),
            None => placeholder_input(preview, n.index),
        };
        let id = match d.merge_into {
            Some(pid) => {
                if n.record.is_some() {
                    db::update_person_from_pdf(&tx, pid, &input)?;
                }
                summary.merged += 1;
                pid
            }
            None => {
                summary.created += 1;
                if n.record.is_some() {
                    db::insert_person(&tx, &input, false)?
                } else {
                    db::insert_placeholder(&tx, &input)?
                }
            }
        };
        ids.push(id);
    }
    tx.execute(
        "INSERT INTO source (file_name, root_person_id) VALUES (?1, ?2)",
        rusqlite::params![preview.file_name, ids[preview.root]],
    )?;
    summary.source_id = tx.last_insert_rowid();
    for n in &preview.nodes {
        if let Some(r) = &n.record {
            tx.execute(
                "INSERT OR IGNORE INTO person_source (person_id, source_id, row_no, relation_text) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![ids[n.index], summary.source_id, r.row_no, r.relation],
            )?;
        }
    }
    let name = |i: usize| -> String {
        let n = &preview.nodes[i];
        match &n.record {
            Some(r) => format!("{} {} ({})", r.given_name, r.surname, n.path_text),
            None => n.path_text.clone(),
        }
    };
    for (child, parent, role) in &preview.parent_edges {
        let (c, p) = (ids[*child], ids[*parent]);
        let (f, m) = db::parents(&tx, c)?;
        if (if *role == Sex::Male { f } else { m }) == Some(p) {
            continue;
        }
        match db::set_parent(&tx, c, p, *role, "pdf") {
            Ok(()) => summary.links_added += 1,
            Err(crate::Error::Invalid(e)) => summary.conflicts.push(format!("{} → {}: {e}", name(*child), name(*parent))),
            Err(e) => return Err(e),
        }
    }
    for (a, b) in &preview.spouse_edges {
        let (x, y) = (ids[*a], ids[*b]);
        if x == y || db::spouses(&tx, x)?.contains(&y) {
            continue;
        }
        db::set_spouse(&tx, x, y)?;
        summary.links_added += 1;
    }
    tx.commit()?;
    Ok(summary)
}

/// Yer tutucuya bilinen tek bilgiyi ver: çocuğunun kaydındaki baba/ana adı.
fn placeholder_input(preview: &ImportPreview, index: usize) -> PersonInput {
    let n = &preview.nodes[index];
    let mut given = String::new();
    for (child, parent, role) in &preview.parent_edges {
        if *parent == index {
            if let Some(r) = &preview.nodes[*child].record {
                given = if *role == Sex::Male { r.father_name.clone() } else { r.mother_name.clone() };
                break;
            }
        }
    }
    PersonInput { given_name: given, sex: n.sex, ..Default::default() }
}

/// Veritabanında aynı kişi olabilecek çiftler (a < b, puan), puana göre azalan.
pub fn duplicate_suggestions(conn: &Connection) -> Result<Vec<(i64, i64, u8)>> {
    let persons: Vec<_> = db::list_persons(conn)?.into_iter().filter(|p| !p.placeholder).collect();
    let mut not_same = std::collections::HashSet::new();
    {
        let mut st = conn.prepare("SELECT a_id, b_id FROM not_same")?;
        for r in st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))? {
            not_same.insert(r?);
        }
    }
    let mut out = Vec::new();
    for (i, a) in persons.iter().enumerate() {
        for b in &persons[i + 1..] {
            let pair = (a.id.min(b.id), a.id.max(b.id));
            if not_same.contains(&pair) {
                continue;
            }
            if let Some((s, _)) = score(&a.info, &b.info, &[]) {
                if s >= SUGGEST_AT {
                    out.push((pair.0, pair.1, s));
                }
            }
        }
    }
    out.sort_by(|x, y| y.2.cmp(&x.2).then(x.0.cmp(&y.0)).then(x.1.cmp(&y.1)));
    Ok(out)
}

/// İki kişinin farklı olduğunu kaydeder; bir daha eşleşme önerilmez.
pub fn mark_not_same(conn: &Connection, a: i64, b: i64) -> Result<()> {
    if a == b {
        return Err(crate::Error::Invalid("Aynı kişi seçildi".into()));
    }
    conn.execute(
        "INSERT OR IGNORE INTO not_same (a_id, b_id) VALUES (?1, ?2)",
        rusqlite::params![a.min(b), a.max(b)],
    )?;
    Ok(())
}
