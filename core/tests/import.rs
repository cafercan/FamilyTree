use familytree_core::db::{self, get_person, list_persons, parents, update_person_manual};
use familytree_core::import::{
    commit_import, duplicate_suggestions, mark_not_same, preview_import, ImportDecision, ImportPreview,
    ImportSummary,
};
use rusqlite::Connection;

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!("{}/../tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

fn db() -> Connection {
    let c = Connection::open_in_memory().unwrap();
    db::migrate(&c).unwrap();
    c
}

/// Önizlemedeki önerileri aynen kabul eden kararlar (kullanıcının "İçe aktar"a basması).
fn accept_suggestions(p: &ImportPreview) -> Vec<ImportDecision> {
    p.nodes
        .iter()
        .map(|n| ImportDecision { index: n.index, merge_into: n.suggested, split_duplicate: false })
        .collect()
}

fn import(c: &Connection, name: &str) -> ImportSummary {
    let p = preview_import(c, name, &fixture(name)).unwrap();
    commit_import(c, &p, &accept_suggestions(&p)).unwrap()
}

fn find(c: &Connection, given: &str, surname: &str) -> i64 {
    let all: Vec<_> = list_persons(c)
        .unwrap()
        .into_iter()
        .filter(|p| p.info.given_name == given && p.info.surname == surname)
        .collect();
    assert_eq!(all.len(), 1, "{given} {surname} tek olmalı");
    all[0].id
}

#[test]
fn import_ali_creates_people_and_links() {
    let c = db();
    let p = preview_import(&c, "ornek-ali.pdf", &fixture("ornek-ali.pdf")).unwrap();
    // Ortak ata Rıza PDF'te iki satırda geçer: ikinci satır ilkinin kopyası olarak işaretli
    let ahmets: Vec<_> = p
        .nodes
        .iter()
        .filter(|n| n.record.as_ref().is_some_and(|r| r.given_name == "RIZA"))
        .collect();
    assert_eq!(ahmets.len(), 2);
    assert_eq!(ahmets[1].duplicate_of, Some(ahmets[0].index));
    assert!(p.nodes.iter().all(|n| n.suggested.is_none()), "boş veritabanında öneri olmaz");

    let decisions: Vec<_> = p
        .nodes
        .iter()
        .map(|n| ImportDecision { index: n.index, merge_into: None, split_duplicate: false })
        .collect();
    let s = commit_import(&c, &p, &decisions).unwrap();
    assert_eq!(s.created, 13, "15 satır, 2'si kopya");
    assert!(s.conflicts.is_empty(), "{:?}", s.conflicts);

    let ali = find(&c, "EMRE", "YILMAZ");
    let mehmet = find(&c, "MEHMET", "YILMAZ");
    let fatma = find(&c, "NURTEN", "YILMAZ");
    assert_eq!(parents(&c, ali).unwrap(), (Some(mehmet), Some(fatma)));
    let defne = find(&c, "DEFNE", "YILMAZ");
    let (df, dm) = parents(&c, defne).unwrap();
    assert_eq!(df, Some(ali));
    // Annesinin satırı yok; ana adından ("SELİN") yer tutucu eklendi
    let selin = get_person(&c, dm.unwrap()).unwrap();
    assert!(selin.placeholder && selin.info.given_name == "SELİN");
    let ahmet = find(&c, "RIZA", "");
    assert_eq!(db::children(&c, ahmet).unwrap().len(), 2, "Kadir ve Elif");
    // Soyadı "-" boş kaydedilir, ölüm bilgisi okunur
    let a = get_person(&c, ahmet).unwrap();
    assert_eq!((a.info.alive, a.info.death_date.as_str()), (Some(false), "14/02/1961"));
}

#[test]
fn kuzen_preview_suggests_shared_grandparents() {
    let c = db();
    import(&c, "ornek-ali.pdf");
    let p = preview_import(&c, "ornek-ayse-kuzen.pdf", &fixture("ornek-ayse-kuzen.pdf")).unwrap();
    let by = |g: &str, s: &str| {
        p.nodes.iter().find(|n| n.record.as_ref().is_some_and(|r| r.given_name == g && r.surname == s)).unwrap()
    };
    let by_name = |g: &str| p.nodes.iter().find(|n| n.record.as_ref().is_some_and(|r| r.given_name == g)).unwrap();
    assert_eq!(by_name("OSMAN").suggested, Some(find(&c, "OSMAN", "KAYA")));
    assert_eq!(by_name("ELİF").suggested, Some(find(&c, "ELİF", "KAYA")));
    assert_eq!(by_name("RIZA").suggested, Some(find(&c, "RIZA", "")));
    assert_eq!(by("AYŞE", "DEMİR").suggested, None, "Ayşe DEMİR yeni kişi");
    assert_eq!(by("AYŞE", "").suggested, Some(find(&c, "AYŞE", "")), "ortak ata Ayşe KAYA");
    assert_eq!(by_name("GÜL").suggested, None, "Gül yeni kişi");
    let osman = by_name("OSMAN");
    assert!(osman.candidates[0].score >= 85 && !osman.candidates[0].label.is_empty());

    let s = commit_import(&c, &p, &accept_suggestions(&p)).unwrap();
    assert_eq!(s.merged, 6);
    assert_eq!(s.created, 5);
    let osman_id = find(&c, "OSMAN", "KAYA");
    assert_eq!(db::children(&c, osman_id).unwrap().len(), 2, "Nurten ve Gül");
}

#[test]
fn reimport_same_pdf_creates_no_duplicates() {
    let c = db();
    import(&c, "ornek-ali.pdf");
    let before = list_persons(&c).unwrap().len();
    let s = import(&c, "ornek-ali.pdf");
    assert_eq!(list_persons(&c).unwrap().len(), before);
    assert_eq!((s.created, s.links_added), (0, 0));
}

#[test]
fn import_keeps_manual_fields() {
    let c = db();
    import(&c, "ornek-ali.pdf");
    let mehmet = find(&c, "MEHMET", "YILMAZ");
    let mut info = get_person(&c, mehmet).unwrap().info;
    info.birth_place = "UYDURMAKÖY".into();
    update_person_manual(&c, mehmet, &info).unwrap();
    import(&c, "ornek-ali.pdf");
    assert_eq!(get_person(&c, mehmet).unwrap().info.birth_place, "UYDURMAKÖY");
}

#[test]
fn split_duplicate_creates_two_people() {
    let c = db();
    let p = preview_import(&c, "ornek-ali.pdf", &fixture("ornek-ali.pdf")).unwrap();
    let decisions: Vec<_> = p
        .nodes
        .iter()
        .map(|n| ImportDecision { index: n.index, merge_into: None, split_duplicate: n.duplicate_of.is_some() })
        .collect();
    assert_eq!(commit_import(&c, &p, &decisions).unwrap().created, 15);
}

#[test]
fn not_same_suppresses_candidate() {
    let c = db();
    // Aynı PDF'i kopyaları ayırarak içe aktar: aynı bilgili iki RIZA olur
    let p = preview_import(&c, "ornek-ali.pdf", &fixture("ornek-ali.pdf")).unwrap();
    let decisions: Vec<_> = p
        .nodes
        .iter()
        .map(|n| ImportDecision { index: n.index, merge_into: None, split_duplicate: true })
        .collect();
    commit_import(&c, &p, &decisions).unwrap();
    let ahmets: Vec<i64> =
        list_persons(&c).unwrap().into_iter().filter(|p| p.info.given_name == "RIZA").map(|p| p.id).collect();
    let pair = (ahmets[0].min(ahmets[1]), ahmets[0].max(ahmets[1]));
    let sugg = duplicate_suggestions(&c).unwrap();
    assert!(sugg.iter().any(|(a, b, s)| (*a, *b) == pair && *s >= 85), "{sugg:?}");
    mark_not_same(&c, pair.0, pair.1).unwrap();
    assert!(!duplicate_suggestions(&c).unwrap().iter().any(|(a, b, _)| (*a, *b) == pair));
}

fn expected(name: &str) -> Vec<familytree_core::Record> {
    serde_json::from_slice(&fixture(&format!("{name}.expected.json"))).unwrap()
}

#[test]
fn two_children_with_same_relation_import() {
    let c = db();
    let mut recs = expected("ornek-ali");
    let mut second = recs.iter().find(|r| r.relation == "Kızı").unwrap().clone();
    second.given_name = "ECE".into();
    second.row_no = 16;
    recs.push(second);
    let p = familytree_core::import::preview_records(&c, "x.pdf", &recs).unwrap();
    let d = accept_suggestions(&p);
    commit_import(&c, &p, &d).unwrap();
    let emre = find(&c, "EMRE", "YILMAZ");
    assert_eq!(db::children(&c, emre).unwrap().len(), 2);
    assert!(p.nodes.iter().all(|n| n.warning.is_none()));
}

#[test]
fn preview_rejects_non_soyagaci_pdf() {
    let c = db();
    assert!(preview_import(&c, "x.pdf", b"%PDF-1.4 bozuk").is_err());
}

fn import_records(c: &Connection, recs: &[familytree_core::Record]) -> ImportSummary {
    let p = familytree_core::import::preview_records(c, "x.pdf", recs).unwrap();
    commit_import(c, &p, &accept_suggestions(&p)).unwrap()
}

/// Ayşe'nin PDF'inde "Annesinin Babası" yok → OSMAN yer tutucu olur. Sonra Emre'nin
/// PDF'i gerçek OSMAN satırıyla gelince yer tutucuyla birleşmeli (ebeveynleri eşleştiği için).
#[test]
fn gap_filled_by_later_pdf_merges_placeholder() {
    let c = db();
    let ayse: Vec<_> = expected("ornek-ayse-kuzen").into_iter().filter(|r| r.relation != "Annesinin Babası").collect();
    import_records(&c, &ayse);
    let ph: Vec<_> = list_persons(&c).unwrap().into_iter().filter(|p| p.placeholder && p.info.given_name == "OSMAN").collect();
    assert_eq!(ph.len(), 1, "boşluktaki OSMAN yer tutucu");
    let s = import_records(&c, &expected("ornek-ali"));
    assert!(s.conflicts.is_empty(), "{:?}", s.conflicts);
    let osmans: Vec<_> = list_persons(&c).unwrap().into_iter().filter(|p| p.info.given_name == "OSMAN").collect();
    assert_eq!(osmans.len(), 1, "tek OSMAN olmalı");
    assert!(!osmans[0].placeholder);
    let r = familytree_core::kinship::find_relationship(&c, find(&c, "NURTEN", "YILMAZ"), find(&c, "GÜL", "DEMİR")).unwrap().unwrap();
    assert_eq!(r.label, "Kız kardeşi");
}

/// Elle düzeltilen ad, aynı PDF yeniden içe aktarılınca yeni kişi oluşturmamalı.
#[test]
fn manual_name_edit_then_reimport_no_duplicate() {
    let c = db();
    import(&c, "ornek-ali.pdf");
    let before = list_persons(&c).unwrap().len();
    let id = find(&c, "HACI İBRAHİM", "");
    let mut info = get_person(&c, id).unwrap().info;
    info.given_name = "İBRAHİM".into();
    update_person_manual(&c, id, &info).unwrap();
    let s = import(&c, "ornek-ali.pdf");
    assert!(s.conflicts.is_empty(), "{:?}", s.conflicts);
    assert_eq!(list_persons(&c).unwrap().len(), before);
    assert_eq!(get_person(&c, id).unwrap().info.given_name, "İBRAHİM", "elle düzeltme korunur");
}

/// Çocuğu olmayan kök kişinin doğum tarihi elle düzeltilince de yeniden içe aktarma kopya üretmemeli.
#[test]
fn manual_edit_on_root_then_reimport_no_duplicate() {
    let c = db();
    import(&c, "ornek-ayse-kuzen.pdf");
    let before = list_persons(&c).unwrap().len();
    let id = find(&c, "AYŞE", "DEMİR");
    let mut info = get_person(&c, id).unwrap().info;
    info.birth_date = "24/12/1988".into();
    update_person_manual(&c, id, &info).unwrap();
    let s = import(&c, "ornek-ayse-kuzen.pdf");
    assert_eq!((s.created, list_persons(&c).unwrap().len()), (0, before));
}

/// Baba/ana adından eklenen yer tutucu, sonra gelen bir PDF'teki gerçek satırla birleşir.
#[test]
fn name_placeholder_merges_with_later_real_row() {
    let c = db();
    let s = import(&c, "ornek-ali.pdf");
    assert!(s.parents_from_names >= 10, "en üst kuşağın ebeveynleri adlardan eklenir: {}", s.parents_from_names);
    let riza = find(&c, "RIZA", "");
    let omer = db::parents(&c, riza).unwrap().0.unwrap();
    assert!(get_person(&c, omer).unwrap().placeholder);
    let before = list_persons(&c).unwrap().len();
    // Aynı PDF'e Rıza'nın babasının gerçek satırı eklenmiş hali
    let mut recs = expected("ornek-ali");
    let mut omer_row = recs[0].clone();
    omer_row.row_no = 16;
    omer_row.relation = "Babasının Babasının Babasının Babası".into();
    omer_row.given_name = "ÖMER".into();
    omer_row.father_name = "HÜSNÜ".into();
    omer_row.mother_name = "ZÜLEYHA".into();
    omer_row.birth_date = "01/07/1870".into();
    recs.push(omer_row);
    let s2 = import_records(&c, &recs);
    assert!(s2.conflicts.is_empty(), "{:?}", s2.conflicts);
    let omers: Vec<_> = list_persons(&c).unwrap().into_iter().filter(|p| p.info.given_name == "ÖMER").collect();
    assert_eq!(omers.len(), 1, "yer tutucu gerçek kişiyle birleşti");
    assert!(!omers[0].placeholder);
    assert_eq!(omers[0].info.birth_date, "01/07/1870");
    // Ömer'in anne-babası adlardan eklendi; başka yeni kişi yok
    assert_eq!(list_persons(&c).unwrap().len(), before + 2);
}
