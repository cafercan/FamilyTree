//! Gerçek bir e-Devlet PDF'inin **kişisel veri içermeyen** düzen dökümünü yazdırır:
//! başlık kelimeleri dışında her harf dizisi tek `x`, rakam dizisi tek `0` olur. Ayrıştırma sonucu
//! olarak yalnızca kayıt sayısı ve hangi alanların dolu olduğu (x/0 biçiminde) gösterilir.
//!
//! cargo run -p familytree-core --example layout_dump -- ~/soyagaci.pdf

use familytree_core::pdf::extract;
use familytree_core::table::{anonymized_layout_dump, parse_records};

fn mask(s: &str) -> String {
    familytree_core::table::anonymize(s)
}

/// Yakınlık ifadeleri kişisel veri değildir; yalnızca bilinen kelimelerden oluşuyorsa açık yaz.
fn relation(s: &str) -> String {
    const KNOWN: &[&str] = &[
        "kendisi", "babası", "babasının", "annesi", "annesinin", "oğlu", "oğlunun", "kızı",
        "kızının", "eşi", "eşinin", "kardeşi", "kardeşinin",
    ];
    if s.split_whitespace().all(|w| KNOWN.contains(&familytree_core::tr_fold(w).as_str())) {
        s.to_string()
    } else {
        mask(s)
    }
}

fn main() {
    let path = std::env::args().nth(1).expect("PDF yolu");
    let bytes = std::fs::read(&path).expect("PDF okunamadı");
    let text = match extract(&bytes) {
        Ok(t) => t,
        Err(e) => {
            println!("PDF okunamadı: {e}");
            return;
        }
    };
    print!("{}", anonymized_layout_dump(&text));
    match parse_records(&text) {
        Ok(records) => {
            println!("\n{} kayıt ayrıştırıldı. Alanların biçimi (x harf, 0 rakam):", records.len());
            for r in &records {
                println!(
                    "#{} c={:?} yakınlık=\"{}\" ad=\"{}\" soyad=\"{}\" baba=\"{}\" ana=\"{}\" d.yeri=\"{}\" d.tarihi=\"{}\" yer=\"{}\" kayıt=\"{}\" medeni=\"{}\" durum=\"{}\" ölüm=\"{}\"",
                    r.row_no,
                    r.sex.map(|_| "x"),
                    relation(&r.relation),
                    mask(&r.given_name),
                    mask(&r.surname),
                    mask(&r.father_name),
                    mask(&r.mother_name),
                    mask(&r.birth_place),
                    mask(&r.birth_date),
                    mask(&r.province_district),
                    mask(&r.register_no),
                    mask(&r.marital_status),
                    mask(&r.status),
                    mask(&r.death_date),
                );
            }
        }
        Err(e) => println!("\nAyrıştırma hatası: {e}"),
    }
}
