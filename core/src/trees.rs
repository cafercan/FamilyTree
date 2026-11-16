//! Birden çok bağımsız soy ağacı: her ağaç `agaclar/<ad>.db` dosyasıdır.

use crate::{Error, Result};
use std::path::{Path, PathBuf};

pub const DEFAULT_TREE: &str = "Benim ağacım";

/// Ağaç dosyalarının klasörü (`<veri>/FamilyTree/agaclar`).
pub fn trees_dir(base: &Path) -> PathBuf {
    base.join("agaclar")
}

/// Dosya adında sorun çıkaracak karakterleri reddeder; baş/son boşlukları kırpar.
pub fn clean_name(name: &str) -> Result<String> {
    let n = name.trim();
    let bad = n.is_empty()
        || n.starts_with('.')
        || n.chars().count() > 80
        || n.chars().any(|c| c.is_control() || "/\\:*?\"<>|".contains(c));
    if bad {
        return Err(Error::Invalid(
            "Geçersiz ağaç adı: boş olamaz, nokta ile başlayamaz ve / \\ : * ? \" < > | içeremez".into(),
        ));
    }
    Ok(n.to_string())
}

fn io(e: std::io::Error) -> Error {
    Error::Invalid(format!("Dosya işlemi başarısız: {e}"))
}

/// Ağaç adları, alfabetik.
pub fn list_trees(base: &Path) -> Result<Vec<String>> {
    let dir = trees_dir(base);
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).map_err(io)? {
        let p = e.map_err(io)?.path();
        if p.extension().and_then(|x| x.to_str()) == Some("db") {
            if let Some(stem) = p.file_stem().and_then(|x| x.to_str()) {
                out.push(stem.to_string());
            }
        }
    }
    out.sort_by_key(|n| crate::tr_fold(n));
    Ok(out)
}

pub fn tree_path(base: &Path, name: &str) -> Result<PathBuf> {
    Ok(trees_dir(base).join(format!("{}.db", clean_name(name)?)))
}

fn exists(base: &Path, name: &str) -> Result<bool> {
    let n = crate::tr_fold(&clean_name(name)?);
    Ok(list_trees(base)?.iter().any(|t| crate::tr_fold(t) == n))
}

/// Yeni boş ağaç oluşturur; aynı adda ağaç varsa hata.
pub fn create_tree(base: &Path, name: &str) -> Result<PathBuf> {
    if exists(base, name)? {
        return Err(Error::Invalid(format!("\"{}\" adında bir ağaç zaten var", name.trim())));
    }
    let path = tree_path(base, name)?;
    crate::db::open(&path)?;
    Ok(path)
}

pub fn rename_tree(base: &Path, old: &str, new: &str) -> Result<()> {
    let from = tree_path(base, old)?;
    if !from.exists() {
        return Err(Error::Invalid(format!("\"{old}\" adında ağaç yok")));
    }
    let same = crate::tr_fold(old.trim()) == crate::tr_fold(new.trim());
    if !same && exists(base, new)? {
        return Err(Error::Invalid(format!("\"{}\" adında bir ağaç zaten var", new.trim())));
    }
    std::fs::rename(from, tree_path(base, new)?).map_err(io)
}

pub fn delete_tree(base: &Path, name: &str) -> Result<()> {
    std::fs::remove_file(tree_path(base, name)?).map_err(io)
}

/// İlk sürümün tek veritabanını (`<veri>/FamilyTree/familytree.db`) "Benim ağacım"
/// olarak taşır; hiç ağaç yoksa boş "Benim ağacım" oluşturur. Açılacak ağacın adını döndürür:
/// `last` (son açılan) hâlâ varsa o, yoksa ilk ağaç.
pub fn prepare(base: &Path, last: Option<&str>) -> Result<String> {
    std::fs::create_dir_all(trees_dir(base)).map_err(io)?;
    let legacy = base.join("familytree.db");
    if legacy.exists() && list_trees(base)?.is_empty() {
        std::fs::rename(&legacy, tree_path(base, DEFAULT_TREE)?).map_err(io)?;
    }
    let trees = list_trees(base)?;
    if trees.is_empty() {
        create_tree(base, DEFAULT_TREE)?;
        return Ok(DEFAULT_TREE.into());
    }
    if let Some(l) = last {
        if let Some(t) = trees.iter().find(|t| t.as_str() == l) {
            return Ok(t.clone());
        }
    }
    Ok(trees.iter().find(|t| t.as_str() == DEFAULT_TREE).unwrap_or(&trees[0]).clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_list_rename_delete() {
        let d = tempfile::tempdir().unwrap();
        let b = d.path();
        assert!(list_trees(b).unwrap().is_empty());
        create_tree(b, "Eşimin ağacı").unwrap();
        create_tree(b, " Benim ağacım ").unwrap();
        assert_eq!(list_trees(b).unwrap(), vec!["Benim ağacım", "Eşimin ağacı"]);
        assert!(matches!(create_tree(b, "Benim ağacım"), Err(Error::Invalid(_))), "aynı ad");
        rename_tree(b, "Eşimin ağacı", "Ayşe'nin ailesi").unwrap();
        assert_eq!(list_trees(b).unwrap(), vec!["Ayşe'nin ailesi", "Benim ağacım"]);
        assert!(matches!(rename_tree(b, "Ayşe'nin ailesi", "Benim ağacım"), Err(Error::Invalid(_))));
        delete_tree(b, "Ayşe'nin ailesi").unwrap();
        assert_eq!(list_trees(b).unwrap(), vec!["Benim ağacım"]);
        // Oluşturulan dosya geçerli bir FamilyTree veritabanı
        let c = crate::db::open(&tree_path(b, "Benim ağacım").unwrap()).unwrap();
        assert!(crate::db::list_persons(&c).unwrap().is_empty());
    }

    #[test]
    fn rejects_bad_names() {
        for bad in ["", "   ", "a/b", "a\\\\b", "..", "x:y", "çok*", ".gizli"] {
            assert!(matches!(clean_name(bad), Err(Error::Invalid(_))), "{bad:?}");
        }
        assert_eq!(clean_name("  Eşimin ağacı (anne tarafı) ").unwrap(), "Eşimin ağacı (anne tarafı)");
    }

    #[test]
    fn prepare_migrates_legacy_db() {
        let d = tempfile::tempdir().unwrap();
        let b = d.path();
        let legacy = b.join("familytree.db");
        let c = crate::db::open(&legacy).unwrap();
        crate::db::insert_person(&c, &crate::db::PersonInput { given_name: "EMRE".into(), ..Default::default() }, true).unwrap();
        drop(c);
        assert_eq!(prepare(b, None).unwrap(), DEFAULT_TREE);
        assert!(!legacy.exists(), "eski dosya taşındı");
        let c = crate::db::open(&tree_path(b, DEFAULT_TREE).unwrap()).unwrap();
        assert_eq!(crate::db::list_persons(&c).unwrap().len(), 1, "veri korunur");
        // İkinci açılışta yeniden taşıma yok, son açılan ağaç seçilir
        create_tree(b, "Eşimin ağacı").unwrap();
        assert_eq!(prepare(b, Some("Eşimin ağacı")).unwrap(), "Eşimin ağacı");
        assert_eq!(prepare(b, Some("Silinmiş")).unwrap(), DEFAULT_TREE);
    }

    #[test]
    fn prepare_creates_default_when_empty() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(prepare(d.path(), None).unwrap(), DEFAULT_TREE);
        assert_eq!(list_trees(d.path()).unwrap(), vec![DEFAULT_TREE]);
    }
}
