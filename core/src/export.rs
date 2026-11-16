//! Ağaç görünümünü dışa aktarma: JPEG görüntüyü tek sayfalık PDF'e sarar.

/// Görüntünün piksel boyutundan sayfa boyutu (punto): 1 px = 0,75 pt (96 dpi); çok büyük
/// ağaçlar PDF'in 14 400 pt sınırına sığacak şekilde küçültülür. Kenar boşluğu dahil.
pub fn page_size(css_w: f32, css_h: f32) -> (f32, f32) {
    const MARGIN: f32 = 24.0;
    const MAX: f32 = 14_400.0;
    let (mut w, mut h) = (css_w.max(1.0) * 0.75, css_h.max(1.0) * 0.75);
    let k = ((MAX - 2.0 * MARGIN) / w).min((MAX - 2.0 * MARGIN) / h).min(1.0);
    w *= k;
    h *= k;
    (w + 2.0 * MARGIN, h + 2.0 * MARGIN)
}

/// PDF metin dizesi: UTF-16BE, BOM ile, onaltılık
fn pdf_text(s: &str) -> String {
    let mut out = String::from("<FEFF");
    for u in s.encode_utf16() {
        out.push_str(&format!("{u:04X}"));
    }
    out.push('>');
    out
}

/// JPEG baytlarını (`DCTDecode`) tek sayfalık bir PDF'e yerleştirir. `px_w`/`px_h` JPEG'in
/// piksel boyutu, `css_w`/`css_h` ağacın ekrandaki boyutu (sayfa ölçüsü için).
pub fn jpeg_to_pdf(jpeg: &[u8], px_w: u32, px_h: u32, css_w: f32, css_h: f32, title: &str) -> Vec<u8> {
    const MARGIN: f32 = 24.0;
    let (pw, ph) = page_size(css_w, css_h);
    let (iw, ih) = (pw - 2.0 * MARGIN, ph - 2.0 * MARGIN);
    let content = format!("q {iw:.2} 0 0 {ih:.2} {MARGIN:.2} {MARGIN:.2} cm /Im0 Do Q");
    let mut out: Vec<u8> = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offsets = Vec::new();
    let mut obj = |out: &mut Vec<u8>, body: Vec<u8>| {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", offsets.len()).as_bytes());
        out.extend_from_slice(&body);
        out.extend_from_slice(b"\nendobj\n");
    };
    obj(&mut out, b"<< /Type /Catalog /Pages 2 0 R >>".to_vec());
    obj(&mut out, b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec());
    obj(
        &mut out,
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {pw:.2} {ph:.2}] /Resources << /XObject << /Im0 4 0 R >> >> /Contents 5 0 R >>"
        )
        .into_bytes(),
    );
    let mut img = format!(
        "<< /Type /XObject /Subtype /Image /Width {px_w} /Height {px_h} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode /Length {} >>\nstream\n",
        jpeg.len()
    )
    .into_bytes();
    img.extend_from_slice(jpeg);
    img.extend_from_slice(b"\nendstream");
    obj(&mut out, img);
    obj(&mut out, format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()).into_bytes());
    obj(&mut out, format!("<< /Title {} /Producer (FamilyTree) >>", pdf_text(title)).into_bytes());
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len() + 1).as_bytes());
    for o in &offsets {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!("trailer\n<< /Size {} /Root 1 0 R /Info 6 0 R >>\nstartxref\n{xref}\n%%EOF\n", offsets.len() + 1).as_bytes(),
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_size_scales_and_caps() {
        let (w, h) = page_size(1000.0, 500.0);
        assert!((w - (750.0 + 2.0 * 24.0)).abs() < 0.5 && (h - (375.0 + 2.0 * 24.0)).abs() < 0.5, "{w}x{h}");
        let (w, h) = page_size(40_000.0, 1_000.0);
        assert!(w <= 14_400.0 && h > 0.0);
        assert!(((w - 48.0) / (h - 48.0) - 40.0).abs() < 0.5, "en-boy oranı korunur");
    }

    #[test]
    fn pdf_is_valid_and_contains_image() {
        let fake_jpeg = b"\xFF\xD8\xFF\xE0sahte-jpeg-baytlari\xFF\xD9";
        let pdf = jpeg_to_pdf(fake_jpeg, 2000, 1000, 1000.0, 500.0, "Emre YILMAZ — ata ağacı");
        assert!(pdf.starts_with(b"%PDF-1.4"));
        let doc = pdf_extract::Document::load_mem(&pdf).expect("geçerli PDF");
        assert_eq!(doc.get_pages().len(), 1);
        let text = String::from_utf8_lossy(&pdf);
        assert!(text.contains("/DCTDecode") && text.contains("/Width 2000") && text.contains("/Height 1000"));
        let start = pdf.windows(fake_jpeg.len()).position(|w| w == fake_jpeg);
        assert!(start.is_some(), "JPEG baytları değişmeden gömülü");
    }
}
