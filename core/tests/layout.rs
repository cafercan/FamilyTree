use familytree_core::db::{self, list_persons};
use familytree_core::import::{commit_import, preview_import, ImportDecision};
use familytree_core::kinship::find_relationship;
use familytree_core::layout::*;
use rusqlite::Connection;

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!("{}/../tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

fn db_with(files: &[&str]) -> Connection {
    let c = Connection::open_in_memory().unwrap();
    db::migrate(&c).unwrap();
    for f in files {
        let p = preview_import(&c, f, &fixture(f)).unwrap();
        let d: Vec<_> = p
            .nodes
            .iter()
            .map(|n| ImportDecision { index: n.index, merge_into: n.suggested, split_duplicate: false })
            .collect();
        commit_import(&c, &p, &d).unwrap();
    }
    c
}

fn find(c: &Connection, given: &str, surname: &str) -> i64 {
    list_persons(c).unwrap().into_iter().find(|p| p.info.given_name == given && p.info.surname == surname).unwrap().id
}

fn node<'a>(l: &'a TreeLayout, id: i64) -> Vec<&'a LayoutNode> {
    l.nodes.iter().filter(|n| n.person_id == id).collect()
}

fn assert_no_overlap(l: &TreeLayout) {
    for a in &l.nodes {
        for b in &l.nodes {
            if a.key != b.key && (a.y - b.y).abs() < 0.01 {
                assert!((a.x - b.x).abs() >= NODE_W + H_GAP - 0.01, "{} / {}", a.label, b.label);
            }
        }
    }
}

#[test]
fn no_overlap_in_ancestors() {
    let c = db_with(&["ornek-ali.pdf"]);
    let l = ancestors_layout(&c, find(&c, "EMRE", "YILMAZ"), 4, Projection::Vertical, false).unwrap();
    assert_eq!(l.nodes.len(), 12, "Emre + 2 + 4 + 7, ortak atalar (Rıza, Hatice) birer kez");
    for a in &l.nodes {
        for b in &l.nodes {
            if a.key != b.key && (a.y - b.y).abs() < 0.01 {
                assert!((a.x - b.x).abs() >= NODE_W + H_GAP - 0.01, "{} / {}", a.label, b.label);
            }
        }
        assert!(a.x >= 0.0 && a.y >= 0.0 && a.x <= l.width && a.y <= l.height);
    }
    // Kök altta, atalar yukarıda; babalar solda
    let ali = node(&l, find(&c, "EMRE", "YILMAZ"))[0];
    let mehmet = node(&l, find(&c, "MEHMET", "YILMAZ"))[0];
    let fatma = node(&l, find(&c, "NURTEN", "YILMAZ"))[0];
    assert!(mehmet.y < ali.y && mehmet.x < fatma.x);
    assert_eq!(l.edges.len(), 13, "her ebeveyn bağı bir çizgi");
}

/// Akraba evliliğindeki ortak ata tek düğümdür; iki koldan gelen çizgiler ona bağlanır.
#[test]
fn shared_ancestor_drawn_once() {
    let c = db_with(&["ornek-ali.pdf"]);
    let l = ancestors_layout(&c, find(&c, "EMRE", "YILMAZ"), 4, Projection::Vertical, false).unwrap();
    let riza = node(&l, find(&c, "RIZA", ""));
    assert_eq!(riza.len(), 1);
    assert!(riza[0].shared);
    assert_eq!(riza[0].years, "1895–1961");
    let incoming: Vec<&LayoutEdge> = l.edges.iter().filter(|e| e.to == riza[0].key).collect();
    assert_eq!(incoming.len(), 2, "Kadir ve Elif'ten");
    // Biri ağaç çizgisi, biri ikinci koldan gelen birleşme çizgisi; ikisi de atanın renk grubunda
    assert_eq!(incoming.iter().filter(|e| e.merge).count(), 1);
    assert!(incoming.iter().all(|e| e.group == riza[0].shared_group));
    let hatice = node(&l, find(&c, "HATİCE", ""));
    assert!(riza[0].shared_group.is_some() && hatice[0].shared_group.is_some());
    assert_ne!(riza[0].shared_group, hatice[0].shared_group, "her ortak ataya ayrı renk");
    assert!(!node(&l, find(&c, "OSMAN", "KAYA"))[0].shared);
    assert_eq!(node(&l, find(&c, "OSMAN", "KAYA"))[0].shared_group, None);
    assert!(l.edges.iter().filter(|e| !e.merge && e.to != riza[0].key && e.to != hatice[0].key).all(|e| e.group.is_none()));
    assert_no_overlap(&l);
}

/// Ortak ata iki kolda farklı kuşak uzaklığındaysa daha uzak kuşağa yerleşir; tüm
/// çizgiler yukarı doğru gider.
#[test]
fn shared_ancestor_at_different_depths() {
    use familytree_core::db::{insert_person, set_parent, PersonInput};
    use familytree_core::Sex;
    let c = Connection::open_in_memory().unwrap();
    db::migrate(&c).unwrap();
    let p = |n: &str, s: Sex| insert_person(&c, &PersonInput { given_name: n.into(), sex: Some(s), ..Default::default() }, true).unwrap();
    let (ata, a, b, b2, f, m, kok) = (p("ATA", Sex::Male), p("A", Sex::Male), p("B", Sex::Female), p("B2", Sex::Female), p("F", Sex::Male), p("M", Sex::Female), p("KÖK", Sex::Male));
    // Baba tarafı: KÖK → F → A → ATA (3 kuşak); anne tarafı: KÖK → M → B → B2 → ATA (4 kuşak)
    set_parent(&c, kok, f, Sex::Male, "manual").unwrap();
    set_parent(&c, kok, m, Sex::Female, "manual").unwrap();
    set_parent(&c, f, a, Sex::Male, "manual").unwrap();
    set_parent(&c, a, ata, Sex::Male, "manual").unwrap();
    set_parent(&c, m, b, Sex::Female, "manual").unwrap();
    set_parent(&c, b, b2, Sex::Female, "manual").unwrap();
    set_parent(&c, b2, ata, Sex::Male, "manual").unwrap();
    let l = ancestors_layout(&c, kok, 6, Projection::Vertical, false).unwrap();
    let ata_n = node(&l, ata);
    assert_eq!(ata_n.len(), 1);
    assert_eq!(ata_n[0].depth, 4);
    let by_key = |k: &str| l.nodes.iter().find(|n| n.key == k).unwrap();
    for e in &l.edges {
        assert!(by_key(&e.to).y < by_key(&e.from).y, "ata her zaman yukarıda");
    }
    assert_eq!(l.edges.len(), 7);
}

/// Soy görünümünde akraba evliliğinden doğan çocuk iki ebeveyninin altında tek düğümdür.
#[test]
fn shared_descendant_drawn_once() {
    let c = db_with(&["ornek-ali.pdf"]);
    let l = descendants_layout(&c, find(&c, "RIZA", ""), 5, Projection::Vertical, false).unwrap();
    let emre = node(&l, find(&c, "EMRE", "YILMAZ"));
    assert_eq!(emre.len(), 1);
    assert_eq!(l.edges.iter().filter(|e| e.to == emre[0].key).count(), 2, "Mehmet ve Nurten'den");
    assert_no_overlap(&l);
}

/// Kum saati: atalar yukarıda, torunlar aşağıda, kök ortada tek düğüm.
#[test]
fn hourglass_ancestors_above_descendants_below() {
    let c = db_with(&["ornek-ali.pdf"]);
    let kadir = find(&c, "KADİR", "YILMAZ");
    let l = hourglass_layout(&c, kadir, 3, Projection::Vertical, false).unwrap();
    let k = node(&l, kadir);
    assert_eq!(k.len(), 1);
    let y = |id: i64| node(&l, id)[0].y;
    assert!(y(find(&c, "RIZA", "")) < k[0].y);
    assert!(y(find(&c, "MEHMET", "YILMAZ")) > k[0].y);
    assert!(y(find(&c, "EMRE", "YILMAZ")) > y(find(&c, "MEHMET", "YILMAZ")));
    assert_no_overlap(&l);
    for proj in [Projection::Horizontal, Projection::Radial] {
        let h = hourglass_layout(&c, kadir, 3, proj, false).unwrap();
        assert_eq!(h.nodes.len(), l.nodes.len());
    }
}

#[test]
fn parent_centered_over_children() {
    let c = db_with(&["ornek-ali.pdf"]);
    let l = descendants_layout(&c, find(&c, "KADİR", "YILMAZ"), 5, Projection::Vertical, false).unwrap();
    let hasan = node(&l, find(&c, "KADİR", "YILMAZ"))[0];
    let mehmet = node(&l, find(&c, "MEHMET", "YILMAZ"))[0];
    let ali = node(&l, find(&c, "EMRE", "YILMAZ"))[0];
    assert!((hasan.x - mehmet.x).abs() < 0.01 && (mehmet.x - ali.x).abs() < 0.01);
    assert!(hasan.y < mehmet.y);
    // Mehmet'in eşi Nurten yan bilgi olarak gelir
    assert_eq!(mehmet.spouse_ids, vec![find(&c, "NURTEN", "YILMAZ")]);
}

#[test]
fn relationship_layout_highlights_paths() {
    let c = db_with(&["ornek-ali.pdf", "ornek-ayse-kuzen.pdf"]);
    let rel = find_relationship(&c, find(&c, "EMRE", "YILMAZ"), find(&c, "AYŞE", "DEMİR")).unwrap().unwrap();
    let l = relationship_layout(&c, &rel, Projection::Vertical).unwrap();
    // Osman ve Elif tek tepe düğümü (eşler), altında Nurten→Emre ve Gül→Ayşe
    assert_eq!(l.nodes.len(), 5);
    assert!(l.nodes.iter().all(|n| n.highlight) && l.edges.iter().all(|e| e.highlight));
    let top = l.nodes.iter().min_by(|a, b| a.y.total_cmp(&b.y)).unwrap();
    assert_eq!(top.spouse_ids.len(), 1);
}

#[test]
fn horizontal_projection_root_left() {
    let c = db_with(&["ornek-ali.pdf"]);
    let ali_id = find(&c, "EMRE", "YILMAZ");
    let l = ancestors_layout(&c, ali_id, 4, Projection::Horizontal, false).unwrap();
    let ali = node(&l, ali_id)[0];
    assert!(l.nodes.iter().all(|n| n.x >= ali.x));
    for a in &l.nodes {
        for b in &l.nodes {
            if a.key != b.key && (a.x - b.x).abs() < 0.01 {
                assert!((a.y - b.y).abs() >= ROW_H - 0.01);
            }
        }
    }
}

#[test]
fn radial_root_at_center() {
    let c = db_with(&["ornek-ali.pdf"]);
    let ali_id = find(&c, "EMRE", "YILMAZ");
    let l = ancestors_layout(&c, ali_id, 4, Projection::Radial, false).unwrap();
    let ali = node(&l, ali_id)[0];
    for n in &l.nodes {
        let r = ((n.x - ali.x).powi(2) + (n.y - ali.y).powi(2)).sqrt();
        assert!((r - n.depth as f32 * RING).abs() < 0.5, "{} r={r}", n.label);
    }
}

#[test]
fn outline_text() {
    let c = db_with(&["ornek-ali.pdf"]);
    let t = outline(&c, find(&c, "EMRE", "YILMAZ"), 3, true).unwrap();
    let lines: Vec<&str> = t.lines().collect();
    assert!(lines[0].starts_with("* EMRE YILMAZ"), "{t}");
    assert!(lines.iter().any(|l| l.starts_with("** Baba: MEHMET YILMAZ")), "{t}");
    assert!(lines.iter().any(|l| l.starts_with("*** Baba: KADİR YILMAZ")), "{t}");
    assert!(lines.iter().any(|l| l.starts_with("** Anne: NURTEN YILMAZ")), "{t}");
    assert_eq!(lines.len(), 7, "3 kuşak: 1 + 2 + 4");
}

/// Yatay görünümde eş adı ayrı bir satıra çizilir: eşi olan görünümde satırlar daha aralıklı.
#[test]
fn horizontal_rows_leave_room_for_spouse_line() {
    let c = db_with(&["ornek-ali.pdf"]);
    let l = descendants_layout(&c, find(&c, "RIZA", ""), 4, Projection::Horizontal, false).unwrap();
    assert!(l.nodes.iter().any(|n| !n.spouse_labels.is_empty()));
    for a in &l.nodes {
        for b in &l.nodes {
            if a.key != b.key && (a.x - b.x).abs() < 0.01 {
                assert!((a.y - b.y).abs() >= ROW_H_SPOUSE - 0.01, "{} / {}", a.label, b.label);
            }
        }
    }
}

/// İkiz kart: ortak ata her iki kolda da yerinde; biri asıl kart, diğeri ikiz. Uzun
/// birleşme çizgisi yok, ikisi aynı renk grubunda.
#[test]
fn twin_cards_instead_of_merge_lines() {
    let c = db_with(&["ornek-ali.pdf"]);
    let l = ancestors_layout(&c, find(&c, "EMRE", "YILMAZ"), 4, Projection::Vertical, true).unwrap();
    assert_eq!(l.nodes.len(), 14, "ortak atalar iki kolda da yerinde");
    assert!(l.edges.iter().all(|e| !e.merge), "birleşme çizgisi yok");
    assert_eq!(l.edges.len(), 13);
    let riza = node(&l, find(&c, "RIZA", ""));
    assert_eq!(riza.len(), 2);
    let main: Vec<_> = riza.iter().filter(|n| n.twin_of.is_none()).collect();
    let twin: Vec<_> = riza.iter().filter(|n| n.twin_of.is_some()).collect();
    assert_eq!((main.len(), twin.len()), (1, 1));
    assert_eq!(twin[0].twin_of.as_deref(), Some(main[0].key.as_str()));
    assert!(riza.iter().all(|n| n.shared && n.shared_group == main[0].shared_group && n.shared_group.is_some()));
    let hatice = node(&l, find(&c, "HATİCE", ""));
    assert_ne!(hatice[0].shared_group, riza[0].shared_group);
    assert_eq!(node(&l, find(&c, "OSMAN", "KAYA"))[0].shared_group, None);
    assert_no_overlap(&l);
    // Kum saati ve soy görünümü de ikiz kartla çalışır
    let d = descendants_layout(&c, find(&c, "RIZA", ""), 5, Projection::Vertical, true).unwrap();
    assert_eq!(node(&d, find(&c, "EMRE", "YILMAZ")).len(), 2);
    assert_no_overlap(&d);
}
