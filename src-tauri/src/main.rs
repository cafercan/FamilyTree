#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! FamilyTree — masaüstü giriş noktası (Tauri 2). Tüm iş mantığı `familytree-core`'da;
//! burada yalnızca IPC komutları ve dosya diyalogları var. Uygulamanın ağ erişimi yoktur.

use familytree_core::db::{self, Person, PersonInput};
use familytree_core::family::{self, NewRelation, PersonDetail};
use familytree_core::import::{self, ImportDecision, ImportPreview, ImportSummary};
use familytree_core::kinship::{self, ConsanguinityItem, Relationship};
use familytree_core::layout::{self, Projection, TreeLayout};
use familytree_core::{backup, pdf, table, trees, Sex};
use rusqlite::Connection;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::{DialogExt, FilePath};

struct AppState {
    db: Mutex<Connection>,
    /// Açık ağacın adı (`agaclar/<ad>.db`)
    current: Mutex<String>,
    /// Son seçilen PDF: ayrıştırılamazsa anonim düzen dökümü bundan üretilir
    last_pdf: Mutex<Option<Vec<u8>>>,
}

type Res<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn with_db<T>(state: &State<AppState>, f: impl FnOnce(&Connection) -> familytree_core::Result<T>) -> Res<T> {
    let conn = state.db.lock().map_err(err)?;
    f(&conn).map_err(err)
}

/// Uygulama veri klasörü; ağaçlar `agaclar/` altında, son açılan ağaç `son-agac.txt`'de.
fn base_dir() -> PathBuf {
    dirs::data_dir().unwrap_or_else(std::env::temp_dir).join("FamilyTree")
}

fn remember(name: &str) {
    let _ = std::fs::write(base_dir().join("son-agac.txt"), name);
}

#[derive(Serialize)]
struct TreeList {
    names: Vec<String>,
    current: String,
}

fn tree_list(state: &State<AppState>) -> Res<TreeList> {
    Ok(TreeList { names: trees::list_trees(&base_dir()).map_err(err)?, current: state.current.lock().map_err(err)?.clone() })
}

/// Açık veritabanını `name` ağacıyla değiştirir.
fn open_tree(state: &State<AppState>, name: &str) -> Res<()> {
    let conn = db::open(&trees::tree_path(&base_dir(), name).map_err(err)?).map_err(err)?;
    *state.db.lock().map_err(err)? = conn;
    *state.current.lock().map_err(err)? = name.trim().to_string();
    *state.last_pdf.lock().map_err(err)? = None;
    remember(name.trim());
    Ok(())
}

#[tauri::command]
fn list_trees(state: State<AppState>) -> Res<TreeList> {
    tree_list(&state)
}

#[tauri::command]
fn switch_tree(state: State<AppState>, name: String) -> Res<TreeList> {
    if !trees::list_trees(&base_dir()).map_err(err)?.contains(&name) {
        return Err(format!("\"{name}\" adında ağaç yok"));
    }
    open_tree(&state, &name)?;
    tree_list(&state)
}

#[tauri::command]
fn create_tree(state: State<AppState>, name: String) -> Res<TreeList> {
    trees::create_tree(&base_dir(), &name).map_err(err)?;
    open_tree(&state, &name)?;
    tree_list(&state)
}

/// Açık ağacı yeniden adlandırır (dosya taşınırken bağlantı kapatılır).
#[tauri::command]
fn rename_tree(state: State<AppState>, name: String) -> Res<TreeList> {
    let old = state.current.lock().map_err(err)?.clone();
    *state.db.lock().map_err(err)? = Connection::open_in_memory().map_err(err)?;
    let result = trees::rename_tree(&base_dir(), &old, &name);
    // Başarısız olsa da eski ağacı yeniden aç
    open_tree(&state, if result.is_ok() { &name } else { &old })?;
    result.map_err(err)?;
    tree_list(&state)
}

/// Açık olmayan bir ağacı siler.
#[tauri::command]
fn delete_tree(state: State<AppState>, name: String) -> Res<TreeList> {
    if *state.current.lock().map_err(err)? == name {
        return Err("Açık olan ağaç silinemez; önce başka bir ağaca geçin".into());
    }
    trees::delete_tree(&base_dir(), &name).map_err(err)?;
    tree_list(&state)
}

fn local_path(f: FilePath) -> Res<PathBuf> {
    f.into_path().map_err(err)
}

/// Diyaloglar ana iş parçacığını bekletmesin diye komutlar `async` (iş parçacığı havuzunda)
/// çalışır ve `blocking_*` çağrıları orada yapılır.
#[tauri::command]
async fn pick_and_preview_pdf(app: AppHandle, state: State<'_, AppState>) -> Res<Option<ImportPreview>> {
    let Some(file) = app
        .dialog()
        .file()
        .set_title("e-Devlet soy ağacı PDF'ini seçin")
        .add_filter("PDF", &["pdf", "PDF"])
        .blocking_pick_file()
    else {
        return Ok(None);
    };
    let path = local_path(file)?;
    let bytes = std::fs::read(&path).map_err(err)?;
    *state.last_pdf.lock().map_err(err)? = Some(bytes.clone());
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    with_db(&state, |c| import::preview_import(c, &name, &bytes)).map(Some)
}

#[tauri::command]
fn commit_import(state: State<AppState>, preview: ImportPreview, decisions: Vec<ImportDecision>) -> Res<ImportSummary> {
    with_db(&state, |c| import::commit_import(c, &preview, &decisions))
}

#[tauri::command]
fn layout_dump_last(state: State<AppState>) -> Res<Option<String>> {
    let last = state.last_pdf.lock().map_err(err)?;
    match last.as_ref() {
        Some(bytes) => Ok(Some(table::anonymized_layout_dump(&pdf::extract(bytes).map_err(err)?))),
        None => Ok(None),
    }
}

#[tauri::command]
fn list_persons(state: State<AppState>) -> Res<Vec<Person>> {
    with_db(&state, db::list_persons)
}

#[tauri::command]
fn person_detail(state: State<AppState>, id: i64) -> Res<PersonDetail> {
    with_db(&state, |c| family::person_detail(c, id))
}

#[tauri::command]
fn create_person(state: State<AppState>, input: PersonInput, relation: Option<NewRelation>) -> Res<i64> {
    with_db(&state, |c| family::create_person(c, &input, relation))
}

/// Açık ağaçta en üst kuşağın baba/ana adlarından eksik ebeveynleri ekler.
#[tauri::command]
fn complete_top_parents(state: State<AppState>) -> Res<usize> {
    with_db(&state, family::complete_top_parents)
}

#[tauri::command]
fn update_person(state: State<AppState>, id: i64, input: PersonInput) -> Res<()> {
    with_db(&state, |c| db::update_person_manual(c, id, &input))
}

#[tauri::command]
fn delete_person(state: State<AppState>, id: i64) -> Res<()> {
    with_db(&state, |c| db::delete_person(c, id))
}

#[tauri::command]
fn set_parent(state: State<AppState>, child: i64, parent: i64, role: Sex) -> Res<()> {
    with_db(&state, |c| db::set_parent(c, child, parent, role, "manual"))
}

#[tauri::command]
fn remove_parent(state: State<AppState>, child: i64, role: Sex) -> Res<()> {
    with_db(&state, |c| db::remove_parent(c, child, role))
}

#[tauri::command]
fn set_spouse(state: State<AppState>, a: i64, b: i64) -> Res<()> {
    with_db(&state, |c| db::set_spouse(c, a, b))
}

#[tauri::command]
fn remove_spouse(state: State<AppState>, a: i64, b: i64) -> Res<()> {
    with_db(&state, |c| db::remove_spouse(c, a, b))
}

#[tauri::command]
fn merge_persons(state: State<AppState>, keep: i64, drop: i64) -> Res<()> {
    with_db(&state, |c| db::merge_persons(c, keep, drop))
}

#[tauri::command]
fn mark_not_same(state: State<AppState>, a: i64, b: i64) -> Res<()> {
    with_db(&state, |c| import::mark_not_same(c, a, b))
}

#[tauri::command]
fn duplicate_suggestions(state: State<AppState>) -> Res<Vec<(i64, i64, u8)>> {
    with_db(&state, import::duplicate_suggestions)
}

#[tauri::command]
fn find_relationship(state: State<AppState>, a: i64, b: i64) -> Res<Option<Relationship>> {
    with_db(&state, |c| kinship::find_relationship(c, a, b))
}

#[tauri::command]
fn consanguinity_report(state: State<AppState>) -> Res<Vec<ConsanguinityItem>> {
    with_db(&state, kinship::consanguinity_report)
}

/// `kind`: "ancestors", "descendants" ya da "both" (kum saati)
#[tauri::command]
fn tree_layout(state: State<AppState>, kind: String, id: i64, gens: u32, projection: Projection, twins: bool) -> Res<TreeLayout> {
    with_db(&state, |c| match kind.as_str() {
        "descendants" => layout::descendants_layout(c, id, gens, projection, twins),
        "both" => layout::hourglass_layout(c, id, gens, projection, twins),
        _ => layout::ancestors_layout(c, id, gens, projection, twins),
    })
}

#[derive(Serialize)]
struct RelationshipView {
    relationship: Relationship,
    layout: TreeLayout,
}

#[tauri::command]
fn relationship_layout(state: State<AppState>, a: i64, b: i64, projection: Projection) -> Res<Option<RelationshipView>> {
    with_db(&state, |c| {
        let Some(rel) = kinship::find_relationship(c, a, b)? else { return Ok(None) };
        let layout = layout::relationship_layout(c, &rel, projection)?;
        Ok(Some(RelationshipView { relationship: rel, layout }))
    })
}

#[tauri::command]
fn outline(state: State<AppState>, id: i64, gens: u32, ancestors: bool) -> Res<String> {
    with_db(&state, |c| layout::outline(c, id, gens, ancestors))
}

#[tauri::command]
async fn export_backup(app: AppHandle, state: State<'_, AppState>) -> Res<bool> {
    let tree = state.current.lock().map_err(err)?.clone();
    let (json, today) = with_db(&state, |c| {
        let today: String = c.query_row("SELECT date('now','localtime')", [], |r| r.get(0))?;
        Ok((backup::export_json(c)?, today))
    })?;
    let Some(file) = app
        .dialog()
        .file()
        .set_title("Yedeği kaydet")
        .set_file_name(format!("familytree-yedek-{tree}-{today}.json"))
        .add_filter("FamilyTree yedeği", &["json"])
        .blocking_save_file()
    else {
        return Ok(false);
    };
    std::fs::write(local_path(file)?, json).map_err(err)?;
    Ok(true)
}

#[tauri::command]
async fn import_backup(app: AppHandle, state: State<'_, AppState>) -> Res<bool> {
    let Some(file) = app
        .dialog()
        .file()
        .set_title("Yedeği geri yükle")
        .add_filter("FamilyTree yedeği", &["json"])
        .blocking_pick_file()
    else {
        return Ok(false);
    };
    let text = std::fs::read_to_string(local_path(file)?).map_err(|_| "Dosya okunamadı ya da metin değil".to_string())?;
    with_db(&state, |c| backup::import_json(c, &text))?;
    Ok(true)
}

/// `%XX` kodlu metni çözer (başlıklar yalnızca ASCII taşıyabilir)
fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Ağaç görünümünü dosyaya kaydeder. Gövde ham bayt (PNG, SVG, JPEG ya da metin);
/// başlıklar: `x-kind` (png|svg|pdf|txt), `x-name` (önerilen dosya adı, %XX kodlu),
/// PDF için `x-px-w`, `x-px-h` (JPEG piksel boyutu), `x-css-w`, `x-css-h` (ağaç boyutu).
#[tauri::command]
async fn save_export(app: AppHandle, request: tauri::ipc::Request<'_>) -> Res<bool> {
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err("Beklenmeyen veri biçimi".into());
    };
    let header = |k: &str| request.headers().get(k).and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
    let kind = header("x-kind");
    let (ext, filter) = match kind.as_str() {
        "png" => ("png", "PNG görüntü"),
        "svg" => ("svg", "SVG vektör çizim"),
        "pdf" => ("pdf", "PDF belge"),
        "txt" => ("txt", "Metin"),
        _ => return Err(format!("Bilinmeyen biçim: {kind}")),
    };
    let name = percent_decode(&header("x-name"));
    let data = if kind == "pdf" {
        let num = |k: &str| header(k).parse::<f32>().unwrap_or(0.0);
        familytree_core::export::jpeg_to_pdf(bytes, num("x-px-w") as u32, num("x-px-h") as u32, num("x-css-w"), num("x-css-h"), &name)
    } else {
        bytes.clone()
    };
    let Some(file) = app
        .dialog()
        .file()
        .set_title("Ağacı kaydet")
        .set_file_name(format!("{name}.{ext}"))
        .add_filter(filter, &[ext])
        .blocking_save_file()
    else {
        return Ok(false);
    };
    std::fs::write(local_path(file)?, data).map_err(err)?;
    Ok(true)
}

/// Paketlenmiş uygulamanın CI denemesi (`--selftest`): pencere açmadan arayüz dosyaları
/// gömülü mü bakar ve bellekteki bir veritabanında küçük bir uydurma aileyle akrabalık
/// hesabını çalıştırır.
fn selftest(context: &tauri::Context<tauri::Wry>) -> i32 {
    let ui = context.assets().get(&"index.html".into()).is_some();
    let core = (|| -> familytree_core::Result<usize> {
        let c = Connection::open_in_memory()?;
        db::migrate(&c)?;
        let p = |n: &str, s: Sex| PersonInput { given_name: n.into(), sex: Some(s), ..Default::default() };
        let dede = family::create_person(&c, &p("DEDE", Sex::Male), None)?;
        let a = family::create_person(&c, &p("A", Sex::Male), Some(NewRelation { to: dede, kind: family::RelationKind::Child }))?;
        let b = family::create_person(&c, &p("B", Sex::Female), Some(NewRelation { to: dede, kind: family::RelationKind::Child }))?;
        let x = family::create_person(&c, &p("X", Sex::Male), Some(NewRelation { to: a, kind: family::RelationKind::Child }))?;
        let y = family::create_person(&c, &p("Y", Sex::Female), Some(NewRelation { to: b, kind: family::RelationKind::Child }))?;
        db::set_spouse(&c, x, y)?;
        Ok(kinship::consanguinity_report(&c)?.len())
    })();
    let ok = ui && matches!(core, Ok(1));
    println!(
        "selftest {}: arayüz={}, akraba evliliği={:?}",
        if ok { "ok" } else { "HATA" },
        if ui { "tamam" } else { "YOK" },
        core.map_err(|e| e.to_string())
    );
    if ok {
        0
    } else {
        1
    }
}

fn main() {
    let context = tauri::generate_context!();
    if std::env::args().skip(1).any(|a| a == "--selftest") {
        std::process::exit(selftest(&context));
    }
    let last = std::fs::read_to_string(base_dir().join("son-agac.txt")).ok();
    let opened = trees::prepare(&base_dir(), last.as_deref().map(str::trim))
        .and_then(|name| Ok((db::open(&trees::tree_path(&base_dir(), &name)?)?, name)));
    let (conn, current) = match opened {
        Ok(x) => x,
        Err(e) => {
            eprintln!("Veritabanı açılamadı ({}): {e}", base_dir().display());
            std::process::exit(1);
        }
    };
    remember(&current);
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState { db: Mutex::new(conn), current: Mutex::new(current), last_pdf: Mutex::new(None) })
        .invoke_handler(tauri::generate_handler![
            pick_and_preview_pdf,
            commit_import,
            layout_dump_last,
            list_persons,
            person_detail,
            create_person,
            update_person,
            delete_person,
            set_parent,
            remove_parent,
            set_spouse,
            remove_spouse,
            merge_persons,
            mark_not_same,
            duplicate_suggestions,
            find_relationship,
            consanguinity_report,
            tree_layout,
            relationship_layout,
            outline,
            export_backup,
            import_backup,
            list_trees,
            switch_tree,
            create_tree,
            rename_tree,
            delete_tree,
            save_export,
            complete_top_parents,
        ])
        .run(context)
        .expect("FamilyTree başlatılamadı");
}
