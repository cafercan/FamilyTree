//! Arayüzün kullandığı bileşik işlemler: ilişkiyle kişi ekleme, kişi ayrıntısı.

use crate::db::{self, Person, PersonInput};
use crate::{Error, Result, Sex};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RelationKind {
    Father,
    Mother,
    Child,
    Spouse,
    Sibling,
}

/// Yeni kişi `to`'nun neyi olacak.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewRelation {
    pub to: i64,
    pub kind: RelationKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Source {
    pub file_name: String,
    pub imported_at: String,
    pub relation_text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PersonDetail {
    pub person: Person,
    pub father: Option<Person>,
    pub mother: Option<Person>,
    pub children: Vec<Person>,
    pub spouses: Vec<Person>,
    /// Aynı anne ya da babadan kardeşler
    pub siblings: Vec<Person>,
    pub sources: Vec<Source>,
}

/// Elle yeni kişi ekler ve isteğe bağlı olarak var olan bir kişiye bağlar; hata olursa
/// hiçbir şey kalmaz.
pub fn create_person(conn: &Connection, input: &PersonInput, relation: Option<NewRelation>) -> Result<i64> {
    let tx = crate::db::Atomic::begin(conn)?;
    let id = match relation {
        None => db::insert_person(&tx, input, true)?,
        Some(NewRelation { to, kind: RelationKind::Sibling }) => db::add_sibling(&tx, to, input)?,
        Some(NewRelation { to, kind }) => {
            let id = db::insert_person(&tx, input, true)?;
            match kind {
                RelationKind::Father => db::set_parent(&tx, to, id, Sex::Male, "manual")?,
                RelationKind::Mother => db::set_parent(&tx, to, id, Sex::Female, "manual")?,
                RelationKind::Child => {
                    let role = db::get_person(&tx, to)?.info.sex.ok_or_else(|| {
                        Error::Invalid("Çocuk eklemek için ebeveynin cinsiyeti bilinmeli".into())
                    })?;
                    db::set_parent(&tx, id, to, role, "manual")?
                }
                RelationKind::Spouse => db::set_spouse(&tx, to, id)?,
                RelationKind::Sibling => unreachable!(),
            }
            id
        }
    };
    tx.commit()?;
    Ok(id)
}

fn persons(conn: &Connection, ids: impl IntoIterator<Item = i64>) -> Result<Vec<Person>> {
    ids.into_iter().map(|i| db::get_person(conn, i)).collect()
}

/// Kişi, anne-babası, çocukları, eşleri, kardeşleri ve hangi PDF'lerde geçtiği.
/// En üst kuşağı tamamlar: ebeveyni kayıtlı olmayan ama baba/ana adı bilinen kişilere
/// bu adlarla yer tutucu ebeveyn ekler. Aynı baba ve ana adına ve aynı haneye (cilt-hane)
/// sahip kardeşler aynı yer tutucu ebeveynleri paylaşır. Eklenen kişi sayısını döndürür.
pub fn complete_top_parents(conn: &Connection) -> Result<usize> {
    use crate::tr_fold;
    use std::collections::HashMap;
    // Hane: cilt-hane (birey no hariç) + kayıt yeri
    let household = |p: &PersonInput| {
        let reg: Vec<&str> = p.register_no.split('-').collect();
        let ch = if reg.len() >= 2 { format!("{}-{}", reg[0].trim(), reg[1].trim()) } else { String::new() };
        format!("{ch}|{}", tr_fold(&p.province_district))
    };
    let key = |p: &PersonInput, role: Sex| {
        let (name, other) = if role == Sex::Male { (&p.father_name, &p.mother_name) } else { (&p.mother_name, &p.father_name) };
        (role, tr_fold(name), tr_fold(other), household(p))
    };
    let tx = db::Atomic::begin(conn)?;
    let persons: Vec<Person> = db::list_persons(&tx)?.into_iter().filter(|p| !p.placeholder).collect();
    // Daha önce eklenmiş yer tutucu ebeveynleri kardeşler için yeniden kullan
    let mut made: HashMap<(Sex, String, String, String), i64> = HashMap::new();
    for p in &persons {
        let (f, m) = db::parents(&tx, p.id)?;
        for (role, parent) in [(Sex::Male, f), (Sex::Female, m)] {
            if let Some(x) = parent {
                if db::get_person(&tx, x)?.placeholder {
                    made.entry(key(&p.info, role)).or_insert(x);
                }
            }
        }
    }
    let mut created = 0;
    for p in &persons {
        let (f, m) = db::parents(&tx, p.id)?;
        for (role, have, name) in [(Sex::Male, f, &p.info.father_name), (Sex::Female, m, &p.info.mother_name)] {
            if have.is_some() || name.trim().is_empty() || name.trim() == "-" {
                continue;
            }
            let k = key(&p.info, role);
            let id = match made.get(&k) {
                Some(id) => *id,
                None => {
                    let input = PersonInput { given_name: name.trim().to_string(), sex: Some(role), ..Default::default() };
                    let id = db::insert_placeholder(&tx, &input)?;
                    created += 1;
                    made.insert(k, id);
                    id
                }
            };
            db::set_parent(&tx, p.id, id, role, "pdf")?;
        }
    }
    tx.commit()?;
    Ok(created)
}

pub fn person_detail(conn: &Connection, id: i64) -> Result<PersonDetail> {
    let person = db::get_person(conn, id)?;
    let (f, m) = db::parents(conn, id)?;
    let mut sibling_ids: Vec<i64> = Vec::new();
    for parent in [f, m].into_iter().flatten() {
        for c in db::children(conn, parent)? {
            if c != id && !sibling_ids.contains(&c) {
                sibling_ids.push(c);
            }
        }
    }
    sibling_ids.sort();
    // Eşler: kayıtlı eş bağları ve ortak çocuğu olan diğer ebeveynler
    let mut spouse_ids = db::spouses(conn, id)?;
    for c in db::children(conn, id)? {
        let (cf, cm) = db::parents(conn, c)?;
        for other in [cf, cm].into_iter().flatten() {
            if other != id && !spouse_ids.contains(&other) {
                spouse_ids.push(other);
            }
        }
    }
    let mut st = conn.prepare(
        "SELECT s.file_name, s.imported_at, ps.relation_text FROM person_source ps
         JOIN source s ON s.id = ps.source_id WHERE ps.person_id = ?1 ORDER BY s.id, ps.row_no",
    )?;
    let sources = st
        .query_map([id], |r| Ok(Source { file_name: r.get(0)?, imported_at: r.get(1)?, relation_text: r.get(2)? }))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(PersonDetail {
        person,
        father: f.map(|x| db::get_person(conn, x)).transpose()?,
        mother: m.map(|x| db::get_person(conn, x)).transpose()?,
        children: persons(conn, db::children(conn, id)?)?,
        spouses: persons(conn, spouse_ids)?,
        siblings: persons(conn, sibling_ids)?,
        sources,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        db::migrate(&c).unwrap();
        c
    }

    fn p(name: &str, sex: Option<Sex>) -> PersonInput {
        PersonInput { given_name: name.into(), sex, ..Default::default() }
    }

    #[test]
    fn create_with_relations() {
        let c = db();
        let ali = create_person(&c, &p("EMRE", Some(Sex::Male)), None).unwrap();
        let rel = |kind| Some(NewRelation { to: ali, kind });
        let f = create_person(&c, &p("MEHMET", Some(Sex::Male)), rel(RelationKind::Father)).unwrap();
        let m = create_person(&c, &p("NURTEN", Some(Sex::Female)), rel(RelationKind::Mother)).unwrap();
        let k = create_person(&c, &p("DEFNE", Some(Sex::Female)), rel(RelationKind::Child)).unwrap();
        let w = create_person(&c, &p("SELİN", Some(Sex::Female)), rel(RelationKind::Spouse)).unwrap();
        let s = create_person(&c, &p("NUR", Some(Sex::Female)), rel(RelationKind::Sibling)).unwrap();
        assert_eq!(db::parents(&c, ali).unwrap(), (Some(f), Some(m)));
        assert_eq!(db::parents(&c, k).unwrap(), (Some(ali), None));
        assert_eq!(db::spouses(&c, ali).unwrap(), vec![w]);
        assert_eq!(db::parents(&c, s).unwrap(), (Some(f), Some(m)));
        // Amca: babanın kardeşi
        let amca = create_person(&c, &p("HÜSEYİN", Some(Sex::Male)), Some(NewRelation { to: f, kind: RelationKind::Sibling })).unwrap();
        assert_eq!(crate::kinship::find_relationship(&c, ali, amca).unwrap().unwrap().label, "Amcası");
        assert!(db::get_person(&c, amca).unwrap().manual_fields.contains(&"given_name".to_string()));
    }

    #[test]
    fn create_child_of_unknown_sex_fails_cleanly() {
        let c = db();
        let x = create_person(&c, &p("X", None), None).unwrap();
        let before = db::list_persons(&c).unwrap().len();
        assert!(matches!(
            create_person(&c, &p("Y", None), Some(NewRelation { to: x, kind: RelationKind::Child })),
            Err(Error::Invalid(_))
        ));
        assert_eq!(db::list_persons(&c).unwrap().len(), before, "yarım kalan kişi oluşmaz");
    }

    #[test]
    fn detail_lists_family() {
        let c = db();
        let ali = create_person(&c, &p("EMRE", Some(Sex::Male)), None).unwrap();
        let rel = |kind| Some(NewRelation { to: ali, kind });
        create_person(&c, &p("MEHMET", Some(Sex::Male)), rel(RelationKind::Father)).unwrap();
        let s = create_person(&c, &p("NUR", Some(Sex::Female)), rel(RelationKind::Sibling)).unwrap();
        create_person(&c, &p("DEFNE", Some(Sex::Female)), rel(RelationKind::Child)).unwrap();
        let d = person_detail(&c, ali).unwrap();
        assert_eq!(d.father.unwrap().info.given_name, "MEHMET");
        assert!(d.mother.unwrap().placeholder, "kardeş eklerken anne yer tutucu oluştu");
        assert_eq!(d.siblings.iter().map(|x| x.id).collect::<Vec<_>>(), vec![s]);
        assert_eq!(d.children.len(), 1);
        assert!(d.sources.is_empty());
    }

    #[test]
    fn complete_top_parents_from_names() {
        let c = db();
        let rec = |n: &str, s: Sex, f: &str, m: &str, reg: &str| PersonInput {
            given_name: n.into(), sex: Some(s), father_name: f.into(), mother_name: m.into(),
            register_no: reg.into(), province_district: "Çankırı/Ilgaz/KUZUÖREN KÖYÜ".into(), ..Default::default()
        };
        // İki kardeş (aynı baba/ana adı, aynı cilt-hane) ve ilgisiz biri
        let a = db::insert_person(&c, &rec("RIZA", Sex::Male, "ÖMER", "ŞERİFE", "3-17-2"), false).unwrap();
        let b = db::insert_person(&c, &rec("ZÜHAL", Sex::Female, "ÖMER", "ŞERİFE", "3-17-5"), false).unwrap();
        let x = db::insert_person(&c, &rec("SALİH", Sex::Male, "ÖMER", "HAVVA", "9-1-1"), false).unwrap();
        // Babası kayıtlı ama annesi olmayan kişi: yalnız anne eklenir
        let kid = db::insert_person(&c, &rec("KADİR", Sex::Male, "RIZA", "NERİMAN", "3-40-1"), false).unwrap();
        db::set_parent(&c, kid, a, Sex::Male, "pdf").unwrap();
        // Adı bilinmeyen: eklenmez
        db::insert_person(&c, &rec("YALNIZ", Sex::Male, "", "", ""), false).unwrap();

        assert_eq!(complete_top_parents(&c).unwrap(), 2 + 2 + 1, "kardeşlere ortak 2, Salih'e 2, Kadir'e anne");
        let (fa, ma) = db::parents(&c, a).unwrap();
        assert_eq!(db::parents(&c, b).unwrap(), (fa, ma), "kardeşler aynı ebeveynleri paylaşır");
        let father = db::get_person(&c, fa.unwrap()).unwrap();
        assert!(father.placeholder);
        assert_eq!((father.info.given_name.as_str(), father.info.sex), ("ÖMER", Some(Sex::Male)));
        assert_eq!(db::get_person(&c, ma.unwrap()).unwrap().info.given_name, "ŞERİFE");
        let (fx, _) = db::parents(&c, x).unwrap();
        assert_ne!(fx, fa, "aynı adlı ama başka hanedeki baba ayrı kişi");
        assert_eq!(db::get_person(&c, db::parents(&c, kid).unwrap().1.unwrap()).unwrap().info.given_name, "NERİMAN");
        // Yeniden çalıştırmak bir şey eklemez
        assert_eq!(complete_top_parents(&c).unwrap(), 0);
    }

    /// PDF'te eş satırı olmasa da ortak çocuğu olan kişi eş olarak görünür.
    #[test]
    fn co_parent_listed_as_spouse() {
        let c = db();
        let k = create_person(&c, &p("ÇOCUK", Some(Sex::Male)), None).unwrap();
        let f = create_person(&c, &p("BABA", Some(Sex::Male)), Some(NewRelation { to: k, kind: RelationKind::Father })).unwrap();
        let m = create_person(&c, &p("ANNE", Some(Sex::Female)), Some(NewRelation { to: k, kind: RelationKind::Mother })).unwrap();
        assert_eq!(person_detail(&c, f).unwrap().spouses.iter().map(|x| x.id).collect::<Vec<_>>(), vec![m]);
        assert_eq!(person_detail(&c, m).unwrap().spouses.iter().map(|x| x.id).collect::<Vec<_>>(), vec![f]);
    }
}
