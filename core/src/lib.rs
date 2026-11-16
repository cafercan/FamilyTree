//! FamilyTree çekirdeği.

pub mod backup;
pub mod db;
pub mod family;
pub mod import;
pub mod kinship;
pub mod layout;
pub mod model;
pub mod pdf;
pub mod relation;
pub mod table;
pub mod trees;

pub use model::{tr_fold, Record, Sex};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("PDF okunamadı: {0}")]
    Pdf(String),
    #[error("{0}")]
    Layout(String),
    #[error("Veritabanı hatası: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("Kişi bulunamadı: {0}")]
    NotFound(i64),
    #[error("{0}")]
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, Error>;
