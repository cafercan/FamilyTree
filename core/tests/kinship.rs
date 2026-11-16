use familytree_core::db::{self, insert_person, list_persons, set_parent, PersonInput};
use familytree_core::import::{commit_import, preview_import, ImportDecision};
use familytree_core::kinship::{common_ancestors, consanguinity_report, find_relationship};
use familytree_core::Sex;
use rusqlite::Connection;

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!("{}/../tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

fn db() -> Connection {
    let c = Connection::open_in_memory().unwrap();
    db::migrate(&c).unwrap();
    c
}

fn import(c: &Connection, name: &str) {
    let p = preview_import(c, name, &fixture(name)).unwrap();
    let d: Vec<_> = p
        .nodes
        .iter()
        .map(|n| ImportDecision { index: n.index, merge_into: n.suggested, split_duplicate: false })
        .collect();
    commit_import(c, &p, &d).unwrap();
}

fn find(c: &Connection, given: &str, surname: &str) -> i64 {
    list_persons(c).unwrap().into_iter().find(|p| p.info.given_name == given && p.info.surname == surname).unwrap().id
}

fn sorted(mut v: Vec<i64>) -> Vec<i64> {
    v.sort();
    v
}

#[test]
fn ali_parents_are_first_cousins() {
    let c = db();
    import(&c, "ornek-ali.pdf");
    let report = consanguinity_report(&c).unwrap();
    assert_eq!(report.len(), 1, "{report:?}");
    let item = &report[0];
    assert_eq!((item.husband, item.wife), (find(&c, "MEHMET", "YILMAZ"), find(&c, "NURTEN", "YILMAZ")));
    assert_eq!(item.relationship.label, "Birinci dereceden kuzeni");
    let anc: Vec<i64> = item.relationship.closest.iter().map(|x| x.ancestor).collect();
    assert_eq!(sorted(anc), sorted(vec![find(&c, "RIZA", ""), find(&c, "HATİCE", "")]));
    let first = &item.relationship.closest[0];
    assert_eq!((first.up_a, first.up_b), (2, 2));
    assert_eq!(first.path_a.len(), 3);
    assert_eq!(first.path_a[0], item.husband);
}

#[test]
fn ali_ayse_first_cousins() {
    let c = db();
    import(&c, "ornek-ali.pdf");
    import(&c, "ornek-ayse-kuzen.pdf");
    let (ali, ayse) = (find(&c, "EMRE", "YILMAZ"), find(&c, "AYŞE", "DEMİR"));
    let r = find_relationship(&c, ali, ayse).unwrap().unwrap();
    assert_eq!(r.label, "Birinci dereceden kuzeni");
    let anc: Vec<i64> = r.closest.iter().map(|x| x.ancestor).collect();
    assert_eq!(sorted(anc), sorted(vec![find(&c, "OSMAN", "KAYA"), find(&c, "ELİF", "KAYA")]));
    // Ayşe'ye göre Emre'nin annesi teyzesidir
    let fatma = find(&c, "NURTEN", "YILMAZ");
    assert_eq!(find_relationship(&c, ayse, fatma).unwrap().unwrap().label, "Teyzesi");
    // İki aile yeniden içe aktarıldıktan sonra da tek akraba evliliği
    assert_eq!(consanguinity_report(&c).unwrap().len(), 1);
}

#[test]
fn ancestor_cycle_free_on_pedigree_collapse() {
    let c = db();
    import(&c, "ornek-ali.pdf");
    let (ali, ahmet, defne) = (find(&c, "EMRE", "YILMAZ"), find(&c, "RIZA", ""), find(&c, "DEFNE", "YILMAZ"));
    let r = find_relationship(&c, ali, ahmet).unwrap().unwrap();
    assert_eq!(r.closest.len(), 1);
    assert_eq!((r.closest[0].up_a, r.closest[0].up_b), (3, 0));
    assert_eq!(r.label, "Büyük dedesi");
    assert_eq!(find_relationship(&c, defne, ali).unwrap().unwrap().label, "Babası");
    let unrelated = insert_person(&c, &PersonInput { given_name: "YABANCI".into(), ..Default::default() }, true).unwrap();
    assert_eq!(find_relationship(&c, ali, unrelated).unwrap(), None);
}

#[test]
fn double_cousins_two_common_ancestor_pairs() {
    let c = db();
    let p = |n: &str, s: Sex| insert_person(&c, &PersonInput { given_name: n.into(), sex: Some(s), ..Default::default() }, true).unwrap();
    // İki kardeş (A1, A2) başka iki kardeşle (B1, B2) evli
    let (ga, gb) = (p("GA_BABA", Sex::Male), p("GA_ANNE", Sex::Female));
    let (gc, gd) = (p("GB_BABA", Sex::Male), p("GB_ANNE", Sex::Female));
    let (a1, a2) = (p("A1", Sex::Male), p("A2", Sex::Male));
    let (b1, b2) = (p("B1", Sex::Female), p("B2", Sex::Female));
    for x in [a1, a2] {
        set_parent(&c, x, ga, Sex::Male, "manual").unwrap();
        set_parent(&c, x, gb, Sex::Female, "manual").unwrap();
    }
    for x in [b1, b2] {
        set_parent(&c, x, gc, Sex::Male, "manual").unwrap();
        set_parent(&c, x, gd, Sex::Female, "manual").unwrap();
    }
    let (k1, k2) = (p("K1", Sex::Male), p("K2", Sex::Female));
    set_parent(&c, k1, a1, Sex::Male, "manual").unwrap();
    set_parent(&c, k1, b1, Sex::Female, "manual").unwrap();
    set_parent(&c, k2, a2, Sex::Male, "manual").unwrap();
    set_parent(&c, k2, b2, Sex::Female, "manual").unwrap();
    let ca = common_ancestors(&c, k1, k2).unwrap();
    assert_eq!(sorted(ca.iter().map(|x| x.ancestor).collect()), sorted(vec![ga, gb, gc, gd]));
    assert_eq!(find_relationship(&c, k1, k2).unwrap().unwrap().label, "Birinci dereceden kuzeni");
}

#[test]
fn half_siblings() {
    let c = db();
    let p = |n: &str, s: Sex| insert_person(&c, &PersonInput { given_name: n.into(), sex: Some(s), ..Default::default() }, true).unwrap();
    let (f, m1, m2) = (p("BABA", Sex::Male), p("ANNE1", Sex::Female), p("ANNE2", Sex::Female));
    let (x, y) = (p("X", Sex::Male), p("Y", Sex::Female));
    set_parent(&c, x, f, Sex::Male, "manual").unwrap();
    set_parent(&c, x, m1, Sex::Female, "manual").unwrap();
    set_parent(&c, y, f, Sex::Male, "manual").unwrap();
    set_parent(&c, y, m2, Sex::Female, "manual").unwrap();
    assert_eq!(find_relationship(&c, x, y).unwrap().unwrap().label, "Baba bir kız kardeşi");
}
