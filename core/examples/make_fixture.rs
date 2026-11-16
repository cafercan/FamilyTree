//! Testler için e-Devlet "Alt-Üst Soy Bilgisi" düzeninde **uydurma** PDF'ler üretir.
//! İçindeki tüm isimler hayalidir.
//!
//! cargo run -p familytree-core --example make_fixture -- /usr/share/fonts/truetype/dejavu/DejaVuSans.ttf tests/fixtures

use familytree_core::{Record, Sex};
use printpdf::{
    Mm, Op, ParsedFont, PdfDocument, PdfFontHandle, PdfPage, PdfSaveOptions, Point, Pt, TextItem,
};

/// Sütun başlıkları ve sütun kenarları (mm), gerçek e-Devlet çıktısındaki sırayla.
/// Başlıklar sütunda yatay ve dikey ortalıdır; uzun başlıklar iki satıra bölünür;
/// "Birey Sıra No" içinde ikinci bir "Sıra" geçer.
const COLUMNS: &[(&str, f32, f32)] = &[
    ("Sıra", 4.0, 14.0),
    ("C", 14.0, 20.0),
    ("Yakınlık Derecesi", 20.0, 56.0),
    ("Adı", 56.0, 86.0),
    ("Soyadı", 86.0, 108.0),
    ("Baba Adı", 108.0, 134.0),
    ("Ana Adı", 134.0, 156.0),
    ("Doğum Yeri ve\nTarihi", 156.0, 178.0),
    ("İl-İlçe-Mahalle/Köy", 178.0, 226.0),
    ("Cilt-Hane-\nBirey Sıra No", 226.0, 246.0),
    ("Medeni\nHali", 246.0, 260.0),
    ("Durumu", 260.0, 293.0),
];

/// Veri hücrelerinin yerleşimi: iki örnek farklı biçimleri sınar.
#[derive(Clone, Copy)]
struct Style {
    borders: bool,
    center_data: bool,
    middle_data: bool,
}

const PAGE_W: f32 = 297.0;
const PAGE_H: f32 = 210.0;
const FONT_PT: f32 = 7.0;
/// Gerçek e-Devlet çıktısında hücre içi satır aralığı yazı boyunun ~1,5 katı
const LINE_MM: f32 = 3.7;

/// Bir hücrenin PDF'te görünen hali; `\n` satır kaydırmasıdır.
struct Row {
    cells: [String; 12],
    record: Record,
}

/// "Durumu" sütunu: e-Devlet'te "Ölüm" + tarih, "Ölümün Tespiti" + "-" görülür.
#[derive(Clone, Copy)]
enum Durum {
    Sag,
    Olum(&'static str),
    Tespit,
}

/// Bir kişinin uydurma kimlik bilgileri (yakınlık ve sıra hariç).
#[derive(Clone, Copy)]
struct P {
    sex: Sex,
    given: &'static str,
    /// Eski kayıtlarda soyadı yoktur: "-"
    surname: &'static str,
    father: &'static str,
    mother: &'static str,
    place: &'static str,
    date: &'static str,
    il: &'static str,
    ilce: &'static str,
    mahalle: &'static str,
    reg: &'static str,
    medeni: &'static str,
    durum: Durum,
}

/// Yakınlık ifadesini tarayıcının yaptığı gibi satır başına iki kelimeyle kaydırır.
fn wrap_relation(s: &str) -> String {
    let words: Vec<&str> = s.split(' ').collect();
    words.chunks(2).map(|c| c.join(" ")).collect::<Vec<_>>().join("\n")
}

fn dash_to_empty(s: &str) -> String {
    if s == "-" { String::new() } else { s.to_string() }
}

fn row(no: u32, relation: &str, p: P) -> Row {
    let (status, death_date, status_cell) = match p.durum {
        Durum::Sag => ("Sağ".to_string(), String::new(), "Sağ".to_string()),
        Durum::Olum(d) => ("Ölüm".to_string(), d.to_string(), format!("Ölüm\n{d}")),
        Durum::Tespit => ("Ölümün Tespiti".to_string(), String::new(), "Ölümün Tespiti\n-".to_string()),
    };
    let record = Record {
        row_no: no,
        sex: Some(p.sex),
        relation: relation.into(),
        given_name: p.given.into(),
        surname: dash_to_empty(p.surname),
        father_name: p.father.into(),
        mother_name: p.mother.into(),
        birth_place: p.place.into(),
        birth_date: p.date.into(),
        province_district: format!("{}/{}/{}", p.il, p.ilce, p.mahalle),
        neighborhood: String::new(),
        register_no: p.reg.into(),
        marital_status: p.medeni.into(),
        status,
        death_date,
        tc: None,
    };
    let sex_cell = match p.sex {
        Sex::Male => "E",
        Sex::Female => "K",
    };
    Row {
        cells: [
            no.to_string(),
            sex_cell.into(),
            wrap_relation(relation),
            p.given.into(),
            p.surname.into(),
            p.father.into(),
            p.mother.into(),
            format!("{}\n{}", p.place, p.date),
            format!("{}/\n{}/\n{}", p.il, p.ilce, p.mahalle),
            p.reg.into(),
            p.medeni.into(),
            status_cell,
        ],
        record,
    }
}

use Sex::{Female as K, Male as E};

// Emre YILMAZ'ın anne ve babası birinci dereceden kuzendir: Kadir (dede) ile Elif
// (anneanne) kardeştir, ortak ataları Rıza ve Hatice'dir. Ayşe DEMİR, Emre'nin
// annesinin kız kardeşinin kızıdır (birinci dereceden kuzen).
const RIZA: P = P { sex: E, given: "RIZA", surname: "-", father: "ÖMER", mother: "ŞERİFE", place: "ILGAZ", date: "01/07/1895", il: "Çankırı", ilce: "Ilgaz", mahalle: "KUZUÖREN KÖYÜ", reg: "3-17-2", medeni: "Evli", durum: Durum::Olum("14/02/1961") };
const HATICE: P = P { sex: K, given: "HATİCE", surname: "-", father: "SALİH", mother: "HAVVA", place: "ILGAZ", date: "01/07/1899", il: "Çankırı", ilce: "Ilgaz", mahalle: "KUZUÖREN KÖYÜ", reg: "3-17-3", medeni: "Evli", durum: Durum::Tespit };
const OSMAN: P = P { sex: E, given: "OSMAN", surname: "KAYA", father: "İBRAHİM", mother: "AYŞE", place: "TOSYA", date: "18/03/1930", il: "Kastamonu", ilce: "Tosya", mahalle: "YENİ MAHALLESİ", reg: "8-41-1", medeni: "Evli", durum: Durum::Olum("02/12/1999") };
const ELIF: P = P { sex: K, given: "ELİF", surname: "KAYA", father: "RIZA", mother: "HATİCE", place: "ILGAZ", date: "22/08/1934", il: "Kastamonu", ilce: "Tosya", mahalle: "YENİ MAHALLESİ", reg: "8-41-2", medeni: "Dul", durum: Durum::Olum("11/01/2010") };
const IBRAHIM: P = P { sex: E, given: "HACI İBRAHİM", surname: "-", father: "MUSA", mother: "SAFİYE", place: "TOSYA", date: "01/07/1901", il: "Kastamonu", ilce: "Tosya", mahalle: "YENİ MAHALLESİ", reg: "8-12-1", medeni: "Evli", durum: Durum::Tespit };
const AYSE_KAYA: P = P { sex: K, given: "AYŞE", surname: "-", father: "YAKUP", mother: "RUKİYE", place: "TOSYA", date: "12/11/1905", il: "Kastamonu", ilce: "Tosya", mahalle: "YENİ MAHALLESİ", reg: "8-12-2", medeni: "Evli", durum: Durum::Olum("03/03/1975") };

fn ali_rows() -> Vec<Row> {
    // e-Devlet en eski kuşaktan başlayarak sıralar
    vec![
        row(1, "Babasının Babasının Babası", RIZA),
        row(2, "Babasının Babasının Annesi", HATICE),
        row(3, "Babasının Annesinin Babası", P { sex: E, given: "SADIK", surname: "ŞAHİN", father: "HÜSEYİN", mother: "FADİME", place: "İSKİLİP", date: "09/09/1902", il: "Çorum", ilce: "İskilip", mahalle: "ÇARŞI MAHALLESİ", reg: "5-9-1", medeni: "Evli", durum: Durum::Olum("21/04/1980") }),
        row(4, "Annesinin Babasının Babası", IBRAHIM),
        row(5, "Annesinin Babasının Annesi", AYSE_KAYA),
        row(6, "Annesinin Annesinin Babası", RIZA),
        row(7, "Annesinin Annesinin Annesi", HATICE),
        row(8, "Babasının Babası", P { sex: E, given: "KADİR", surname: "YILMAZ", father: "RIZA", mother: "HATİCE", place: "ILGAZ", date: "15/01/1930", il: "Çankırı", ilce: "Ilgaz", mahalle: "KUZUÖREN KÖYÜ", reg: "3-40-1", medeni: "Evli", durum: Durum::Olum("08/08/2001") }),
        row(9, "Babasının Annesi", P { sex: K, given: "NERİMAN", surname: "YILMAZ", father: "SADIK", mother: "EMİNE", place: "İSKİLİP", date: "27/02/1933", il: "Çankırı", ilce: "Ilgaz", mahalle: "KUZUÖREN KÖYÜ", reg: "3-40-2", medeni: "Dul", durum: Durum::Sag }),
        row(10, "Annesinin Babası", OSMAN),
        row(11, "Annesinin Annesi", ELIF),
        row(12, "Babası", P { sex: E, given: "MEHMET", surname: "YILMAZ", father: "KADİR", mother: "NERİMAN", place: "ILGAZ", date: "04/06/1958", il: "Ankara", ilce: "Çankaya", mahalle: "BAHÇELİ MAHALLESİ", reg: "41-102-1", medeni: "Evli", durum: Durum::Sag }),
        row(13, "Annesi", P { sex: K, given: "NURTEN", surname: "YILMAZ", father: "OSMAN", mother: "ELİF", place: "TOSYA", date: "19/09/1961", il: "Ankara", ilce: "Çankaya", mahalle: "BAHÇELİ MAHALLESİ", reg: "41-102-2", medeni: "Evli", durum: Durum::Sag }),
        row(14, "Kendisi", P { sex: E, given: "EMRE", surname: "YILMAZ", father: "MEHMET", mother: "NURTEN", place: "ÇANKAYA", date: "12/03/1985", il: "Ankara", ilce: "Çankaya", mahalle: "BAHÇELİ MAHALLESİ", reg: "41-102-3", medeni: "Evli", durum: Durum::Sag }),
        row(15, "Kızı", P { sex: K, given: "DEFNE", surname: "YILMAZ", father: "EMRE", mother: "SELİN", place: "ÇANKAYA", date: "07/10/2015", il: "Ankara", ilce: "Çankaya", mahalle: "BAHÇELİ MAHALLESİ", reg: "41-102-4", medeni: "Bekâr", durum: Durum::Sag }),
    ]
}

fn ayse_rows() -> Vec<Row> {
    vec![
        row(1, "Annesinin Babasının Babası", IBRAHIM),
        row(2, "Annesinin Babasının Annesi", AYSE_KAYA),
        row(3, "Annesinin Annesinin Babası", RIZA),
        row(4, "Annesinin Annesinin Annesi", HATICE),
        row(5, "Babasının Babası", P { sex: E, given: "NECATİ", surname: "DEMİR", father: "RECEP", mother: "NAZLI", place: "BOYABAT", date: "02/02/1931", il: "Sinop", ilce: "Boyabat", mahalle: "KÖPRÜBAŞI KÖYÜ", reg: "11-6-1", medeni: "Evli", durum: Durum::Olum("17/05/1995") }),
        row(6, "Babasının Annesi", P { sex: K, given: "SULTAN", surname: "DEMİR", father: "HALİL", mother: "LEMAN", place: "BOYABAT", date: "06/06/1936", il: "Sinop", ilce: "Boyabat", mahalle: "KÖPRÜBAŞI KÖYÜ", reg: "11-6-2", medeni: "Evli", durum: Durum::Olum("01/09/2008") }),
        row(7, "Annesinin Babası", OSMAN),
        row(8, "Annesinin Annesi", ELIF),
        row(9, "Babası", P { sex: E, given: "KEMAL", surname: "DEMİR", father: "NECATİ", mother: "SULTAN", place: "BOYABAT", date: "30/01/1960", il: "İstanbul", ilce: "Kadıköy", mahalle: "MODA MAHALLESİ", reg: "77-310-1", medeni: "Evli", durum: Durum::Sag }),
        row(10, "Annesi", P { sex: K, given: "GÜL", surname: "DEMİR", father: "OSMAN", mother: "ELİF", place: "TOSYA", date: "14/07/1964", il: "İstanbul", ilce: "Kadıköy", mahalle: "MODA MAHALLESİ", reg: "77-310-3", medeni: "Evli", durum: Durum::Sag }),
        row(11, "Kendisi", P { sex: K, given: "AYŞE", surname: "DEMİR", father: "KEMAL", mother: "GÜL", place: "KADIKÖY", date: "23/12/1988", il: "İstanbul", ilce: "Kadıköy", mahalle: "MODA MAHALLESİ", reg: "77-310-2", medeni: "Bekâr", durum: Durum::Sag }),
    ]
}

fn text_at(ops: &mut Vec<Op>, font: &PdfFontHandle, x: f32, y_from_top: f32, s: &str) {
    ops.push(Op::StartTextSection);
    ops.push(Op::SetFont { font: font.clone(), size: Pt(FONT_PT) });
    ops.push(Op::SetTextCursor { pos: Point::new(Mm(x), Mm(PAGE_H - y_from_top)) });
    ops.push(Op::ShowText { items: vec![TextItem::Text(s.to_string())] });
    ops.push(Op::EndTextSection);
}

/// DejaVuSans için kaba genişlik tahmini (mm); yalnızca ortalamak için.
fn text_w(s: &str) -> f32 {
    s.chars().count() as f32 * FONT_PT * 0.62 * 0.3528
}

/// Hücreyi çizer; `rows` hücrenin ait olduğu satırın kaç metin satırı yüksekliğinde olduğu.
#[allow(clippy::too_many_arguments)]
fn cell(ops: &mut Vec<Op>, font: &PdfFontHandle, left: f32, right: f32, top: f32, rows: usize, s: &str, center: bool, middle: bool) {
    let lines: Vec<&str> = s.split('\n').collect();
    let off = if middle { (rows - lines.len()) as f32 * LINE_MM / 2.0 } else { 0.0 };
    for (i, line) in lines.iter().enumerate() {
        let x = if center { (left + right - text_w(line)) / 2.0 } else { left + 1.5 };
        text_at(ops, font, x, top + off + i as f32 * LINE_MM, line);
    }
}

fn lines_in(s: &str) -> usize {
    s.split('\n').count()
}

/// İnce dolu dikdörtgen: tarayıcıların tablo kenarlığı çizme biçimi.
fn rule(ops: &mut Vec<Op>, x: f32, y_top: f32, w: f32, h: f32) {
    ops.push(Op::DrawRectangle {
        rectangle: printpdf::Rect {
            mode: Some(printpdf::PaintMode::Fill),
            ..printpdf::Rect::from_xywh(Mm(x).into(), Mm(PAGE_H - y_top - h).into(), Mm(w).into(), Mm(h).into())
        },
    });
}

fn build(font_bytes: &[u8], rows: &[Row], per_page: usize, style: Style) -> Vec<u8> {
    let mut doc = PdfDocument::new("Alt-Üst Soy Bilgisi");
    let parsed = ParsedFont::from_bytes(font_bytes, 0, &mut Vec::new()).expect("font");
    let font = PdfFontHandle::External(doc.add_font(&parsed));
    let chunks: Vec<&[Row]> = rows.chunks(per_page).collect();
    let total = chunks.len();
    let mut pages = Vec::new();
    for (pi, chunk) in chunks.iter().enumerate() {
        let mut ops = Vec::new();
        // Tarayıcının yazdırma üst bilgisi ve sayfa başlığı (tablonun üstünde: yok sayılmalı)
        text_at(&mut ops, &font, 6.0, 8.0, "16.11.2026 10:42");
        text_at(&mut ops, &font, 120.0, 8.0, "e-Devlet Kapısı");
        let mut ty = 16.0;
        for t in ["T.C.", "İÇİŞLERİ BAKANLIĞI", "NÜFUS VE VATANDAŞLIK İŞLERİ GENEL MÜDÜRLÜĞÜ", "ALT ÜST SOY BELGESİ"] {
            text_at(&mut ops, &font, (PAGE_W - text_w(t)) / 2.0, ty, t);
            ty += LINE_MM;
        }
        let table_top = ty + 1.0;
        let mut y = table_top + 2.0;
        let header_rows = COLUMNS.iter().map(|c| lines_in(c.0)).max().unwrap();
        for (title, l, r) in COLUMNS {
            cell(&mut ops, &font, *l, *r, y, header_rows, title, true, true);
        }
        y += header_rows as f32 * LINE_MM + 1.0;
        let mut row_tops = vec![y - 2.0];
        for r in chunk.iter() {
            y += 2.0;
            let n = r.cells.iter().map(|c| lines_in(c)).max().unwrap();
            for (i, (_, left, right)) in COLUMNS.iter().enumerate() {
                cell(&mut ops, &font, *left, *right, y, n, &r.cells[i], style.center_data, style.middle_data);
            }
            y += n as f32 * LINE_MM;
            row_tops.push(y - 1.0);
        }
        if style.borders {
            let bottom = y - 1.0;
            for (_, l, _) in COLUMNS {
                rule(&mut ops, *l, table_top, 0.2, bottom - table_top);
            }
            rule(&mut ops, COLUMNS.last().unwrap().2, table_top, 0.2, bottom - table_top);
            for t in std::iter::once(table_top).chain(row_tops) {
                rule(&mut ops, COLUMNS[0].1, t, COLUMNS.last().unwrap().2 - COLUMNS[0].1, 0.2);
            }
        }
        // Tarayıcının alt bilgisi: sol altta adres, sağ altta sayfa no (yok sayılmalı)
        text_at(&mut ops, &font, 6.0, PAGE_H - 6.0, "https://www.turkiye.gov.tr/nvi-alt-ust-soy-bilgisi-sorgulama");
        text_at(&mut ops, &font, PAGE_W - 16.0, PAGE_H - 6.0, &format!("{}/{}", pi + 1, total));
        pages.push(PdfPage::new(Mm(PAGE_W), Mm(PAGE_H), ops));
    }
    doc.with_pages(pages).save(&PdfSaveOptions::default(), &mut Vec::new())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let font_path = args.get(1).expect("font yolu");
    let out = std::path::PathBuf::from(args.get(2).expect("çıktı klasörü"));
    let font = std::fs::read(font_path).expect("font okunamadı");
    std::fs::create_dir_all(&out).unwrap();
    // Emre: gerçek e-Devlet çıktısı gibi kenarlıklı, veri yatay ve dikey ortalı.
    // Ayşe: kenarlıksız (sütun sınırı başlıklardan tahmin edilir), veri ortalı ve üste hizalı.
    let ali = Style { borders: true, center_data: true, middle_data: true };
    let ayse = Style { borders: false, center_data: true, middle_data: false };
    for (name, rows, per_page, style) in
        [("ornek-ali", ali_rows(), 9, ali), ("ornek-ayse-kuzen", ayse_rows(), 6, ayse)]
    {
        std::fs::write(out.join(format!("{name}.pdf")), build(&font, &rows, per_page, style)).unwrap();
        let records: Vec<&Record> = rows.iter().map(|r| &r.record).collect();
        std::fs::write(
            out.join(format!("{name}.expected.json")),
            serde_json::to_string_pretty(&records).unwrap() + "\n",
        )
        .unwrap();
    }
}
