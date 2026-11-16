//! Konumlu kelimelerden e-Devlet soy ağacı tablosu satırları.
//!
//! e-Devlet çıktısında başlıklar ve hücreler sütun içinde yatayda ve dikeyde ortalıdır:
//! tek satırlık "Sıra" ile iki satırlık "Doğum Yeri ve / Tarihi" aynı y'de değildir. Bu
//! yüzden başlık, "Sıra" ve "Yakınlık" içeren satırın çevresindeki satırlardan oluşan bir
//! **bant** olarak okunur. Sütun sınırları tablonun dikey kenarlık çizgilerinden alınır;
//! çizgi yoksa başlık kelimelerinin kapladığı aralıklar arasındaki boşluğun ortası
//! kullanılır. Bir kelime, merkezinin düştüğü sütuna aittir. Boş hücrelerde "-" yazar.
//!
//! Her kaydın çapası "Sıra" sütunundaki sayıdır. Diğer sütunlardaki metin dikey olarak
//! bitişik satırlardan oluşan bloklara ayrılır ve her blok en yakın çapaya bağlanır;
//! böylece hücreler üste ya da ortaya hizalı olsa da, kaydırılmış olsa da doğru kayda düşer.

use crate::pdf::{PdfText, VRule, Word};
use crate::{tr_fold, Error, Record, Result, Sex};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Col {
    Sira,
    Sex,
    Relation,
    Given,
    Surname,
    Father,
    Mother,
    Birth,
    Location,
    Neighborhood,
    Register,
    Marital,
    Status,
    Tc,
}

/// Başlıkta geçebilecek kelimeler (düzen dökümünde okunur bırakılır).
const HEADER_VOCAB: &[&str] = &[
    "sıra", "c", "cinsiyet", "cinsiyeti", "yakınlık", "derecesi", "adı", "soyadı", "baba", "ana",
    "anne", "doğum", "yeri", "ve", "tarihi", "il-ilçe-mahalle/köy", "il-ilçe-", "il-ilçe", "il",
    "ilçe", "mahalle/köy", "cilt-hane-", "cilt-hane-birey", "birey", "no", "medeni", "hali",
    "durumu", "kimlik",
];

fn is_vocab(w: &Word) -> bool {
    HEADER_VOCAB.contains(&tr_fold(&w.text).as_str())
}

/// Bir sayfa satırı: aynı y'deki kelimeler, soldan sağa.
struct Line<'a> {
    y: f32,
    size: f32,
    words: Vec<&'a Word>,
}

fn lines_of<'a>(words: &[&'a Word]) -> Vec<Line<'a>> {
    let mut sorted: Vec<&Word> = words.to_vec();
    sorted.sort_by(|a, b| a.y.total_cmp(&b.y).then(a.x.total_cmp(&b.x)));
    let mut lines: Vec<Line> = Vec::new();
    for w in sorted {
        match lines.last_mut() {
            Some(l) if (w.y - l.y).abs() < 0.4 * w.size.max(l.size) => l.words.push(w),
            _ => lines.push(Line { y: w.y, size: w.size, words: vec![w] }),
        }
    }
    for l in &mut lines {
        l.words.sort_by(|a, b| a.x.total_cmp(&b.x));
    }
    lines
}

fn is_header_line(line: &Line) -> bool {
    let f: Vec<String> = line.words.iter().map(|w| tr_fold(&w.text)).collect();
    f.iter().any(|s| s == "sıra") && f.iter().any(|s| s == "yakınlık")
}

/// Bir sütunu başlatan anahtar kelime. `prev` aynı satırda soldaki kelimedir.
fn keyword(folded: &str, prev: &str) -> Option<Col> {
    match folded {
        "sıra" => Some(Col::Sira),
        "c" | "cinsiyet" | "cinsiyeti" => Some(Col::Sex),
        "yakınlık" => Some(Col::Relation),
        "adı" if prev != "baba" && prev != "ana" && prev != "anne" => Some(Col::Given),
        "soyadı" => Some(Col::Surname),
        "baba" => Some(Col::Father),
        "ana" | "anne" => Some(Col::Mother),
        "doğum" => Some(Col::Birth),
        "medeni" => Some(Col::Marital),
        "durumu" => Some(Col::Status),
        "kimlik" => Some(Col::Tc),
        s if s.starts_with("cilt") => Some(Col::Register),
        s if s.starts_with("il-") || s == "il" => Some(Col::Location),
        s if s.starts_with("mahalle") => Some(Col::Neighborhood),
        _ => None,
    }
}

/// Sütun ve sağ sınırı (son sütunun sınırı sonsuz).
#[derive(Clone)]
struct Columns {
    cols: Vec<(Col, f32)>,
}

impl Columns {
    /// Başlık bandındaki kelimelerden sütun aralıklarını ve sınırlarını çıkarır.
    fn from_header(band: &[&Line], rules: &[&VRule]) -> Columns {
        // (sütun, sol, sağ, anahtar kelimenin merkezi)
        let mut spans: Vec<(Col, f32, f32, f32)> = Vec::new();
        let mut rest: Vec<&Word> = Vec::new();
        // "Birey Sıra No" içinde ikinci bir "Sıra" var: asıl sütun en soldakidir
        let sira_x = band
            .iter()
            .flat_map(|l| l.words.iter())
            .filter(|w| tr_fold(&w.text) == "sıra")
            .map(|w| w.x)
            .fold(f32::INFINITY, f32::min);
        for line in band {
            let mut prev = String::new();
            for w in &line.words {
                let f = tr_fold(&w.text);
                match keyword(&f, &prev) {
                    Some(Col::Sira) if w.x > sira_x => rest.push(w),
                    Some(c) if !spans.iter().any(|s| s.0 == c) => {
                        spans.push((c, w.x, w.x + w.w, w.x + w.w / 2.0))
                    }
                    _ => rest.push(w),
                }
                prev = f;
            }
        }
        // Diğer başlık kelimeleri en yakın sütunun aralığını genişletir
        for w in rest {
            let c = w.x + w.w / 2.0;
            if let Some(s) = spans.iter_mut().min_by(|a, b| (a.3 - c).abs().total_cmp(&(b.3 - c).abs())) {
                s.1 = s.1.min(w.x);
                s.2 = s.2.max(w.x + w.w);
            }
        }
        spans.sort_by(|a, b| a.1.total_cmp(&b.1));
        // Başlık bandını kesen dikey çizgiler varsa sınırlar onlardır
        let top = band.first().map(|l| l.y).unwrap_or(0.0);
        let bottom = band.last().map(|l| l.y).unwrap_or(0.0);
        let mut xs: Vec<f32> = rules.iter().filter(|r| r.y0 <= bottom && r.y1 >= top - 2.0 * band[0].size).map(|r| r.x).collect();
        xs.sort_by(f32::total_cmp);
        xs.dedup_by(|a, b| (*a - *b).abs() < 1.0);
        let inner: Vec<f32> = xs
            .iter()
            .copied()
            .filter(|x| spans.first().is_some_and(|s| *x > s.3) && spans.last().is_some_and(|s| *x < s.3))
            .collect();
        if inner.len() + 1 >= spans.len() && spans.len() > 1 {
            // Her sütunun sağ sınırı: anahtar kelimesinin merkezinden sonraki ilk çizgi
            let cols = spans
                .iter()
                .map(|s| (s.0, inner.iter().copied().find(|x| *x > s.3).unwrap_or(f32::INFINITY)))
                .collect();
            return Columns { cols };
        }
        let mut cols = Vec::new();
        for i in 0..spans.len() {
            let right = match spans.get(i + 1) {
                Some(n) => (spans[i].2 + n.1) / 2.0,
                None => f32::INFINITY,
            };
            cols.push((spans[i].0, right));
        }
        Columns { cols }
    }

    fn has(&self, c: Col) -> bool {
        self.cols.iter().any(|(k, _)| *k == c)
    }

    fn of(&self, w: &Word) -> Col {
        let center = w.x + w.w / 2.0;
        self.cols.iter().find(|(_, right)| center < *right).map(|(c, _)| *c).unwrap_or(Col::Status)
    }
}

/// gg/aa/yyyy ya da gg.aa.yyyy
fn is_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && (b[2] == b'/' || b[2] == b'.')
        && b[5] == b[2]
        && b.iter().enumerate().all(|(i, c)| i == 2 || i == 5 || c.is_ascii_digit())
}

/// Metni son tarih parçası ve geri kalanı olarak böler; "-" boş sayılır.
fn split_date(s: &str) -> (String, String) {
    let parts: Vec<&str> = s.split_whitespace().filter(|p| *p != "-").collect();
    match parts.iter().rposition(|p| is_date(p)) {
        Some(i) => {
            let rest: Vec<&str> =
                parts.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, p)| *p).collect();
            (rest.join(" "), parts[i].to_string())
        }
        None => (parts.join(" "), String::new()),
    }
}

/// Hücre satırlarını birleştirir: "-" ya da "/" ile biten satır boşluksuz eklenir.
/// Yalnızca "-" içeren hücre boştur.
fn join_cell(lines: &[String]) -> String {
    let mut out = String::new();
    // Yalnızca "-" olan satır boş değerdir; sonraki satıra yapıştırılmaz
    for l in lines.iter().filter(|l| l.trim() != "-") {
        if !out.is_empty() && !out.ends_with('-') && !out.ends_with('/') {
            out.push(' ');
        }
        out.push_str(l);
    }
    if out == "-" {
        out.clear();
    }
    out
}

/// Bir sütunda dikey olarak bitişik satırlar (bir hücre).
struct Block {
    top: f32,
    bottom: f32,
    lines: Vec<String>,
}

/// `entries`: (y, yazı boyu, o satırın bu sütundaki metni), y'ye göre sıralı.
fn blocks_in_column(entries: &[(f32, f32, String)]) -> Vec<Block> {
    let mut blocks: Vec<Block> = Vec::new();
    for (y, size, text) in entries {
        match blocks.last_mut() {
            // Hücre içi satır aralığı ~1,3-1,5 × yazı boyu; satırlar arası (kenar boşluğu
            // ve çizgiyle) ~2 × yazı boyu
            Some(b) if y - b.bottom <= size * 1.75 => {
                b.bottom = *y;
                b.lines.push(text.clone());
            }
            _ => blocks.push(Block { top: *y, bottom: *y, lines: vec![text.clone()] }),
        }
    }
    blocks
}

/// Sayfadaki kayıtları `out`'a ekler; sayfada başlık yoksa `false`. Başlığı olmayan
/// sayfa (tablo başlığı her sayfada tekrarlanmamışsa) önceki sayfanın sütunlarıyla okunur.
fn parse_page(lines: &[Line], rules: &[&VRule], prev: &mut Option<(Columns, f32)>, out: &mut Vec<Record>) -> Result<bool> {
    let Some(hi) = lines.iter().position(is_header_line) else {
        return match prev.clone() {
            Some((cols, size)) => {
                read_rows(lines, &cols, size, out);
                Ok(false)
            }
            None => Ok(false),
        };
    };
    let size = lines[hi].size;
    // Başlık bandı: başlık satırına bitişik, yalnızca başlık kelimelerinden oluşan satırlar
    let mut lo = hi;
    while lo > 0
        && lines[lo].y - lines[lo - 1].y <= size * 1.6
        && lines[lo - 1].words.iter().all(|w| is_vocab(w))
    {
        lo -= 1;
    }
    let mut end = hi + 1;
    while end < lines.len()
        && lines[end].y - lines[end - 1].y <= size * 1.6
        && lines[end].words.iter().all(|w| is_vocab(w))
    {
        end += 1;
    }
    let band: Vec<&Line> = lines[lo..end].iter().collect();
    let cols = Columns::from_header(&band, rules);
    for need in [Col::Sira, Col::Relation, Col::Given, Col::Surname] {
        if !cols.has(need) {
            return Err(Error::Layout(format!("Sütun başlıkları bulunamadı: {need:?} sütunu yok")));
        }
    }
    read_rows(&lines[end..], &cols, size, out);
    *prev = Some((cols, size));
    Ok(true)
}

/// Başlığın altındaki satırlardan kayıtları okur.
fn read_rows(body: &[Line], cols: &Columns, size: f32, out: &mut Vec<Record>) {
    // Çapalar: Sıra sütunundaki tamsayılar
    let mut records: Vec<(f32, u32, Vec<(Col, Vec<String>)>)> = Vec::new();
    for l in body {
        for w in &l.words {
            if cols.of(w) == Col::Sira {
                if let Ok(n) = w.text.parse::<u32>() {
                    records.push((w.y, n, Vec::new()));
                }
            }
        }
    }
    if records.is_empty() {
        return;
    }
    for (col, _) in cols.cols.iter().filter(|(c, _)| *c != Col::Sira) {
        let mut entries: Vec<(f32, f32, String)> = Vec::new();
        for l in body {
            let text: Vec<&str> =
                l.words.iter().filter(|w| cols.of(w) == *col).map(|w| w.text.as_str()).collect();
            if !text.is_empty() {
                entries.push((l.y, l.size, text.join(" ")));
            }
        }
        for b in blocks_in_column(&entries) {
            // En yakın çapa; çok uzaktaysa (alt bilgi vb.) yok say
            let dist = |ay: f32| {
                if ay < b.top {
                    b.top - ay
                } else if ay > b.bottom {
                    ay - b.bottom
                } else {
                    0.0
                }
            };
            let best = records.iter_mut().min_by(|a, c| dist(a.0).total_cmp(&dist(c.0))).expect("çapa");
            if dist(best.0) <= size * 2.5 {
                best.2.push((*col, b.lines));
            }
        }
    }
    for (_, row_no, cells) in records {
        // Aynı kayda düşen tüm parçalar (y sırasıyla) tek hücre gibi birleşir
        let get = |c: Col| -> String {
            let lines: Vec<String> = cells.iter().filter(|(k, _)| *k == c).flat_map(|(_, l)| l.iter().cloned()).collect();
            join_cell(&lines)
        };
        let (birth_place, birth_date) = split_date(&get(Col::Birth));
        let (status, death_date) = split_date(&get(Col::Status));
        let tc = get(Col::Tc);
        out.push(Record {
            row_no,
            sex: Sex::from_tr(&get(Col::Sex)),
            relation: get(Col::Relation),
            given_name: get(Col::Given),
            surname: get(Col::Surname),
            father_name: get(Col::Father),
            mother_name: get(Col::Mother),
            birth_place,
            birth_date,
            province_district: get(Col::Location),
            neighborhood: get(Col::Neighborhood),
            register_no: get(Col::Register),
            marital_status: get(Col::Marital),
            status,
            death_date,
            tc: if tc.is_empty() { None } else { Some(tc) },
        });
    }

}

/// Sayfa numarası, o sayfanın kelimeleri ve çizgileri.
fn pages(text: &PdfText) -> Vec<(u32, Vec<&Word>, Vec<&VRule>)> {
    let mut nums: Vec<u32> = text.words.iter().map(|w| w.page).collect();
    nums.sort_unstable();
    nums.dedup();
    nums.into_iter()
        .map(|p| {
            (
                p,
                text.words.iter().filter(|w| w.page == p).collect(),
                text.rules.iter().filter(|r| r.page == p).collect(),
            )
        })
        .collect()
}

/// e-Devlet soy ağacı tablosunun tüm sayfalarındaki kayıtları döndürür.
pub fn parse_records(text: &PdfText) -> Result<Vec<Record>> {
    let mut out = Vec::new();
    let mut found_header = false;
    let mut prev = None;
    for (_, words, rules) in pages(text) {
        found_header |= parse_page(&lines_of(&words), &rules, &mut prev, &mut out)?;
    }
    if !found_header {
        return Err(Error::Layout(
            "Sütun başlıkları bulunamadı: bu PDF bir e-Devlet soy ağacı çıktısına benzemiyor".into(),
        ));
    }
    if out.is_empty() {
        return Err(Error::Layout("Tabloda hiç kayıt bulunamadı".into()));
    }
    // Güvenlik ağı: sıra numaraları 1..N kesintisiz değilse bir sayfa ya da satır
    // okunamamıştır; eksik aile ağacıyla sessizce devam etme
    let mut nums: Vec<u32> = out.iter().map(|r| r.row_no).collect();
    nums.sort_unstable();
    let max = nums.last().copied().unwrap_or(0);
    let missing: Vec<u32> = (1..=max).filter(|n| nums.binary_search(n).is_err()).collect();
    let repeated = nums.windows(2).any(|w| w[0] == w[1]);
    if !missing.is_empty() || repeated {
        let shown: Vec<String> = missing.iter().take(10).map(|n| n.to_string()).collect();
        return Err(Error::Layout(format!(
            "Tablo eksik okundu: {max} satır olmalıydı, {} satır okunabildi{}",
            out.len(),
            if shown.is_empty() { " (tekrarlanan sıra numarası var)".to_string() } else { format!(" (eksik sıra no: {})", shown.join(", ")) }
        )));
    }
    Ok(out)
}

/// Harf dizilerini tek `x`, rakam dizilerini tek `0` yapar; uzunluk bilgisi kalmaz.
/// Ayrıştırma için gereken biçim (`/`, `-`, `.`) korunur: "12/03/1985" → "0/0/0".
pub fn anonymize(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        let m = if c.is_alphabetic() { 'x' } else if c.is_numeric() { '0' } else { c };
        if (m == 'x' || m == '0') && out.ends_with(m) {
            continue;
        }
        out.push(m);
    }
    out
}

/// Kişisel veri içermeyen düzen dökümü: başlık kelimeleri dışında her harf dizisi tek `x`,
/// rakam dizisi tek `0` olur; kelime uzunluğu ve genişliği yazılmaz, konum 5 puntoya
/// yuvarlanır. Gerçek bir PDF ayrıştırılamadığında paylaşmak içindir.
pub fn anonymized_layout_dump(text: &PdfText) -> String {
    let mut out = String::from("FamilyTree düzen dökümü (harfler x, rakamlar 0)\n");
    for (p, words, rules) in pages(text) {
        let xs: Vec<String> = rules.iter().map(|r| format!("{:.0}[{:.0}-{:.0}]", r.x, r.y0, r.y1)).collect();
        out.push_str(&format!("p{p} dikey çizgiler: {}\n", xs.join(" ")));
        for l in lines_of(&words) {
            let header_like = l.words.iter().all(|w| is_vocab(w));
            let cells: Vec<String> = l
                .words
                .iter()
                .map(|w| {
                    if header_like {
                        format!("x{:.0}+{:.0}=\"{}\"", w.x, w.w, w.text)
                    } else {
                        let center = ((w.x + w.w / 2.0) / 5.0).round() * 5.0;
                        format!("~{center:.0}=\"{}\"", anonymize(&w.text))
                    }
                })
                .collect();
            out.push_str(&format!("p{} y{:.1} s{:.1}: {}\n", l.words[0].page, l.y, l.size, cells.join(" ")));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_date_takes_last_date() {
        assert_eq!(split_date("ILGAZ 01/07/1895"), ("ILGAZ".into(), "01/07/1895".into()));
        assert_eq!(split_date("ÇANKIRI 01.07.1895"), ("ÇANKIRI".into(), "01.07.1895".into()));
        assert_eq!(split_date("Ölümün Tespiti -"), ("Ölümün Tespiti".into(), String::new()));
        assert_eq!(split_date("Sağ"), ("Sağ".into(), String::new()));
    }

    #[test]
    fn join_cell_hyphen() {
        assert_eq!(join_cell(&["ANKARA-ÇANKAYA-".into(), "BAHÇELİ".into()]), "ANKARA-ÇANKAYA-BAHÇELİ");
        assert_eq!(join_cell(&["Babasının".into(), "Babası".into()]), "Babasının Babası");
        assert_eq!(join_cell(&["Çankırı/".into(), "Ilgaz/".into(), "KUZUÖREN KÖYÜ".into()]), "Çankırı/Ilgaz/KUZUÖREN KÖYÜ");
        assert_eq!(join_cell(&["-".into()]), "");
        // Doğum yeri bilinmiyor ("-") ama tarih var
        assert_eq!(split_date(&join_cell(&["-".into(), "01/07/1900".into()])), (String::new(), "01/07/1900".into()));
    }
}
