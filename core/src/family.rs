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
