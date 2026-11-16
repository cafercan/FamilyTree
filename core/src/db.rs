//! SQLite şeması ve kişi işlemleri.

use crate::{Error, Record, Result, Sex};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Kişinin düzenlenebilir bilgileri.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersonInput {
    pub given_name: String,
    pub surname: String,
    pub sex: Option<Sex>,
    pub father_name: String,
    pub mother_name: String,
    pub birth_place: String,
    pub birth_date: String,
    pub death_date: String,
    pub alive: Option<bool>,
    pub province_district: String,
    pub neighborhood: String,
    pub register_no: String,
    pub marital_status: String,
    pub tc: Option<String>,
    pub notes: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Person {
    pub id: i64,
    #[serde(flatten)]
    pub info: PersonInput,
    /// PDF'te satırı olmayan, ağacı bağlamak için oluşturulmuş kişi.
    pub placeholder: bool,
    /// Elle girilmiş alanlar; içe aktarma bunlara dokunmaz.
    pub manual_fields: Vec<String>,
}

impl From<&Record> for PersonInput {
    fn from(r: &Record) -> Self {
        let status = crate::tr_fold(&r.status);
        let alive = if status == "sağ" {
            Some(true)
        } else if status.starts_with("ölü") || !r.death_date.is_empty() {
            Some(false)
        } else {
            None
        };
        PersonInput {
            given_name: r.given_name.clone(),
            surname: r.surname.clone(),
            sex: r.sex,
            father_name: r.father_name.clone(),
            mother_name: r.mother_name.clone(),
            birth_place: r.birth_place.clone(),
            birth_date: r.birth_date.clone(),
            death_date: r.death_date.clone(),
            alive,
            province_district: r.province_district.clone(),
            neighborhood: r.neighborhood.clone(),
            register_no: r.register_no.clone(),
            marital_status: r.marital_status.clone(),
            tc: r.tc.clone(),
            notes: String::new(),
        }
    }
}

/// İç içe kullanılabilen işlem: SAVEPOINT ile başlar, `commit` edilmeden düşerse geri alınır.
pub(crate) struct Atomic<'a> {
    conn: &'a Connection,
    name: String,
    done: bool,
}

impl<'a> Atomic<'a> {
    pub(crate) fn begin(conn: &'a Connection) -> Result<Atomic<'a>> {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let name = format!("sp{}", NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
        conn.execute_batch(&format!("SAVEPOINT {name}"))?;
        Ok(Atomic { conn, name, done: false })
    }

    pub(crate) fn commit(mut self) -> Result<()> {
        self.conn.execute_batch(&format!("RELEASE {}", self.name))?;
        self.done = true;
        Ok(())
    }
}

impl Drop for Atomic<'_> {
    fn drop(&mut self) {
        if !self.done {
            let _ = self.conn.execute_batch(&format!("ROLLBACK TO {0}; RELEASE {0}", self.name));
        }
    }
}

impl std::ops::Deref for Atomic<'_> {
    type Target = Connection;
    fn deref(&self) -> &Connection {
        self.conn
    }
}

/// `PersonInput` alanlarının adları (veritabanı sütunlarıyla aynı).
pub const FIELDS: &[&str] = &[
    "given_name", "surname", "sex", "father_name", "mother_name", "birth_place", "birth_date",
    "death_date", "alive", "province_district", "neighborhood", "register_no", "marital_status",
    "tc", "notes",
];

impl PersonInput {
    /// Alan değeri, karşılaştırma ve doluluk denetimi için metin olarak ("" = boş).
    pub fn field(&self, name: &str) -> String {
        match name {
            "given_name" => self.given_name.clone(),
            "surname" => self.surname.clone(),
            "sex" => self.sex.map(|s| sex_code(s).to_string()).unwrap_or_default(),
            "father_name" => self.father_name.clone(),
            "mother_name" => self.mother_name.clone(),
            "birth_place" => self.birth_place.clone(),
            "birth_date" => self.birth_date.clone(),
            "death_date" => self.death_date.clone(),
            "alive" => self.alive.map(|a| if a { "1" } else { "0" }.to_string()).unwrap_or_default(),
            "province_district" => self.province_district.clone(),
            "neighborhood" => self.neighborhood.clone(),
            "register_no" => self.register_no.clone(),
            "marital_status" => self.marital_status.clone(),
            "tc" => self.tc.clone().unwrap_or_default(),
            "notes" => self.notes.clone(),
            _ => String::new(),
        }
    }

    /// `name` alanını `other`'dakiyle değiştirir.
    pub fn copy_field(&mut self, name: &str, other: &PersonInput) {
        match name {
            "given_name" => self.given_name = other.given_name.clone(),
            "surname" => self.surname = other.surname.clone(),
            "sex" => self.sex = other.sex,
            "father_name" => self.father_name = other.father_name.clone(),
            "mother_name" => self.mother_name = other.mother_name.clone(),
            "birth_place" => self.birth_place = other.birth_place.clone(),
            "birth_date" => self.birth_date = other.birth_date.clone(),
            "death_date" => self.death_date = other.death_date.clone(),
            "alive" => self.alive = other.alive,
            "province_district" => self.province_district = other.province_district.clone(),
            "neighborhood" => self.neighborhood = other.neighborhood.clone(),
            "register_no" => self.register_no = other.register_no.clone(),
            "marital_status" => self.marital_status = other.marital_status.clone(),
            "tc" => self.tc = other.tc.clone(),
            "notes" => self.notes = other.notes.clone(),
            _ => {}
        }
    }
}

pub(crate) fn sex_code(s: Sex) -> &'static str {
    match s {
        Sex::Male => "E",
        Sex::Female => "K",
    }
}

fn sex_from_code(s: Option<String>) -> Option<Sex> {
    match s.as_deref() {
        Some("E") => Some(Sex::Male),
        Some("K") => Some(Sex::Female),
        _ => None,
    }
}

fn role_code(s: Sex) -> &'static str {
    match s {
        Sex::Male => "father",
        Sex::Female => "mother",
    }
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS person (
    id INTEGER PRIMARY KEY,
    given_name TEXT NOT NULL DEFAULT '',
    surname TEXT NOT NULL DEFAULT '',
    sex TEXT CHECK (sex IN ('E','K')),
    father_name TEXT NOT NULL DEFAULT '',
    mother_name TEXT NOT NULL DEFAULT '',
    birth_place TEXT NOT NULL DEFAULT '',
    birth_date TEXT NOT NULL DEFAULT '',
    death_date TEXT NOT NULL DEFAULT '',
    alive INTEGER,
    province_district TEXT NOT NULL DEFAULT '',
    neighborhood TEXT NOT NULL DEFAULT '',
    register_no TEXT NOT NULL DEFAULT '',
    marital_status TEXT NOT NULL DEFAULT '',
    tc TEXT UNIQUE,
    notes TEXT NOT NULL DEFAULT '',
    placeholder INTEGER NOT NULL DEFAULT 0,
    manual_fields TEXT NOT NULL DEFAULT '[]',
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE TABLE IF NOT EXISTS parent_link (
    child_id INTEGER NOT NULL REFERENCES person(id) ON DELETE CASCADE,
    parent_id INTEGER NOT NULL REFERENCES person(id) ON DELETE CASCADE,
    role TEXT NOT NULL CHECK (role IN ('father','mother')),
    origin TEXT NOT NULL CHECK (origin IN ('pdf','manual')),
    PRIMARY KEY (child_id, role)
);
CREATE INDEX IF NOT EXISTS parent_link_parent ON parent_link(parent_id);
CREATE TABLE IF NOT EXISTS spouse_link (
    a_id INTEGER NOT NULL REFERENCES person(id) ON DELETE CASCADE,
    b_id INTEGER NOT NULL REFERENCES person(id) ON DELETE CASCADE,
    PRIMARY KEY (a_id, b_id),
    CHECK (a_id < b_id)
);
CREATE TABLE IF NOT EXISTS source (
    id INTEGER PRIMARY KEY,
    file_name TEXT NOT NULL,
    imported_at TEXT NOT NULL DEFAULT (datetime('now')),
    root_person_id INTEGER REFERENCES person(id) ON DELETE SET NULL
);
CREATE TABLE IF NOT EXISTS person_source (
    person_id INTEGER NOT NULL REFERENCES person(id) ON DELETE CASCADE,
    source_id INTEGER NOT NULL REFERENCES source(id) ON DELETE CASCADE,
    row_no INTEGER NOT NULL,
    relation_text TEXT NOT NULL,
    PRIMARY KEY (person_id, source_id, row_no)
);
CREATE TABLE IF NOT EXISTS not_same (
    a_id INTEGER NOT NULL REFERENCES person(id) ON DELETE CASCADE,
    b_id INTEGER NOT NULL REFERENCES person(id) ON DELETE CASCADE,
    PRIMARY KEY (a_id, b_id),
    CHECK (a_id < b_id)
);
";

/// Veritabanını açar (klasörü ve dosyayı gerekirse oluşturur) ve şemayı günceller.
pub fn open(path: &Path) -> Result<Connection> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| Error::Invalid(format!("Klasör oluşturulamadı: {e}")))?;
    }
    let conn = Connection::open(path)?;
    migrate(&conn)?;
    Ok(conn)
}

pub fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    let v: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if v < 1 {
        conn.execute_batch(SCHEMA)?;
        conn.execute_batch("PRAGMA user_version = 1;")?;
    }
    if v < 2 {
        // 2: en üst kuşağı baba/ana adlarından tamamlama eklendi; önceki sürümle içe
        // aktarılmış ağaçlarda bir kez çalıştır
        crate::family::complete_top_parents(conn)?;
        conn.execute_batch("PRAGMA user_version = 2;")?;
    }
    if v < 3 {
        // 3: ağaç ayarları (başlangıç kişisi)
        conn.execute_batch("CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL); PRAGMA user_version = 3;")?;
    }
    Ok(())
}

fn non_empty_fields(input: &PersonInput) -> Vec<String> {
    FIELDS.iter().filter(|f| !input.field(f).is_empty()).map(|f| f.to_string()).collect()
}

fn write_fields(conn: &Connection, id: i64, i: &PersonInput) -> Result<()> {
    conn.execute(
        "UPDATE person SET given_name=?2, surname=?3, sex=?4, father_name=?5, mother_name=?6,
         birth_place=?7, birth_date=?8, death_date=?9, alive=?10, province_district=?11,
         neighborhood=?12, register_no=?13, marital_status=?14, tc=?15, notes=?16 WHERE id=?1",
        params![
            id,
            i.given_name,
            i.surname,
            i.sex.map(sex_code),
            i.father_name,
            i.mother_name,
            i.birth_place,
            i.birth_date,
            i.death_date,
            i.alive,
            i.province_district,
            i.neighborhood,
            i.register_no,
            i.marital_status,
            i.tc.as_deref().filter(|t| !t.is_empty()),
            i.notes,
        ],
    )?;
    Ok(())
}

fn set_manual_fields(conn: &Connection, id: i64, fields: &[String]) -> Result<()> {
    let json = serde_json::to_string(fields).expect("json");
    conn.execute("UPDATE person SET manual_fields=?2 WHERE id=?1", params![id, json])?;
    Ok(())
}

/// Yeni kişi ekler. `manual` ise dolu alanların hepsi elle girilmiş sayılır.
pub fn insert_person(conn: &Connection, input: &PersonInput, manual: bool) -> Result<i64> {
    conn.execute("INSERT INTO person DEFAULT VALUES", [])?;
    let id = conn.last_insert_rowid();
    write_fields(conn, id, input)?;
    if manual {
        set_manual_fields(conn, id, &non_empty_fields(input))?;
    }
    Ok(id)
}

pub(crate) fn insert_placeholder(conn: &Connection, input: &PersonInput) -> Result<i64> {
    let id = insert_person(conn, input, false)?;
    conn.execute("UPDATE person SET placeholder=1 WHERE id=?1", [id])?;
    Ok(id)
}

/// PDF'ten gelen bilgileri yazar: elle girilmiş alanlara dokunmaz, boş PDF değeri
/// mevcut değeri silmez.
pub(crate) fn update_person_from_pdf(conn: &Connection, id: i64, input: &PersonInput) -> Result<()> {
    let p = get_person(conn, id)?;
    let mut info = p.info.clone();
    for f in FIELDS {
        if !p.manual_fields.iter().any(|m| m == f) && !input.field(f).is_empty() {
            info.copy_field(f, input);
        }
    }
    write_fields(conn, id, &info)?;
    conn.execute("UPDATE person SET placeholder=0 WHERE id=?1", [id])?;
    Ok(())
}

/// Kullanıcının düzenlemesini yazar; değişen alanlar elle girilmiş olarak işaretlenir.
pub fn update_person_manual(conn: &Connection, id: i64, input: &PersonInput) -> Result<()> {
    let p = get_person(conn, id)?;
    let mut manual = p.manual_fields.clone();
    for f in FIELDS {
        if p.info.field(f) != input.field(f) && !manual.iter().any(|m| m == f) {
            manual.push(f.to_string());
        }
    }
    write_fields(conn, id, input)?;
    set_manual_fields(conn, id, &manual)?;
    conn.execute("UPDATE person SET placeholder=0 WHERE id=?1", [id])?;
    Ok(())
}

const PERSON_COLS: &str = "id, given_name, surname, sex, father_name, mother_name, birth_place, birth_date,
    death_date, alive, province_district, neighborhood, register_no, marital_status, tc, notes,
    placeholder, manual_fields";

fn row_to_person(r: &rusqlite::Row) -> rusqlite::Result<Person> {
    let manual: String = r.get(17)?;
    Ok(Person {
        id: r.get(0)?,
        info: PersonInput {
            given_name: r.get(1)?,
            surname: r.get(2)?,
            sex: sex_from_code(r.get(3)?),
            father_name: r.get(4)?,
            mother_name: r.get(5)?,
            birth_place: r.get(6)?,
            birth_date: r.get(7)?,
            death_date: r.get(8)?,
            alive: r.get(9)?,
            province_district: r.get(10)?,
            neighborhood: r.get(11)?,
            register_no: r.get(12)?,
            marital_status: r.get(13)?,
            tc: r.get(14)?,
            notes: r.get(15)?,
        },
        placeholder: r.get(16)?,
        manual_fields: serde_json::from_str(&manual).unwrap_or_default(),
    })
}

/// Ağacın başlangıç kişisi (açılışta ağacın kökü); kişi artık yoksa `None`.
pub fn home_person(conn: &Connection) -> Result<Option<i64>> {
    let v: Option<String> =
        conn.query_row("SELECT value FROM meta WHERE key = 'home_person'", [], |r| r.get(0)).optional()?;
    let Some(id) = v.and_then(|x| x.parse::<i64>().ok()) else { return Ok(None) };
    Ok(get_person(conn, id).ok().map(|p| p.id))
}

pub fn set_home_person(conn: &Connection, id: i64) -> Result<()> {
    get_person(conn, id)?;
    conn.execute(
        "INSERT INTO meta (key, value) VALUES ('home_person', ?1) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [id.to_string()],
    )?;
    Ok(())
}

pub fn get_person(conn: &Connection, id: i64) -> Result<Person> {
    conn.query_row(&format!("SELECT {PERSON_COLS} FROM person WHERE id=?1"), [id], row_to_person)
        .optional()?
        .ok_or(Error::NotFound(id))
}

/// Tüm kişiler, ada göre sıralı.
pub fn list_persons(conn: &Connection) -> Result<Vec<Person>> {
    let mut st = conn.prepare(&format!("SELECT {PERSON_COLS} FROM person ORDER BY given_name, surname, id"))?;
    let rows = st.query_map([], row_to_person)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn delete_person(conn: &Connection, id: i64) -> Result<()> {
    if conn.execute("DELETE FROM person WHERE id=?1", [id])? == 0 {
        return Err(Error::NotFound(id));
    }
    Ok(())
}

/// `ancestor`, `id`'nin kendisi ya da atası mı?
fn is_ancestor_or_self(conn: &Connection, ancestor: i64, id: i64) -> Result<bool> {
    let found: Option<i64> = conn
        .query_row(
            "WITH RECURSIVE up(id) AS (SELECT ?1 UNION SELECT parent_id FROM parent_link JOIN up ON child_id = up.id)
             SELECT id FROM up WHERE id = ?2",
            [id, ancestor],
            |r| r.get(0),
        )
        .optional()?;
    Ok(found.is_some())
}

/// `parent`'ı `child`'ın babası/annesi yapar. `origin`: "pdf" ya da "manual".
pub fn set_parent(conn: &Connection, child: i64, parent: i64, role: Sex, origin: &str) -> Result<()> {
    get_person(conn, child)?;
    get_person(conn, parent)?;
    if is_ancestor_or_self(conn, child, parent)? {
        return Err(Error::Invalid("Bir kişi kendi atasının ebeveyni olamaz".into()));
    }
    let existing: Option<i64> = conn
        .query_row(
            "SELECT parent_id FROM parent_link WHERE child_id=?1 AND role=?2",
            params![child, role_code(role)],
            |r| r.get(0),
        )
        .optional()?;
    match existing {
        Some(e) if e == parent => return Ok(()),
        Some(_) if origin != "manual" => {
            return Err(Error::Invalid(format!(
                "Çakışan ebeveyn: {} kişisinin {} zaten kayıtlı",
                child,
                if role == Sex::Male { "babası" } else { "annesi" }
            )))
        }
        _ => {}
    }
    conn.execute(
        "INSERT OR REPLACE INTO parent_link (child_id, parent_id, role, origin) VALUES (?1, ?2, ?3, ?4)",
        params![child, parent, role_code(role), origin],
    )?;
    Ok(())
}

pub fn remove_parent(conn: &Connection, child: i64, role: Sex) -> Result<()> {
    conn.execute("DELETE FROM parent_link WHERE child_id=?1 AND role=?2", params![child, role_code(role)])?;
    Ok(())
}

pub fn set_spouse(conn: &Connection, a: i64, b: i64) -> Result<()> {
    if a == b {
        return Err(Error::Invalid("Kişi kendisiyle evli olamaz".into()));
    }
    get_person(conn, a)?;
    get_person(conn, b)?;
    conn.execute("INSERT OR IGNORE INTO spouse_link (a_id, b_id) VALUES (?1, ?2)", params![a.min(b), a.max(b)])?;
    Ok(())
}

pub fn remove_spouse(conn: &Connection, a: i64, b: i64) -> Result<()> {
    conn.execute("DELETE FROM spouse_link WHERE a_id=?1 AND b_id=?2", params![a.min(b), a.max(b)])?;
    Ok(())
}

/// (baba, anne)
pub fn parents(conn: &Connection, id: i64) -> Result<(Option<i64>, Option<i64>)> {
    let get = |role: &str| -> Result<Option<i64>> {
        Ok(conn
            .query_row("SELECT parent_id FROM parent_link WHERE child_id=?1 AND role=?2", params![id, role], |r| {
                r.get(0)
            })
            .optional()?)
    };
    Ok((get("father")?, get("mother")?))
}

fn ids(conn: &Connection, sql: &str, id: i64) -> Result<Vec<i64>> {
    let mut st = conn.prepare(sql)?;
    let rows = st.query_map([id], |r| r.get(0))?;
    Ok(rows.collect::<rusqlite::Result<Vec<i64>>>()?)
}

pub fn children(conn: &Connection, id: i64) -> Result<Vec<i64>> {
    ids(conn, "SELECT child_id FROM parent_link WHERE parent_id=?1 ORDER BY child_id", id)
}

pub fn spouses(conn: &Connection, id: i64) -> Result<Vec<i64>> {
    ids(
        conn,
        "SELECT b_id FROM spouse_link WHERE a_id=?1 UNION SELECT a_id FROM spouse_link WHERE b_id=?1 ORDER BY 1",
        id,
    )
}

/// `of` ile aynı anne-babaya sahip yeni bir kişi ekler. `of`'un ebeveynleri kayıtlı
/// değilse (adları biliniyorsa adlarıyla) yer tutucu ebeveynler oluşturulur.
/// Amca/hala babanın, dayı/teyze annenin kardeşi olarak bu yolla eklenir.
pub fn add_sibling(conn: &Connection, of: i64, input: &PersonInput) -> Result<i64> {
    let tx = Atomic::begin(conn)?;
    let base = get_person(&tx, of)?;
    let (mut father, mut mother) = parents(&tx, of)?;
    if father.is_none() {
        let name = PersonInput { given_name: base.info.father_name.clone(), sex: Some(Sex::Male), ..Default::default() };
        let id = insert_placeholder(&tx, &name)?;
        set_parent(&tx, of, id, Sex::Male, "manual")?;
        father = Some(id);
    }
    if mother.is_none() {
        let name = PersonInput { given_name: base.info.mother_name.clone(), sex: Some(Sex::Female), ..Default::default() };
        let id = insert_placeholder(&tx, &name)?;
        set_parent(&tx, of, id, Sex::Female, "manual")?;
        mother = Some(id);
    }
    let id = insert_person(&tx, input, true)?;
    set_parent(&tx, id, father.expect("baba"), Sex::Male, "manual")?;
    set_parent(&tx, id, mother.expect("anne"), Sex::Female, "manual")?;
    tx.commit()?;
    Ok(id)
}

/// `drop`'u `keep`'e katar: `keep`'in boş alanları `drop`'tan dolar, tüm bağlar ve
/// kaynaklar `keep`'e taşınır (çakışmada `keep`'inki kalır), `drop` silinir.
pub fn merge_persons(conn: &Connection, keep: i64, drop: i64) -> Result<()> {
    if keep == drop {
        return Ok(());
    }
    let tx = Atomic::begin(conn)?;
    let k = get_person(&tx, keep)?;
    let d = get_person(&tx, drop)?;
    if is_ancestor_or_self(&tx, keep, drop)? || is_ancestor_or_self(&tx, drop, keep)? {
        return Err(Error::Invalid("Biri diğerinin atası olan iki kişi birleştirilemez".into()));
    }
    let mut info = k.info.clone();
    let mut manual = k.manual_fields.clone();
    for f in FIELDS {
        if info.field(f).is_empty() && !d.info.field(f).is_empty() {
            info.copy_field(f, &d.info);
            if d.manual_fields.iter().any(|m| m == f) && !manual.iter().any(|m| m == f) {
                manual.push(f.to_string());
            }
        }
    }
    // tc UNIQUE: önce drop'unkini boşalt
    tx.execute("UPDATE person SET tc=NULL WHERE id=?1", [drop])?;
    write_fields(&tx, keep, &info)?;
    set_manual_fields(&tx, keep, &manual)?;
    if !d.placeholder {
        tx.execute("UPDATE person SET placeholder=0 WHERE id=?1", [keep])?;
    }
    // drop'un ebeveynleri: keep'te o rol boşsa taşı
    tx.execute(
        "UPDATE OR IGNORE parent_link SET child_id=?1 WHERE child_id=?2",
        params![keep, drop],
    )?;
    // drop'un çocukları: aynı rolde keep'e bağla
    tx.execute("UPDATE parent_link SET parent_id=?1 WHERE parent_id=?2", params![keep, drop])?;
    for s in spouses(&tx, drop)? {
        if s != keep {
            set_spouse(&tx, keep, s)?;
        }
    }
    tx.execute("UPDATE OR IGNORE person_source SET person_id=?1 WHERE person_id=?2", params![keep, drop])?;
    for (a, b) in [("a_id", "b_id"), ("b_id", "a_id")] {
        let others = ids(&tx, &format!("SELECT {b} FROM not_same WHERE {a}=?1"), drop)?;
        for o in others {
            if o != keep {
                tx.execute(
                    "INSERT OR IGNORE INTO not_same (a_id, b_id) VALUES (?1, ?2)",
                    params![keep.min(o), keep.max(o)],
                )?;
            }
        }
    }
    tx.execute("DELETE FROM person WHERE id=?1", [drop])?;
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        migrate(&c).unwrap();
        c
    }

    fn p(name: &str, sex: Sex) -> PersonInput {
        PersonInput { given_name: name.into(), surname: "UYDURMA".into(), sex: Some(sex), ..Default::default() }
    }

    #[test]
    fn migrate_is_idempotent() {
        let c = db();
        let id = insert_person(&c, &p("EMRE", Sex::Male), false).unwrap();
        migrate(&c).unwrap();
        assert_eq!(get_person(&c, id).unwrap().info.given_name, "EMRE");
        let v: i64 = c.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(v, 3);
    }

    /// Şema 1 → 2: önceki sürümle içe aktarılmış ağaçlarda en üst kuşak bir kez tamamlanır.
    #[test]
    fn upgrade_to_v2_completes_top_parents_once() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("eski.db");
        {
            let c = Connection::open(&path).unwrap();
            c.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
            c.execute_batch(SCHEMA).unwrap();
            c.execute_batch("PRAGMA user_version = 1;").unwrap();
            let mut x = p("RIZA", Sex::Male);
            x.father_name = "ÖMER".into();
            x.mother_name = "ŞERİFE".into();
            insert_person(&c, &x, false).unwrap();
        }
        let c = open(&path).unwrap();
        let v: i64 = c.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(v, 3);
        assert_eq!(list_persons(&c).unwrap().len(), 3, "Ömer ve Şerife eklendi");
        // Kullanıcı yer tutucuyu silse bile tekrar açılışta yeniden eklenmez
        let omer = list_persons(&c).unwrap().into_iter().find(|x| x.info.given_name == "ÖMER").unwrap().id;
        delete_person(&c, omer).unwrap();
        drop(c);
        let c = open(&path).unwrap();
        assert_eq!(list_persons(&c).unwrap().len(), 2);
    }

    /// Ağacın başlangıç kişisi dosyada saklanır; kişi silinirse boşalır.
    #[test]
    fn home_person_roundtrip() {
        let c = db();
        assert_eq!(home_person(&c).unwrap(), None);
        let a = insert_person(&c, &p("EMRE", Sex::Male), false).unwrap();
        let b = insert_person(&c, &p("SU", Sex::Female), false).unwrap();
        set_home_person(&c, a).unwrap();
        assert_eq!(home_person(&c).unwrap(), Some(a));
        set_home_person(&c, b).unwrap();
        assert_eq!(home_person(&c).unwrap(), Some(b));
        assert!(matches!(set_home_person(&c, 9999), Err(Error::NotFound(9999))));
        delete_person(&c, b).unwrap();
        assert_eq!(home_person(&c).unwrap(), None, "silinen kişi başlangıç olamaz");
    }

    #[test]
    fn open_creates_file_and_parent_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a/b/familytree.db");
        let c = open(&path).unwrap();
        insert_person(&c, &p("EMRE", Sex::Male), false).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn record_to_input_alive() {
        let mut r = Record { given_name: "X".into(), status: "Sağ".into(), ..Default::default() };
        assert_eq!(PersonInput::from(&r).alive, Some(true));
        r.status = "Ölümün Tespiti".into();
        assert_eq!(PersonInput::from(&r).alive, Some(false));
        r.status = "Ölüm".into();
        r.death_date = "01/01/1950".into();
        let i = PersonInput::from(&r);
        assert_eq!((i.alive, i.death_date.as_str()), (Some(false), "01/01/1950"));
        r.status = String::new();
        r.death_date = String::new();
        assert_eq!(PersonInput::from(&r).alive, None);
    }

    #[test]
    fn manual_update_records_fields() {
        let c = db();
        let id = insert_person(&c, &p("EMRE", Sex::Male), false).unwrap();
        assert!(get_person(&c, id).unwrap().manual_fields.is_empty());
        let mut changed = p("EMRE", Sex::Male);
        changed.birth_place = "UYDURMAKÖY".into();
        update_person_manual(&c, id, &changed).unwrap();
        let got = get_person(&c, id).unwrap();
        assert_eq!(got.info.birth_place, "UYDURMAKÖY");
        assert_eq!(got.manual_fields, vec!["birth_place".to_string()]);
        let manual = insert_person(&c, &p("NUR", Sex::Female), true).unwrap();
        let mf = get_person(&c, manual).unwrap().manual_fields;
        assert!(mf.contains(&"given_name".to_string()) && mf.contains(&"sex".to_string()));
        assert!(!mf.contains(&"birth_place".to_string()), "boş alan elle sayılmaz");
    }

    #[test]
    fn set_parent_rejects_cycle() {
        let c = db();
        let a = insert_person(&c, &p("A", Sex::Male), false).unwrap();
        let b = insert_person(&c, &p("B", Sex::Male), false).unwrap();
        let d = insert_person(&c, &p("C", Sex::Male), false).unwrap();
        set_parent(&c, b, a, Sex::Male, "manual").unwrap();
        set_parent(&c, d, b, Sex::Male, "manual").unwrap();
        assert!(matches!(set_parent(&c, a, d, Sex::Male, "manual"), Err(Error::Invalid(_))));
        assert!(matches!(set_parent(&c, a, a, Sex::Male, "manual"), Err(Error::Invalid(_))));
        assert_eq!(parents(&c, d).unwrap(), (Some(b), None));
        assert_eq!(children(&c, a).unwrap(), vec![b]);
    }

    #[test]
    fn set_parent_conflict_pdf_vs_manual() {
        let c = db();
        let child = insert_person(&c, &p("ÇOCUK", Sex::Male), false).unwrap();
        let f1 = insert_person(&c, &p("BABA1", Sex::Male), false).unwrap();
        let f2 = insert_person(&c, &p("BABA2", Sex::Male), false).unwrap();
        set_parent(&c, child, f1, Sex::Male, "pdf").unwrap();
        set_parent(&c, child, f1, Sex::Male, "pdf").unwrap(); // aynısı: sorun yok
        assert!(matches!(set_parent(&c, child, f2, Sex::Male, "pdf"), Err(Error::Invalid(_))));
        set_parent(&c, child, f2, Sex::Male, "manual").unwrap();
        assert_eq!(parents(&c, child).unwrap(), (Some(f2), None));
        remove_parent(&c, child, Sex::Male).unwrap();
        assert_eq!(parents(&c, child).unwrap(), (None, None));
    }

    #[test]
    fn spouses_roundtrip() {
        let c = db();
        let a = insert_person(&c, &p("A", Sex::Male), false).unwrap();
        let b = insert_person(&c, &p("B", Sex::Female), false).unwrap();
        set_spouse(&c, b, a).unwrap();
        set_spouse(&c, a, b).unwrap();
        assert_eq!(spouses(&c, a).unwrap(), vec![b]);
        assert_eq!(spouses(&c, b).unwrap(), vec![a]);
        remove_spouse(&c, a, b).unwrap();
        assert!(spouses(&c, a).unwrap().is_empty());
        assert!(matches!(set_spouse(&c, a, a), Err(Error::Invalid(_))));
    }

    #[test]
    fn add_sibling_creates_placeholder_parents() {
        let c = db();
        let mut ali = p("EMRE", Sex::Male);
        ali.father_name = "MEHMET".into();
        ali.mother_name = "NURTEN".into();
        let a = insert_person(&c, &ali, false).unwrap();
        let s = add_sibling(&c, a, &p("NUR", Sex::Female)).unwrap();
        let (f, m) = parents(&c, a).unwrap();
        assert_eq!(parents(&c, s).unwrap(), (f, m));
        let father = get_person(&c, f.unwrap()).unwrap();
        assert!(father.placeholder);
        assert_eq!(father.info.given_name, "MEHMET");
        assert_eq!(father.info.sex, Some(Sex::Male));
        assert_eq!(get_person(&c, m.unwrap()).unwrap().info.given_name, "NURTEN");
        // İkinci kardeş aynı ebeveynleri kullanır
        let s2 = add_sibling(&c, a, &p("CAN", Sex::Male)).unwrap();
        assert_eq!(parents(&c, s2).unwrap(), (f, m));
        assert_eq!(list_persons(&c).unwrap().len(), 5);
    }

    #[test]
    fn merge_moves_links() {
        let c = db();
        let keep = insert_person(&c, &p("EMRE", Sex::Male), false).unwrap();
        let mut d = p("EMRE", Sex::Male);
        d.birth_place = "ANKARA".into();
        let drop = insert_person(&c, &d, false).unwrap();
        let kid = insert_person(&c, &p("DEFNE", Sex::Female), false).unwrap();
        let wife = insert_person(&c, &p("SELİN", Sex::Female), false).unwrap();
        let dad = insert_person(&c, &p("MEHMET", Sex::Male), false).unwrap();
        set_parent(&c, kid, drop, Sex::Male, "pdf").unwrap();
        set_spouse(&c, drop, wife).unwrap();
        set_parent(&c, drop, dad, Sex::Male, "pdf").unwrap();
        merge_persons(&c, keep, drop).unwrap();
        assert!(matches!(get_person(&c, drop), Err(Error::NotFound(_))));
        assert_eq!(children(&c, keep).unwrap(), vec![kid]);
        assert_eq!(spouses(&c, keep).unwrap(), vec![wife]);
        assert_eq!(parents(&c, keep).unwrap(), (Some(dad), None));
        assert_eq!(get_person(&c, keep).unwrap().info.birth_place, "ANKARA");
    }

    #[test]
    fn merge_rejects_ancestor() {
        let c = db();
        let a = insert_person(&c, &p("A", Sex::Male), false).unwrap();
        let b = insert_person(&c, &p("B", Sex::Male), false).unwrap();
        set_parent(&c, b, a, Sex::Male, "pdf").unwrap();
        assert!(matches!(merge_persons(&c, a, b), Err(Error::Invalid(_))));
    }

    #[test]
    fn delete_cascades_links() {
        let c = db();
        let a = insert_person(&c, &p("A", Sex::Male), false).unwrap();
        let b = insert_person(&c, &p("B", Sex::Male), false).unwrap();
        let w = insert_person(&c, &p("W", Sex::Female), false).unwrap();
        set_parent(&c, b, a, Sex::Male, "pdf").unwrap();
        set_spouse(&c, a, w).unwrap();
        delete_person(&c, a).unwrap();
        assert_eq!(parents(&c, b).unwrap(), (None, None));
        assert!(spouses(&c, w).unwrap().is_empty());
        assert!(matches!(delete_person(&c, a), Err(Error::NotFound(_))));
    }
}
