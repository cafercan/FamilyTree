//! Akrabalık hesabı: ortak atalar, Türkçe akrabalık adı, akraba evliliği raporu.

use crate::db;
use crate::{Result, Sex};
use std::collections::{BTreeSet, HashMap, VecDeque};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommonAncestor {
    pub ancestor: i64,
    pub up_a: u32,
    pub up_b: u32,
    /// a'dan ataya (iki uç dahil)
    pub path_a: Vec<i64>,
    /// b'den ataya (iki uç dahil)
    pub path_b: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Relationship {
    pub a: i64,
    pub b: i64,
    pub closest: Vec<CommonAncestor>,
    /// B'nin A'ya göre ne olduğu ("Teyzesi", "2. dereceden kuzeni" …)
    pub label: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Side {
    Paternal,
    Maternal,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsanguinityItem {
    pub husband: i64,
    pub wife: i64,
    pub relationship: Relationship,
}

/// Kişinin tüm ataları (kendisi dahil, mesafe 0): ata → (mesafe, bir önceki kişi).
/// BFS en kısa yolu verir; ziyaret kümesi sayesinde aynı ata iki yoldan gelse de biter.
fn ancestors(conn: &Connection, start: i64) -> Result<HashMap<i64, (u32, Option<i64>)>> {
    let mut seen: HashMap<i64, (u32, Option<i64>)> = HashMap::new();
    seen.insert(start, (0, None));
    let mut queue = VecDeque::from([start]);
    while let Some(x) = queue.pop_front() {
        let d = seen[&x].0;
        let (f, m) = db::parents(conn, x)?;
        for p in [f, m].into_iter().flatten() {
            if let std::collections::hash_map::Entry::Vacant(e) = seen.entry(p) {
                e.insert((d + 1, Some(x)));
                queue.push_back(p);
            }
        }
    }
    Ok(seen)
}

/// `from`'dan `to` atasına giden yol (iki uç dahil).
fn path_to(map: &HashMap<i64, (u32, Option<i64>)>, to: i64) -> Vec<i64> {
    let mut path = vec![to];
    let mut cur = to;
    while let Some((_, Some(prev))) = map.get(&cur) {
        path.push(*prev);
        cur = *prev;
    }
    path.reverse();
    path
}

/// En yakın ortak atalar: ortak olup başka bir ortak atanın atası olmayanlar.
pub fn common_ancestors(conn: &Connection, a: i64, b: i64) -> Result<Vec<CommonAncestor>> {
    let anc_a = ancestors(conn, a)?;
    let anc_b = ancestors(conn, b)?;
    let common: Vec<i64> = anc_a.keys().filter(|k| anc_b.contains_key(k)).copied().collect();
    let mut out = Vec::new();
    for &c in &common {
        let mut shadowed = false;
        for &d in &common {
            if d != c && ancestors(conn, d)?.contains_key(&c) {
                shadowed = true;
                break;
            }
        }
        if !shadowed {
            out.push(CommonAncestor {
                ancestor: c,
                up_a: anc_a[&c].0,
                up_b: anc_b[&c].0,
                path_a: path_to(&anc_a, c),
                path_b: path_to(&anc_b, c),
            });
        }
    }
    out.sort_by_key(|x| (x.up_a + x.up_b, x.ancestor));
    Ok(out)
}

fn by_sex<'a>(sex: Option<Sex>, male: &'a str, female: &'a str, unknown: &'a str) -> &'a str {
    match sex {
        Some(Sex::Male) => male,
        Some(Sex::Female) => female,
        None => unknown,
    }
}

/// B'nin A'ya göre akrabalık adı. `side`: A'dan ortak ataya giden yolun ilk adımı.
pub fn relationship_label(up_a: u32, up_b: u32, side: Side, sex_b: Option<Sex>) -> String {
    match (up_a, up_b) {
        (0, 0) => "Kendisi".into(),
        (1, 0) => by_sex(sex_b, "Babası", "Annesi", "Ebeveyni").into(),
        (2, 0) => match (side, sex_b) {
            (Side::Paternal, Some(Sex::Male)) => "Dedesi (baba tarafı)".into(),
            (Side::Paternal, Some(Sex::Female)) => "Babaannesi".into(),
            (Side::Maternal, Some(Sex::Male)) => "Dedesi (anne tarafı)".into(),
            (Side::Maternal, Some(Sex::Female)) => "Anneannesi".into(),
            _ => by_sex(sex_b, "Dedesi", "Ninesi", "Büyük ebeveyni").into(),
        },
        (3, 0) => by_sex(sex_b, "Büyük dedesi", "Büyük annesi", "Büyük ebeveyni").into(),
        (n, 0) => format!("{}. dereceden {}", n - 2, by_sex(sex_b, "büyük dedesi", "büyük annesi", "büyük ebeveyni")),
        (0, 1) => by_sex(sex_b, "Oğlu", "Kızı", "Çocuğu").into(),
        (0, 2) => "Torunu".into(),
        (0, n) => format!("{}. kuşak torunu", n - 1),
        (1, 1) => by_sex(sex_b, "Erkek kardeşi", "Kız kardeşi", "Kardeşi").into(),
        (a, 1) => {
            let word = match side {
                Side::Paternal => by_sex(sex_b, "amcası", "halası", "amcası/halası"),
                Side::Maternal => by_sex(sex_b, "dayısı", "teyzesi", "dayısı/teyzesi"),
                Side::Unknown => by_sex(sex_b, "amcası/dayısı", "halası/teyzesi", "amca/dayı/hala/teyzesi"),
            };
            if a == 2 {
                let mut c = word.chars();
                let first = c.next().expect("boş değil").to_uppercase().collect::<String>();
                first + c.as_str()
            } else {
                format!("{}. kuşak büyük {word}", a - 2)
            }
        }
        (1, 2) => "Yeğeni".into(),
        (1, 3) => "Yeğeninin çocuğu".into(),
        (1, 4) => "Yeğeninin torunu".into(),
        (1, n) => format!("Yeğeninin {}. kuşak torunu", n - 3),
        (a, b) => {
            let k = a.min(b) - 1;
            let d = a.abs_diff(b);
            let base = if k == 1 { "Birinci dereceden kuzeni".to_string() } else { format!("{k}. dereceden kuzeni") };
            if d > 0 {
                format!("{base} ({d} kuşak fark)")
            } else {
                base
            }
        }
    }
}

/// B'nin A'ya göre akrabalığı; ortak ataları yoksa `None`.
pub fn find_relationship(conn: &Connection, a: i64, b: i64) -> Result<Option<Relationship>> {
    let closest = common_ancestors(conn, a, b)?;
    let Some(first) = closest.first() else { return Ok(None) };
    let side = match first.path_a.get(1) {
        Some(p) => {
            let (f, m) = db::parents(conn, a)?;
            if f == Some(*p) {
                Side::Paternal
            } else if m == Some(*p) {
                Side::Maternal
            } else {
                Side::Unknown
            }
        }
        None => Side::Unknown,
    };
    let sex_b = db::get_person(conn, b)?.info.sex;
    let mut label = relationship_label(first.up_a, first.up_b, side, sex_b);
    // Tek ortak ebeveyn: baba bir / anne bir kardeş
    let at_level: Vec<&CommonAncestor> =
        closest.iter().filter(|c| (c.up_a, c.up_b) == (first.up_a, first.up_b)).collect();
    if (first.up_a, first.up_b) == (1, 1) && at_level.len() == 1 {
        let parent_sex = db::get_person(conn, first.ancestor)?.info.sex;
        let prefix = by_sex(parent_sex, "Baba bir", "Anne bir", "Yarım");
        label = format!("{prefix} {}", by_sex(sex_b, "erkek kardeşi", "kız kardeşi", "kardeşi"));
    }
    Ok(Some(Relationship { a, b, closest, label }))
}

/// Ortak atası olan çiftler (eşler ve ortak çocuğu olan anne-babalar), en yakından uzağa.
pub fn consanguinity_report(conn: &Connection) -> Result<Vec<ConsanguinityItem>> {
    let mut couples: BTreeSet<(i64, i64)> = BTreeSet::new();
    {
        let mut st = conn.prepare(
            "SELECT a_id, b_id FROM spouse_link
             UNION
             SELECT f.parent_id, m.parent_id FROM parent_link f
             JOIN parent_link m ON m.child_id = f.child_id AND m.role = 'mother'
             WHERE f.role = 'father'",
        )?;
        for r in st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))? {
            let (x, y) = r?;
            couples.insert((x.min(y), x.max(y)));
        }
    }
    let mut out = Vec::new();
    for (x, y) in couples {
        let (sx, sy) = (db::get_person(conn, x)?.info.sex, db::get_person(conn, y)?.info.sex);
        let (husband, wife) = if sx == Some(Sex::Female) || sy == Some(Sex::Male) { (y, x) } else { (x, y) };
        if let Some(rel) = find_relationship(conn, husband, wife)? {
            out.push(ConsanguinityItem { husband, wife, relationship: rel });
        }
    }
    out.sort_by_key(|i| i.relationship.closest.first().map(|c| c.up_a + c.up_b).unwrap_or(u32::MAX));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use Side::*;
    use Sex::*;

    #[test]
    fn label_table() {
        let l = relationship_label;
        assert_eq!(l(0, 0, Unknown, None), "Kendisi");
        assert_eq!(l(1, 0, Paternal, Some(Male)), "Babası");
        assert_eq!(l(1, 0, Maternal, Some(Female)), "Annesi");
        assert_eq!(l(0, 1, Unknown, Some(Male)), "Oğlu");
        assert_eq!(l(0, 1, Unknown, Some(Female)), "Kızı");
        assert_eq!(l(0, 1, Unknown, None), "Çocuğu");
        assert_eq!(l(2, 0, Paternal, Some(Male)), "Dedesi (baba tarafı)");
        assert_eq!(l(2, 0, Paternal, Some(Female)), "Babaannesi");
        assert_eq!(l(2, 0, Maternal, Some(Male)), "Dedesi (anne tarafı)");
        assert_eq!(l(2, 0, Maternal, Some(Female)), "Anneannesi");
        assert_eq!(l(3, 0, Paternal, Some(Male)), "Büyük dedesi");
        assert_eq!(l(3, 0, Maternal, Some(Female)), "Büyük annesi");
        assert_eq!(l(4, 0, Paternal, Some(Male)), "2. dereceden büyük dedesi");
        assert_eq!(l(0, 2, Unknown, None), "Torunu");
        assert_eq!(l(0, 3, Unknown, None), "2. kuşak torunu");
        assert_eq!(l(1, 1, Unknown, Some(Male)), "Erkek kardeşi");
        assert_eq!(l(1, 1, Unknown, Some(Female)), "Kız kardeşi");
        assert_eq!(l(1, 1, Unknown, None), "Kardeşi");
        assert_eq!(l(2, 1, Paternal, Some(Male)), "Amcası");
        assert_eq!(l(2, 1, Paternal, Some(Female)), "Halası");
        assert_eq!(l(2, 1, Maternal, Some(Male)), "Dayısı");
        assert_eq!(l(2, 1, Maternal, Some(Female)), "Teyzesi");
        assert_eq!(l(3, 1, Paternal, Some(Male)), "1. kuşak büyük amcası");
        assert_eq!(l(1, 2, Unknown, None), "Yeğeni");
        assert_eq!(l(1, 3, Unknown, None), "Yeğeninin çocuğu");
        assert_eq!(l(2, 2, Paternal, None), "Birinci dereceden kuzeni");
        assert_eq!(l(3, 2, Paternal, None), "Birinci dereceden kuzeni (1 kuşak fark)");
        assert_eq!(l(3, 3, Maternal, None), "2. dereceden kuzeni");
        assert_eq!(l(4, 3, Paternal, None), "2. dereceden kuzeni (1 kuşak fark)");
    }
}
