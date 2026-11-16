//! JSON yedek ve geri yükleme.

use crate::{Error, Result};
use rusqlite::types::{Value as Sql, ValueRef};
use rusqlite::Connection;
use serde_json::{json, Map, Value};

const FORMAT: &str = "familytree-backup";
const VERSION: i64 = 1;

/// Yedeklenen tablolar, yabancı anahtar sırasına göre.
const TABLES: &[(&str, &str)] = &[
    ("persons", "person"),
    ("sources", "source"),
    ("parent_links", "parent_link"),
    ("spouse_links", "spouse_link"),
    ("person_sources", "person_source"),
    ("not_same", "not_same"),
];

fn to_json(v: ValueRef) -> Value {
    match v {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(i) => json!(i),
        ValueRef::Real(f) => json!(f),
        ValueRef::Text(t) => json!(String::from_utf8_lossy(t)),
        ValueRef::Blob(_) => Value::Null,
    }
}

fn to_sql(v: &Value) -> Sql {
    match v {
        Value::Null => Sql::Null,
        Value::Bool(b) => Sql::Integer(*b as i64),
        Value::Number(n) => n.as_i64().map(Sql::Integer).unwrap_or_else(|| Sql::Real(n.as_f64().unwrap_or(0.0))),
        Value::String(s) => Sql::Text(s.clone()),
        other => Sql::Text(other.to_string()),
    }
}

fn columns(conn: &Connection, table: &str) -> Result<Vec<String>> {
    let st = conn.prepare(&format!("SELECT * FROM {table} LIMIT 0"))?;
    Ok(st.column_names().map(|c| c.to_string()).collect())
}

/// Tüm veritabanını okunabilir JSON olarak döndürür.
pub fn export_json(conn: &Connection) -> Result<String> {
    let mut root = Map::new();
    root.insert("format".into(), json!(FORMAT));
    root.insert("version".into(), json!(VERSION));
    for (key, table) in TABLES {
        let cols = columns(conn, table)?;
        let mut st = conn.prepare(&format!("SELECT * FROM {table} ORDER BY rowid"))?;
        let mut rows = st.query([])?;
        let mut out = Vec::new();
        while let Some(r) = rows.next()? {
            let mut obj = Map::new();
            for (i, c) in cols.iter().enumerate() {
                obj.insert(c.clone(), to_json(r.get_ref(i)?));
            }
            out.push(Value::Object(obj));
        }
        root.insert((*key).into(), Value::Array(out));
    }
    Ok(serde_json::to_string_pretty(&Value::Object(root)).expect("json") + "\n")
}

/// Yedeği geri yükler: mevcut verinin yerini tamamen alır. Hata olursa hiçbir şey değişmez.
pub fn import_json(conn: &Connection, json: &str) -> Result<()> {
    let not_backup = || Error::Invalid("Bu dosya bir FamilyTree yedeği değil".into());
    let v: Value = serde_json::from_str(json).map_err(|_| not_backup())?;
    if v.get("format").and_then(Value::as_str) != Some(FORMAT) || v.get("version").and_then(Value::as_i64) != Some(VERSION) {
        return Err(not_backup());
    }
    let tx = crate::db::Atomic::begin(conn)?;
    for (_, table) in TABLES.iter().rev() {
        tx.execute(&format!("DELETE FROM {table}"), [])?;
    }
    for (key, table) in TABLES {
        let cols = columns(&tx, table)?;
        let rows = v.get(*key).and_then(Value::as_array).ok_or_else(not_backup)?;
        let sql = format!(
            "INSERT INTO {table} ({}) VALUES ({})",
            cols.join(", "),
            (1..=cols.len()).map(|i| format!("?{i}")).collect::<Vec<_>>().join(", ")
        );
        let mut st = tx.prepare(&sql)?;
        for row in rows {
            let obj = row.as_object().ok_or_else(not_backup)?;
            let vals: Vec<Sql> = cols.iter().map(|c| obj.get(c).map(to_sql).unwrap_or(Sql::Null)).collect();
            st.execute(rusqlite::params_from_iter(vals))?;
        }
    }
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::import::{commit_import, preview_import, ImportDecision};

    fn db_with_ali() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        db::migrate(&c).unwrap();
        let bytes = std::fs::read(format!("{}/../tests/fixtures/ornek-ali.pdf", env!("CARGO_MANIFEST_DIR"))).unwrap();
        let p = preview_import(&c, "ornek-ali.pdf", &bytes).unwrap();
        let d: Vec<_> = p.nodes.iter().map(|n| ImportDecision { index: n.index, merge_into: None, split_duplicate: false }).collect();
        commit_import(&c, &p, &d).unwrap();
        let a = db::list_persons(&c).unwrap()[0].id;
        let b = db::list_persons(&c).unwrap()[1].id;
        crate::import::mark_not_same(&c, a, b).unwrap();
        let mut info = db::get_person(&c, a).unwrap().info;
        info.notes = "elle not".into();
        db::update_person_manual(&c, a, &info).unwrap();
        c
    }

    #[test]
    fn roundtrip_preserves_everything() {
        let src = db_with_ali();
        let json = export_json(&src).unwrap();
        assert!(json.contains("\"format\": \"familytree-backup\""));
        let dst = Connection::open_in_memory().unwrap();
        db::migrate(&dst).unwrap();
        // Hedefte var olan veri tamamen değişir
        db::insert_person(&dst, &db::PersonInput { given_name: "SİLİNECEK".into(), ..Default::default() }, true).unwrap();
        import_json(&dst, &json).unwrap();
        assert_eq!(export_json(&dst).unwrap(), json);
        assert_eq!(db::list_persons(&dst).unwrap(), db::list_persons(&src).unwrap());
    }

    #[test]
    fn rejects_foreign_json() {
        let c = db_with_ali();
        let before = export_json(&c).unwrap();
        for bad in ["{}", "[1,2]", "bozuk", r#"{"format":"familytree-backup","version":99}"#] {
            match import_json(&c, bad) {
                Err(Error::Invalid(m)) => assert!(m.contains("FamilyTree yedeği"), "{m}"),
                other => panic!("{bad}: {other:?}"),
            }
        }
        assert_eq!(export_json(&c).unwrap(), before, "başarısız geri yükleme veriyi değiştirmez");
    }
}
