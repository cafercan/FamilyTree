//! Ortak tipler.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Sex {
    #[serde(rename = "E")]
    Male,
    #[serde(rename = "K")]
    Female,
}

impl Sex {
    pub fn from_tr(s: &str) -> Option<Sex> {
        match tr_fold(s).as_str() {
            "e" | "erkek" => Some(Sex::Male),
            "k" | "kadın" | "kadin" => Some(Sex::Female),
            _ => None,
        }
    }
}

/// e-Devlet soy ağacı tablosunun bir satırı. Boş alanlar `""`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Record {
    pub row_no: u32,
    pub sex: Option<Sex>,
    pub relation: String,
    pub given_name: String,
    pub surname: String,
    pub father_name: String,
    pub mother_name: String,
    pub birth_place: String,
    pub birth_date: String,
    pub province_district: String,
    pub neighborhood: String,
    pub register_no: String,
    pub marital_status: String,
    pub status: String,
    pub death_date: String,
    pub tc: Option<String>,
}

/// Türkçe kurallarıyla küçük harfe çevirir ve boşlukları sadeleştirir.
pub fn tr_fold(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for word in s.split_whitespace() {
        if !out.is_empty() {
            out.push(' ');
        }
        for c in word.chars() {
            match c {
                'İ' => out.push('i'),
                'I' => out.push('ı'),
                _ => out.extend(c.to_lowercase()),
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sex_from_tr_variants() {
        assert_eq!(Sex::from_tr("ERKEK"), Some(Sex::Male));
        assert_eq!(Sex::from_tr("E"), Some(Sex::Male));
        assert_eq!(Sex::from_tr("Kadın"), Some(Sex::Female));
        assert_eq!(Sex::from_tr("KADIN"), Some(Sex::Female));
        assert_eq!(Sex::from_tr("Kadin"), Some(Sex::Female));
        assert_eq!(Sex::from_tr("K"), Some(Sex::Female));
        assert_eq!(Sex::from_tr("?"), None);
    }

    #[test]
    fn tr_fold_turkish_i() {
        assert_eq!(tr_fold("  İSMAİL  ÇAKIR "), "ismail çakır");
        assert_eq!(tr_fold("IŞIK"), "ışık");
    }
}
