//! PDF'ten konumlu kelimeler.

use crate::{Error, Result};
use pdf_extract::{ColorSpace, MediaBox, OutputDev, OutputError, Path, PathOp, Transform};

/// Sayfadaki bir kelime; koordinatlar punto, sol üst köşe orijin.
#[derive(Debug, Clone, PartialEq)]
pub struct Word {
    pub page: u32,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    /// Yazı yüksekliği (punto).
    pub size: f32,
    pub text: String,
}

/// Dikey çizgi (tablo kenarlığı); koordinatlar punto, sol üst köşe orijin, y0 < y1.
#[derive(Debug, Clone, PartialEq)]
pub struct VRule {
    pub page: u32,
    pub x: f32,
    pub y0: f32,
    pub y1: f32,
}

/// Bir PDF'in konumlu metni ve dikey çizgileri.
#[derive(Debug, Clone, Default)]
pub struct PdfText {
    pub words: Vec<Word>,
    pub rules: Vec<VRule>,
}

#[derive(Default)]
struct Collector {
    page: u32,
    page_h: f64,
    words: Vec<Word>,
    rules: Vec<VRule>,
    cur: Option<Word>,
    last_end: f64,
}

impl Collector {
    fn flush(&mut self) {
        if let Some(w) = self.cur.take() {
            if !w.text.is_empty() {
                self.words.push(w);
            }
        }
    }
}

impl Collector {
    fn to_page(&self, ctm: &Transform, x: f64, y: f64) -> (f64, f64) {
        let px = x * ctm.m11 + y * ctm.m21 + ctm.m31;
        let py = x * ctm.m12 + y * ctm.m22 + ctm.m32;
        (px, self.page_h - py)
    }

    /// İnce ve uzun dikey bir şekli çizgi olarak kaydeder.
    fn add_if_vertical(&mut self, x0: f64, y0: f64, x1: f64, y1: f64) {
        let (w, h) = ((x1 - x0).abs(), (y1 - y0).abs());
        if w <= 2.0 && h >= 4.0 {
            self.rules.push(VRule {
                page: self.page,
                x: ((x0 + x1) / 2.0) as f32,
                y0: y0.min(y1) as f32,
                y1: y0.max(y1) as f32,
            });
        }
    }

    /// Yolun alt yollarının sayfa koordinatlarındaki sınır kutuları ve doğru parçaları.
    fn shapes(&self, ctm: &Transform, path: &Path) -> (Vec<[f64; 4]>, Vec<[f64; 4]>) {
        let mut boxes = Vec::new();
        let mut segs = Vec::new();
        let mut cur: Option<[f64; 4]> = None;
        let mut last = (0.0, 0.0);
        let grow = |b: &mut Option<[f64; 4]>, (x, y): (f64, f64)| match b {
            Some(r) => {
                r[0] = r[0].min(x);
                r[1] = r[1].min(y);
                r[2] = r[2].max(x);
                r[3] = r[3].max(y);
            }
            None => *b = Some([x, y, x, y]),
        };
        for op in &path.ops {
            match *op {
                PathOp::MoveTo(x, y) => {
                    if let Some(b) = cur.take() {
                        boxes.push(b);
                    }
                    last = self.to_page(ctm, x, y);
                    grow(&mut cur, last);
                }
                PathOp::LineTo(x, y) => {
                    let p = self.to_page(ctm, x, y);
                    segs.push([last.0, last.1, p.0, p.1]);
                    last = p;
                    grow(&mut cur, p);
                }
                PathOp::CurveTo(_, _, _, _, x, y) => {
                    last = self.to_page(ctm, x, y);
                    grow(&mut cur, last);
                }
                PathOp::Rect(x, y, w, h) => {
                    let a = self.to_page(ctm, x, y);
                    let b = self.to_page(ctm, x + w, y + h);
                    boxes.push([a.0.min(b.0), a.1.min(b.1), a.0.max(b.0), a.1.max(b.1)]);
                }
                PathOp::Close => {}
            }
        }
        if let Some(b) = cur {
            boxes.push(b);
        }
        (boxes, segs)
    }
}

impl OutputDev for Collector {
    fn stroke(
        &mut self,
        ctm: &Transform,
        _colorspace: &ColorSpace,
        _color: &[f64],
        path: &Path,
    ) -> std::result::Result<(), OutputError> {
        let (boxes, segs) = self.shapes(ctm, path);
        for s in segs {
            self.add_if_vertical(s[0], s[1], s[2], s[3]);
        }
        // Kenarlığı dikdörtgen olarak çizilmiş hücreler: sol ve sağ kenar
        for b in boxes {
            if b[2] - b[0] > 2.0 && b[3] - b[1] >= 4.0 {
                self.add_if_vertical(b[0], b[1], b[0], b[3]);
                self.add_if_vertical(b[2], b[1], b[2], b[3]);
            }
        }
        Ok(())
    }

    fn fill(
        &mut self,
        ctm: &Transform,
        _colorspace: &ColorSpace,
        _color: &[f64],
        path: &Path,
    ) -> std::result::Result<(), OutputError> {
        let (boxes, _) = self.shapes(ctm, path);
        for b in boxes {
            self.add_if_vertical(b[0], b[1], b[2], b[3]);
        }
        Ok(())
    }

    fn begin_page(
        &mut self,
        page_num: u32,
        media_box: &MediaBox,
        _art_box: Option<(f64, f64, f64, f64)>,
    ) -> std::result::Result<(), OutputError> {
        self.flush();
        self.page = page_num;
        self.page_h = media_box.ury - media_box.lly;
        Ok(())
    }

    fn end_page(&mut self) -> std::result::Result<(), OutputError> {
        self.flush();
        Ok(())
    }

    fn output_character(
        &mut self,
        trm: &Transform,
        width: f64,
        _spacing: f64,
        font_size: f64,
        ch: &str,
    ) -> std::result::Result<(), OutputError> {
        // pdf-extract'in PlainTextOutput'u ile aynı ölçek: (fs, fs) vektörünün dönüşümü
        let vx = font_size * (trm.m11 + trm.m21);
        let vy = font_size * (trm.m12 + trm.m22);
        let size = (vx * vy).abs().sqrt();
        let x = trm.m31;
        let y = self.page_h - trm.m32;
        // Bazı PDF'lerde (e-Devlet çıktısındaki rakamlar gibi) glif genişliği 0 gelir;
        // o zaman ortalama bir rakam genişliği varsay, yoksa her rakam ayrı kelime olur.
        let advance = if width > 0.0 { width * size } else { 0.6 * size };
        let end = x + advance;
        if ch.chars().all(char::is_whitespace) {
            self.flush();
            self.last_end = end;
            return Ok(());
        }
        let continues = match &self.cur {
            Some(w) => {
                (w.y as f64 - y).abs() < size * 0.5
                    && x <= self.last_end + size * 0.15
                    && x >= self.last_end - size
            }
            None => false,
        };
        if !continues {
            self.flush();
            self.cur = Some(Word {
                page: self.page,
                x: x as f32,
                y: y as f32,
                w: 0.0,
                size: size as f32,
                text: String::new(),
            });
        }
        let w = self.cur.as_mut().expect("kelime");
        w.text.push_str(ch);
        w.w = (end - w.x as f64) as f32;
        self.last_end = end;
        Ok(())
    }

    fn begin_word(&mut self) -> std::result::Result<(), OutputError> {
        Ok(())
    }
    fn end_word(&mut self) -> std::result::Result<(), OutputError> {
        Ok(())
    }
    fn end_line(&mut self) -> std::result::Result<(), OutputError> {
        Ok(())
    }
}

/// PDF'teki tüm kelimeleri ve dikey çizgileri sayfa sırasıyla, konumlarıyla döndürür.
pub fn extract(pdf_bytes: &[u8]) -> Result<PdfText> {
    let doc = pdf_extract::Document::load_mem(pdf_bytes).map_err(|e| Error::Pdf(e.to_string()))?;
    let mut c = Collector::default();
    // pdf-extract bazı bozuk PDF'lerde panik atabiliyor: arayüzü çökertmesin
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        pdf_extract::output_doc(&doc, &mut c)
    }))
    .map_err(|_| Error::Pdf("PDF içeriği çözümlenemedi".into()))?
    .map_err(|e| Error::Pdf(format!("{e:?}")))?;
    c.flush();
    Ok(PdfText { words: c.words, rules: c.rules })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(c: &mut Collector, chars: &[(f64, &str, f64)]) {
        c.page_h = 600.0;
        for (x, ch, width) in chars {
            let trm = Transform::row_major(1.0, 0.0, 0.0, 1.0, *x, 500.0);
            c.output_character(&trm, *width, 0.0, 9.0, ch).unwrap();
        }
        c.flush();
    }

    /// Bazı e-Devlet PDF'lerinde rakamların genişliği 0 gelir: "12/03/1985" tek kelime kalmalı
    #[test]
    fn zero_width_digits_stay_one_word() {
        let mut c = Collector::default();
        feed(&mut c, &[(100.0, "1", 0.0), (105.0, "2", 0.0), (110.0, "/", 0.33), (113.0, "0", 0.0), (118.0, "3", 0.0)]);
        let texts: Vec<&str> = c.words.iter().map(|w| w.text.as_str()).collect();
        assert_eq!(texts, vec!["12/03"]);
        // Gerçek bir boşluk yine ayırır
        let mut c = Collector::default();
        feed(&mut c, &[(100.0, "1", 0.0), (130.0, "2", 0.0)]);
        assert_eq!(c.words.len(), 2);
    }

    #[test]
    fn rejects_non_pdf() {
        assert!(matches!(extract(b"bu bir pdf degil"), Err(Error::Pdf(_))));
    }
}
