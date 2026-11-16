use familytree_core::pdf::extract;
use familytree_core::table::{anonymized_layout_dump, parse_records};
use familytree_core::{Error, Record};

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!("{}/../tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

fn expected(name: &str) -> Vec<Record> {
    serde_json::from_slice(&fixture(&format!("{name}.expected.json"))).unwrap()
}

fn assert_records(name: &str) {
    let text = extract(&fixture(&format!("{name}.pdf"))).unwrap();
    let got = parse_records(&text).unwrap();
    let want = expected(name);
    assert_eq!(got.len(), want.len(), "kayıt sayısı");
    for (g, w) in got.iter().zip(want.iter()) {
        assert_eq!(g, w, "satır {}", w.row_no);
    }
}

#[test]
fn parses_ali_fixture() {
    assert_records("ornek-ali");
}

#[test]
fn parses_kuzen_fixture() {
    assert_records("ornek-ayse-kuzen");
}

#[test]
fn anonymized_layout_dump_has_no_letters_from_data() {
    let text = extract(&fixture("ornek-ali.pdf")).unwrap();
    let dump = anonymized_layout_dump(&text);
    for secret in ["YILMAZ", "EMRE", "MEHMET", "ILGAZ", "Çankırı", "12/03/1985", "41-102-3", "Kendisi", "Babası", "Ölüm"] {
        assert!(!dump.contains(secret), "dökümde kişisel veri var: {secret}");
    }
    assert!(dump.contains("Yakınlık"));
    assert!(dump.contains("Sıra"));
    // Uzunluk ve genişlik de sızmamalı: harf/rakam dizileri tek karaktere iner, genişlik yok
    assert!(dump.contains("\"0/0/0\""), "tarih biçimi görünür kalmalı");
    for line in dump.lines().filter(|l| !l.contains("Sıra") && !l.contains("Tarihi") && !l.contains("Doğum")) {
        assert!(!line.contains('+'), "genişlik yazılmamalı: {line}");
        for token in line.split('"').skip(1).step_by(2) {
            assert!(!token.contains("xx") && !token.contains("00"), "uzunluk sızıyor: {line}");
        }
    }
}

#[test]
fn missing_header_is_layout_error() {
    let mut text = extract(&fixture("ornek-ali.pdf")).unwrap();
    text.words.retain(|w| !["Sıra", "Yakınlık", "Derecesi", "Adı", "Soyadı"].contains(&w.text.as_str()));
    match parse_records(&text) {
        Err(Error::Layout(m)) => assert!(m.starts_with("Sütun başlıkları bulunamadı"), "{m}"),
        other => panic!("beklenmeyen sonuç: {other:?}"),
    }
}

/// Başlığı tekrarlanmayan sayfa önceki sayfanın sütunlarıyla okunur, sessizce atlanmaz.
#[test]
fn page_without_header_uses_previous_columns() {
    let mut text = extract(&fixture("ornek-ali.pdf")).unwrap();
    text.words.retain(|w| !(w.page == 2 && ["Sıra", "Yakınlık"].contains(&w.text.as_str())));
    assert_eq!(parse_records(&text).unwrap(), expected("ornek-ali"));
}

/// Ortadan bir satır okunamazsa (sıra no boşluğu) eksik ağaçla devam edilmez.
#[test]
fn missing_row_is_layout_error() {
    let mut text = extract(&fixture("ornek-ali.pdf")).unwrap();
    // 5 numaralı satırın sıra numarasını sil
    let five = text.words.iter().position(|w| w.text == "5" && w.x < 60.0).unwrap();
    text.words.remove(five);
    match parse_records(&text) {
        Err(Error::Layout(m)) => assert!(m.contains("eksik sıra no: 5"), "{m}"),
        other => panic!("beklenmeyen sonuç: {:?}", other.map(|r| r.len())),
    }
}
