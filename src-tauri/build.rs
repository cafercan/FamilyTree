/// Uygulamanın kendi IPC komutları. AppManifest'te bildirilir; her biri bir
/// `allow-<komut>` izni alır ve capabilities/default.json yalnız bunları
/// ana pencereye verir. main.rs `generate_handler!` listesiyle aynı olmalı.
const COMMANDS: &[&str] = &[
    "pick_and_preview_pdf",
    "commit_import",
    "layout_dump_last",
    "list_persons",
    "person_detail",
    "create_person",
    "update_person",
    "delete_person",
    "set_parent",
    "remove_parent",
    "set_spouse",
    "remove_spouse",
    "merge_persons",
    "mark_not_same",
    "duplicate_suggestions",
    "find_relationship",
    "consanguinity_report",
    "tree_layout",
    "relationship_layout",
    "outline",
    "export_backup",
    "import_backup",
    "list_trees",
    "switch_tree",
    "create_tree",
    "rename_tree",
    "delete_tree",
    "save_export",
    "complete_top_parents",
];

fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new().app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("tauri build script failed")
}
