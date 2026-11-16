# FamilyTree Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** e-Devlet soy ağacı PDF'lerini yerel SQLite veritabanında birleştiren, elle kişi eklemeye izin veren, akrabalık/akraba evliliği bulan ve ağaç diyagramı çizen Tauri 2 masaüstü uygulaması.

**Architecture:** Cargo workspace. `core/` saf Rust kütüphanesi (PDF okuma, ayrıştırma, veritabanı, birleştirme, akrabalık, ağaç yerleşimi) ve tüm testler burada. `src-tauri/` ince IPC katmanı, `ui/` paketleyicisiz HTML/JS/CSS; ağaç koordinatları Rust'ta hesaplanır, JS yalnızca SVG çizer.

**Tech Stack:** Rust 2021, Tauri 2.12.x, tauri-plugin-dialog 2.8.x, rusqlite 0.43 (bundled), pdf-extract 0.12, serde 1, thiserror 2, dirs 7; dev: tempfile 3, printpdf 0.12 (yalnızca fixture üretici örnek).

**Spec:** `docs/superpowers/specs/2026-11-16-familytree-design.md`

## Global Constraints

- Bağımlılıklar yazıldığı gündeki en güncel **kararlı** sürümler (Tauri 3 alfa DEĞİL; `tauri = "2"` hattının sonu). `Cargo.lock` commit edilir, `.github/dependabot.yml` cargo + npm + github-actions için haftalık.
- Kişisel veri git'e girmez: `.gitignore` → `*.pdf`, `!tests/fixtures/*.pdf`, `*.db`, `*.sqlite*`, `*.db-journal`, `familytree-yedek*.json`, `.claude/`, `target/`, `node_modules/`, `dist/`. Test/README/ekran görüntülerinde yalnızca uydurma isimler; TC no gerekiyorsa `1000000000x` biçimli sahte değerler.
- Commit mesajlarında Claude/AI/Anthropic geçmez, Co-Authored-By satırı eklenmez.
- Arayüz metinleri Türkçe. Uygulama ağ erişimi yok: CSP `default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src ipc: http://ipc.localhost`, harici CDN yok, http eklentisi yok.
- Veritabanı yolu: `dirs::data_dir()/FamilyTree/familytree.db`.
- Kimlik: identifier `com.cafercan.familytree`, productName `FamilyTree`, sürüm `0.1.0`.
- İçe aktarma elle düzenlenmiş alanları asla ezmez. TC'siz eşleşmeler kullanıcı onayı olmadan birleştirilmez.
- Yerel derleme önkoşulu: `build-essential pkg-config libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev libxdo-dev libssl-dev` (kullanıcı kurar).

## Review Focus

1. **Gerçek e-Devlet PDF düzeni fixture'dan farklı** (sütun sırası, satır kaydırma, sayfa başında tekrar eden başlık, alt bilgi/sayfa no). Beklenen: anlaşılır hata + kişisel veri içermeyen "düzen dökümü" (harfler `x`, rakamlar `0`) üretilebilmesi. → Task 2 `anonymized_layout_dump` testi.
2. **Aynı PDF iki kez içe aktarılır** veya iki kuzen PDF'i aynı ataları içerir. Beklenen: kopya kişi oluşmaz (öneriler önceden işaretli gelir), ilişkiler tekrar eklenmez. → Task 5 `reimport_same_pdf_creates_no_duplicates`.
3. **Zincirde boşluk** ("Babasının Annesinin Babası" var ama "Babasının Annesi" satırı yok). Beklenen: yer tutucu kişi oluşur, ağaç kopmaz. → Task 3 `gap_creates_placeholder`.
4. **Akraba evliliği nedeniyle aynı ata iki yoldan gelir** (pedigree collapse). Beklenen: akrabalık hesabı sonsuz döngüye girmez, en yakın ortak ataların hepsi listelenir; ağaçta tekrar eden düğüm işaretli. → Task 6 `double_cousins_two_common_ancestor_pairs`, Task 7 `repeated_ancestor_marked`.
5. **Elle düzeltilmiş alan + yeniden içe aktarma.** Beklenen: elle girilen değer kalır. → Task 5 `import_keeps_manual_fields`.

---

## Dosya Yapısı

```
FamilyTree/
  Cargo.toml                 workspace (members: core, src-tauri), release profili MDViewer ile aynı
  package.json               yalnızca @tauri-apps/cli (paketleme için)
  .gitignore  .github/dependabot.yml  .github/workflows/{test.yml,release.yml}
  README.md  LICENSE (MIT)
  core/
    Cargo.toml               package familytree-core, lib familytree_core
    src/lib.rs               mod bildirimleri + Error
    src/model.rs             Record, Person, Sex, Step ortak tipleri
    src/pdf.rs               PDF → konumlu metin parçaları (Glyph/Word)
    src/table.rs             konumlu metin → Record satırları (sütun algılama, satır birleştirme)
    src/relation.rs          "Babasının Annesinin…" → Vec<Step>
    src/db.rs                şema, migration, CRUD
    src/import.rs            önizleme, eşleşme önerileri, onaylı içe aktarma
    src/kinship.rs           ortak ata, Türkçe akrabalık adı, akraba evliliği raporu
    src/layout.rs            ata/soy/akrabalık-yolu ağaç koordinatları
    src/backup.rs            JSON yedek/geri yükleme
    examples/make_fixture.rs sahte e-Devlet PDF'i üretir (printpdf, DejaVuSans yolu argüman)
  tests/fixtures/            ornek-ali.pdf, ornek-ayse-kuzen.pdf (uydurma), *.expected.json
  src-tauri/
    Cargo.toml build.rs tauri.conf.json capabilities/default.json icons/
    src/main.rs              komutlar + durum (Mutex<Connection>)
  ui/
    index.html style.css app.js tree.js dialogs.js
```

---

### Task 1: Workspace iskeleti, ortak tipler, repo hijyeni

**Files:**
- Create: `Cargo.toml`, `core/Cargo.toml`, `core/src/lib.rs`, `core/src/model.rs`, `.gitignore`, `.github/dependabot.yml`, `LICENSE`, `README.md` (kısa, kişisel veri yok)
- Test: `core/src/model.rs` (unit)

**Interfaces:**
- Produces:
  - `pub enum Sex { Male, Female }` (`serde` "E"/"K"), `Sex::from_tr(&str) -> Option<Sex>` ("Erkek"/"E" → Male, "Kadın"/"Kadin"/"K" → Female, büyük/küçük harf ve Türkçe İ/ı duyarsız).
  - `pub struct Record { row_no: u32, sex: Option<Sex>, relation: String, given_name: String, surname: String, father_name: String, mother_name: String, birth_place: String, birth_date: String, province_district: String, neighborhood: String, register_no: String, marital_status: String, status: String, death_date: String, tc: Option<String> }` (Serialize/Deserialize/Clone/PartialEq/Debug, boş alan `""`).
  - `pub fn tr_fold(s: &str) -> String` — Türkçe büyük/küçük harf katlama + boşluk sadeleştirme (İ→i, I→ı, kenar boşlukları kırp, çoklu boşluk → tek). Tüm karşılaştırmalar bunu kullanır.
  - `pub enum Error` (thiserror): `Pdf(String)`, `Layout(String)`, `Db(rusqlite::Error)`, `NotFound(i64)`, `Invalid(String)`; `pub type Result<T>`.

- [ ] **Step 1:** Workspace + core crate oluştur; `.gitignore` Global Constraints'teki tam listeyle; dependabot MDViewer'daki ile aynı üç ekosistem. `git init`.
- [ ] **Step 2: Failing tests** `sex_from_tr_variants` (`"ERKEK"`, `"Kadın"`, `"K"` → doğru; `"?"` → None), `tr_fold_turkish_i` (`tr_fold("  İSMAİL  ÇAKIR ") == "ismail çakır"`, `tr_fold("IŞIK") == "ışık"`).
- [ ] **Step 3:** Run `cargo test -p familytree-core` → FAIL; uygula; tekrar → PASS.
- [ ] **Step 4:** Commit `chore: workspace iskeleti ve ortak tipler`.

### Task 2: Fixture PDF üretici + PDF'ten konumlu metin + tablo ayrıştırma

**Files:**
- Create: `core/examples/make_fixture.rs`, `core/src/pdf.rs`, `core/src/table.rs`, `tests/fixtures/ornek-ali.pdf`, `tests/fixtures/ornek-ali.expected.json`, `tests/fixtures/ornek-ayse-kuzen.pdf`, `tests/fixtures/ornek-ayse-kuzen.expected.json`
- Test: `core/tests/table.rs`

**Interfaces:**
- Produces:
  - `pub struct Word { page: u32, x: f32, y: f32, w: f32, text: String }`; `pub fn extract_words(pdf_bytes: &[u8]) -> Result<Vec<Word>>` (pdf-extract `OutputDev` ile karakter konumları; aynı satırdaki, aralarında boşluk genişliğinden az mesafe olan karakterler bir kelime).
  - `pub fn parse_records(words: &[Word]) -> Result<Vec<Record>>`.
  - `pub fn anonymized_layout_dump(words: &[Word]) -> String` — her satır `p{page} y{y:.0}: x{x:.0}="xxxx 00.00.0000" …`; başlık kelimeleri (bilinen sütun başlıkları) olduğu gibi, diğer tüm harfler `x`, rakamlar `0`.

Fixture içeriği (uydurma): sütunlar e-Devlet "Alt-Üst Soy Bilgisi" sırasıyla — `Sıra | Cinsiyet | Yakınlık Derecesi | Adı | Soyadı | Baba Adı | Ana Adı | Doğum Yeri ve Tarihi | İl-İlçe | Mahalle/Köy | Cilt-Hane-Birey No | Medeni Hali | Durumu`. `ornek-ali.pdf` kök "Ali YILMAZ"; 15 satır: kendisi, anne, baba, 4 büyükanne/büyükbaba, 7 büyük-büyük ebeveynden 6'sı (biri eksik: boşluk testi için "Babasının Annesinin Babası" var, "Babasının Annesinin Annesi" yok), 1 kızı. En az bir "Yakınlık" ve bir "Doğum Yeri ve Tarihi" hücresi iki satıra kaydırılmış; 2 sayfa, 2. sayfada başlık tekrar; sayfa alt bilgisi "Sayfa 1 / 2". Bir kişi "Ölü" + ölüm tarihi. `ornek-ayse-kuzen.pdf` kök "Ayşe DEMİR", Ali'nin anne tarafından birinci kuzeni: Ali'nin annesi ile Ayşe'nin annesi kardeş → anneanne/dedenin tüm satırları birebir aynı değerlerle.
Akraba evliliği için: Ali'nin babasının babasının babası ile annesinin annesinin babası **aynı kişi** (ad, baba/ana adı, doğum yeri-tarihi, cilt-hane-birey aynı) — Ali'nin anne-babası ikinci dereceden kuzen.

- [ ] **Step 1:** `make_fixture.rs`: `cargo run -p familytree-core --example make_fixture -- /usr/share/fonts/truetype/dejavu/DejaVuSans.ttf tests/fixtures` iki PDF'i ve `*.expected.json`'u (beklenen `Vec<Record>`) yazar. Çalıştır, PDF'leri commit'e ekle (test sırasında font gerekmez).
- [ ] **Step 2: Failing tests** `core/tests/table.rs`:
  - `parses_ali_fixture`: `parse_records(&extract_words(ornek-ali.pdf)?)? == expected.json` (15 kayıt, kaydırılmış hücreler tek boşlukla birleşik, başlık/alt bilgi satırı yok).
  - `parses_kuzen_fixture`: aynı kontrol.
  - `anonymized_layout_dump_has_no_letters_from_data`: döküm fixture'daki hiçbir ad/soyadı içermez (`!dump.contains("YILMAZ")` vb.) ama `"Yakınlık"` ve `"Cinsiyet"` başlıklarını içerir.
  - `missing_header_is_layout_error`: başlıksız kelime listesi → `Err(Error::Layout(_))`, mesaj "Sütun başlıkları bulunamadı" ile başlar.
- [ ] **Step 3:** Uygula. Algoritma: başlık satırını bilinen başlık kelimelerinin `tr_fold` eşleşmesiyle bul, sütun sınırlarını başlık x'lerinin orta noktalarından çıkar (her sayfada başlık yeniden aranır); başlığın altındaki kelimeleri y'ye göre (tolerans 0.4 × font yüksekliği) satırlara grupla; "Sıra" sütununda tamsayı olan satır yeni kayıt başlatır, olmayan satır önceki kaydın hücrelerine eklenir; "Sayfa n / m" ve son başlığın üstündeki her şey atlanır. "Doğum Yeri ve Tarihi" son `dd.mm.yyyy` parçası tarih, kalanı yer. "Durumu" içinde tarih varsa `death_date`. "T.C. Kimlik No" sütunu varsa `tc`.
- [ ] **Step 4:** `cargo test -p familytree-core --test table` → PASS. Commit `feat: e-Devlet soy ağacı PDF ayrıştırma`.

### Task 3: Yakınlık ifadesi ve kayıtlardan aile grafiği

**Files:**
- Create: `core/src/relation.rs`
- Test: unit `core/src/relation.rs`

**Interfaces:**
- Produces:
  - `pub enum Step { Father, Mother, Son, Daughter, Spouse, Sibling }`; `pub fn parse_relation(s: &str) -> Result<Vec<Step>>` ("Kendisi" → `[]`; parçalar `tr_fold` ile: babası/babasının, annesi/annesinin, oğlu/oğlunun, kızı/kızının, eşi/eşinin, kardeşi/kardeşinin; bilinmeyen parça → `Error::Invalid("Tanınmayan yakınlık: …")`).
  - `pub struct RecordGraph { nodes: Vec<GraphNode>, parent_edges: Vec<(usize /*child*/, usize /*parent*/, Sex /*parent role*/)>, spouse_edges: Vec<(usize, usize)> }`, `pub struct GraphNode { record: Option<Record> /*None = yer tutucu*/, path: Vec<Step>, sex: Option<Sex> }`; `pub fn build_graph(records: &[Record]) -> Result<RecordGraph>`.
- Kurallar: `P+[Father]` kişisi `P`'nin babası; `P+[Son|Daughter]` kişisinin ebeveyni `P` (rolü P'nin cinsiyeti, bilinmiyorsa P'nin yolunun son adımı Father→Male/Mother→Female); `P+[Spouse]` eş kenarı; `P+[Sibling]` P'nin ebeveynlerini paylaşır (ebeveynler yoksa yer tutucu ebeveyn oluşturma — yalnızca aynı yoldaki ebeveyn düğümleri varsa bağla). Önek düğüm yoksa cinsiyeti son adımdan çıkarılan yer tutucu düğüm oluştur (özyinelemeli).

- [ ] **Step 1: Failing tests** `parse_relation_examples` (`"Babasının Annesinin Babası"` → `[Father, Mother, Father]`, `"KENDİSİ"` → `[]`, `"Oğlunun Kızı"` → `[Son, Daughter]`, `"Komşusu"` → Err), `graph_links_parents` (Kendisi/Babası/Annesi/Babasının Babası → 3 ebeveyn kenarı, rolleri doğru), `gap_creates_placeholder` (yalnızca Kendisi + "Babasının Annesinin Babası" → 2 yer tutucu düğüm, zincir bağlı), `child_row_links_to_root` (Kendisi(K) + Oğlu → kenar (oğul, kök, Female)).
- [ ] **Step 2:** FAIL → uygula → PASS. Commit `feat: yakınlık ifadesinden aile grafiği`.

### Task 4: Veritabanı şeması ve CRUD

**Files:**
- Create: `core/src/db.rs`
- Test: unit `core/src/db.rs` (`Connection::open_in_memory`)

**Interfaces:**
- Produces:
  - Şema (`PRAGMA user_version = 1`, `foreign_keys = ON`):
    - `person(id INTEGER PRIMARY KEY, given_name TEXT NOT NULL DEFAULT '', surname, sex TEXT /*'E'|'K'|NULL*/, father_name, mother_name, birth_place, birth_date, death_date, alive INTEGER /*1|0|NULL*/, province_district, neighborhood, register_no, marital_status, tc TEXT UNIQUE, notes, placeholder INTEGER NOT NULL DEFAULT 0, manual_fields TEXT NOT NULL DEFAULT '[]', created_at TEXT NOT NULL)` (metin sütunları `NOT NULL DEFAULT ''`)
    - `parent_link(child_id, parent_id, role TEXT CHECK(role IN ('father','mother')), origin TEXT CHECK(origin IN ('pdf','manual')), PRIMARY KEY(child_id, role))`, FK'ler `ON DELETE CASCADE`
    - `spouse_link(a_id, b_id, PRIMARY KEY(a_id,b_id), CHECK(a_id < b_id))`
    - `source(id INTEGER PRIMARY KEY, file_name TEXT, imported_at TEXT, root_person_id INTEGER)`
    - `person_source(person_id, source_id, row_no, relation_text, PRIMARY KEY(person_id, source_id, row_no))`
    - `not_same(a_id, b_id, PRIMARY KEY(a_id,b_id), CHECK(a_id < b_id))`
  - `pub struct Person { id: i64, /* Record alanlarıyla aynı adlar */ sex: Option<Sex>, alive: Option<bool>, notes: String, placeholder: bool, manual_fields: Vec<String> }` (Serialize/Deserialize).
  - `pub struct PersonInput` = Person'ın id/placeholder/manual_fields hariç alanları.
  - `pub fn open(path: &Path) -> Result<Connection>`, `pub fn migrate(conn: &Connection) -> Result<()>`
  - `pub fn insert_person(conn, &PersonInput, manual: bool) -> Result<i64>` (manual=true ise boş olmayan tüm alanlar `manual_fields`'a)
  - `pub fn update_person_manual(conn, id, &PersonInput) -> Result<()>` (değişen alanlar `manual_fields`'a eklenir; `placeholder=0`)
  - `pub fn get_person(conn, id) -> Result<Person>`, `pub fn list_persons(conn) -> Result<Vec<Person>>`, `pub fn delete_person(conn, id) -> Result<()>`
  - `pub fn set_parent(conn, child, parent, role: Sex, origin: &str) -> Result<()>` (aynı rolde mevcut ebeveyn varsa ve farklıysa: origin='manual' ise değiştir, 'pdf' ise `Error::Invalid("Çakışan ebeveyn …")`); döngü oluşturacaksa (parent, child'ın soyundansa) `Error::Invalid`.
  - `pub fn remove_parent(conn, child, role: Sex) -> Result<()>`, `pub fn set_spouse(conn, a, b)`, `pub fn remove_spouse(conn, a, b)`
  - `pub fn parents(conn, id) -> Result<(Option<i64>, Option<i64>)>` (baba, anne), `pub fn children(conn, id) -> Result<Vec<i64>>`, `pub fn spouses(conn, id) -> Result<Vec<i64>>`
  - `pub fn add_sibling(conn, of: i64, &PersonInput) -> Result<i64>` — `of`'un ebeveyni yoksa iki yer tutucu ebeveyn oluşturup `of`'a da bağlar, sonra yeni kişiyi aynı ebeveynlere bağlar (amca/hala = babanın kardeşi, dayı/teyze = annenin kardeşi bu yolla).
  - `pub fn merge_persons(conn, keep: i64, drop: i64) -> Result<()>` — boş alanları `drop`'tan doldurur, tüm bağları/kaynakları `keep`'e taşır (çakışmada `keep` kazanır), `drop`'u siler; tek işlem (transaction).

- [ ] **Step 1: Failing tests** `migrate_is_idempotent`, `manual_update_records_fields`, `set_parent_rejects_cycle`, `add_sibling_creates_placeholder_parents` (ebeveynsiz kişiye kardeş → 2 yer tutucu, ikisi de aynı ebeveynlere bağlı), `merge_moves_links` (çocuğu ve eşi olan `drop` birleşince bağlar `keep`'te, `drop` yok), `delete_cascades_links`.
- [ ] **Step 2:** FAIL → uygula → PASS. Commit `feat: SQLite şeması ve kişi işlemleri`.

### Task 5: İçe aktarma — önizleme, eşleşme önerileri, onaylı yazma

**Files:**
- Create: `core/src/import.rs`
- Test: `core/tests/import.rs` (fixture PDF'lerle, bellek içi DB)

**Interfaces:**
- Consumes: `extract_words`, `parse_records`, `build_graph`, db fonksiyonları.
- Produces:
  - `pub struct ImportPreview { file_name: String, nodes: Vec<PreviewNode> }`, `pub struct PreviewNode { index: usize, record: Option<Record>, path_text: String, candidates: Vec<Candidate>, suggested: Option<i64> }`, `pub struct Candidate { person_id: i64, score: u8 /*0-100*/, reasons: Vec<String> }`.
  - `pub fn preview_import(conn, file_name: &str, pdf_bytes: &[u8]) -> Result<ImportPreview>`
  - `pub struct ImportDecision { index: usize, merge_into: Option<i64>, split_duplicate: bool }`; `pub fn commit_import(conn, preview: &ImportPreview, decisions: &[ImportDecision]) -> Result<ImportSummary>` (`ImportSummary { source_id, created, merged, links_added }`), tek transaction.
- Eşleşme puanı (`tr_fold` ile): TC eşit → 100 ve `suggested` (TC tek otomatik kural, yine önizlemede görünür). Aksi halde ad (25) + soyad (10) + baba adı (15) + ana adı (15) + doğum tarihi (20) + doğum yeri (5) + cilt-hane-birey ve il-ilçe birlikte (10). Cinsiyet çelişirse aday değil. `not_same` çiftleri aday değil. Skor ≥ 85 → `suggested` (UI'da önceden işaretli, kullanıcı onaylamadan yazılmaz). Ayrıca aynı önizlemedeki grafik bağlamı: zaten eşleşmiş çocuğun aynı roldeki ebeveyni varsa o ebeveyn +20 (en fazla 100).
- Birleştirmede: `manual_fields`'taki alanlara dokunma, boş alanları doldur, dolu ve farklı PDF değeri → PDF değeri yazılır (elle değilse). Ebeveyn kenarları `origin='pdf'`; mevcut farklı ebeveyn çakışması → kenar atlanır ve `ImportSummary.conflicts: Vec<String>`'e eklenir (içe aktarma durmaz).

- [ ] **Step 1: Failing tests:**
  - `import_ali_creates_people_and_links`: boş DB, ali.pdf önizle, tüm kararlar `merge_into=None` → kayıtlar + 1 yer tutucu kişi; Ali'nin babası ve annesi doğru. Akraba evliliğindeki ortak ata PDF'te iki satır olarak geçer; önizlemede ikinci satır `duplicate_of = Some(ilk satırın index'i)` ile işaretlenir ve UI'da görünür (kullanıcı "ayrı kişi" diyebilir: karar `ImportDecision.split_duplicate: bool`). Varsayılan kararla commit → ortak ata tek `person` satırı, iki çocuğu var.
  - `kuzen_preview_suggests_shared_grandparents`: Ali içe aktarılmış DB'ye ayse-kuzen.pdf önizleme → anneanne/dede düğümlerinin `suggested` Ali'deki kişileri gösteriyor; Ayşe'nin kendisi için öneri yok.
  - `reimport_same_pdf_creates_no_duplicates`: ali.pdf'i önerileri kabul ederek ikinci kez → kişi sayısı değişmez, `links_added == 0`.
  - `import_keeps_manual_fields`: Ali'nin babasının `birth_place` alanı elle "Uydurmaköy" yapılır, yeniden içe aktarma → hâlâ "Uydurmaköy".
  - `not_same_suppresses_candidate`.
- [ ] **Step 2:** `PreviewNode`'a `duplicate_of: Option<usize>` ekle (aynı PDF içindeki birebir eş kayıtlar: ad, soyad, baba, ana, doğum tarihi+yeri, cilt-hane-birey hepsi eşit). FAIL → uygula → PASS.
- [ ] **Step 3:** Commit `feat: önizlemeli PDF içe aktarma ve kişi eşleştirme`.

### Task 6: Akrabalık hesabı ve akraba evliliği raporu

**Files:**
- Create: `core/src/kinship.rs`
- Test: unit + `core/tests/kinship.rs` (fixture'lar içe aktarılmış DB ile)

**Interfaces:**
- Produces:
  - `pub struct CommonAncestor { ancestor: i64, up_a: u32, up_b: u32, path_a: Vec<i64>, path_b: Vec<i64> }` (yollar kişiden ataya, iki uç dahil).
  - `pub struct Relationship { a: i64, b: i64, closest: Vec<CommonAncestor>, label: String }`
  - `pub fn common_ancestors(conn, a, b) -> Result<Vec<CommonAncestor>>` — her iki kişinin tüm atalarını BFS ile (ziyaret kümesi, en kısa mesafe) bul; ortak olanlardan, kendisi başka bir ortak atanın atası olmayanları ("en yakın") döndür; `up_a+up_b`'ye göre sırala. Bir kişi diğerinin atası ise o kişi de ortak ata sayılır (`up=0`).
  - `pub fn relationship_label(up_a: u32, up_b: u32, side: Side, sex_b: Option<Sex>) -> String` — B'nin A'ya göre ne olduğu. `pub enum Side { Paternal, Maternal, Unknown }` (A'nın yolunun ilk adımı).
  - `pub fn find_relationship(conn, a, b) -> Result<Option<Relationship>>`
  - `pub struct ConsanguinityItem { husband: i64, wife: i64, relationship: Relationship }`; `pub fn consanguinity_report(conn) -> Result<Vec<ConsanguinityItem>>` — çiftler = `spouse_link` ∪ ortak çocuğu olan (baba, anne) ikilileri; tekilleştir; ortak atası olanları döndür, en yakından uzağa.
- Etiket tablosu (up_a, up_b) → B:
  - (0,0) "Kendisi"; (1,0) "Babası"/"Annesi"/"Ebeveyni"; (0,1) "Oğlu"/"Kızı"/"Çocuğu"; (2,0) Paternal: "Dedesi (baba tarafı)"/"Babaannesi", Maternal: "Dedesi (anne tarafı)"/"Anneannesi"; (n≥3,0) "{n-2}. kuşak büyük dedesi/büyük annesi" → n=3 "Büyük dedesi"/"Büyük annesi", n≥4 "{n-2}. dereceden büyük dedesi/annesi"; (0,2) "Torunu"; (0,n≥3) "{n-1}. kuşak torunu"; (1,1) "Erkek kardeşi"/"Kız kardeşi"/"Kardeşi"; (2,1) Paternal: "Amcası"/"Halası", Maternal: "Dayısı"/"Teyzesi"; (1,2) "Yeğeni"; (2,2) "Birinci dereceden kuzeni"; genel a,b ≥ 2: `k = min(a,b)-1`, `d = |a-b|`, "{k}. dereceden kuzeni" + (d>0 ise " ({d} kuşak fark)"); (a≥3,1) "{a-1}. dereceden amca/hala/dayı/teyze akrabası" yerine sade: "Büyük amcası" vb. gereksiz — "{a-2}. kuşak büyük amcası/halası/dayısı/teyzesi"; (1,b≥3) "{b-2}. kuşak yeğen torunu" yerine "Yeğeninin {b-2}. kuşak torunu". Yarım kardeş (tek ortak ebeveyn, up 1/1): "Üvey/yarım kardeşi".

- [ ] **Step 1: Failing tests:** `label_table` (yukarıdaki satırların her biri için bir assert; ör. `relationship_label(2,2,Paternal,None) == "Birinci dereceden kuzeni"`, `(4,3) == "2. dereceden kuzeni (1 kuşak fark)"`, `(2,1,Maternal,Some(Female)) == "Teyzesi"`), `ali_parents_are_second_cousins` (fixture DB: `consanguinity_report` tek kayıt, Ali'nin anne-babası, label "2. dereceden kuzeni", ortak ata = fixture'daki ortak ata), `ali_ayse_first_cousins` (iki PDF önerilerle içe aktarılmış → `find_relationship(ali, ayse).label == "Birinci dereceden kuzeni"`, 2 ortak ata: anneanne ve dede), `double_cousins_two_common_ancestor_pairs` (elle kurulmuş: iki kardeş başka iki kardeşle evli → çocukları arasında 4 en yakın ortak ata), `ancestor_cycle_free_on_pedigree_collapse` (ortak atanın iki yoldan gelmesi; BFS biter, mesafe en kısa olan).
- [ ] **Step 2:** FAIL → uygula → PASS. Commit `feat: akrabalık ve akraba evliliği hesabı`.

### Task 7: Ağaç yerleşimi (Rust'ta koordinat)

**Files:**
- Create: `core/src/layout.rs`
- Test: unit

**Interfaces:**
- Produces:
  - `pub struct LayoutNode { key: String /*benzersiz düğüm anahtarı*/, person_id: i64, x: f32, y: f32, repeated: bool, highlight: bool }`, `pub struct LayoutEdge { from: String, to: String, highlight: bool }`, `pub struct TreeLayout { nodes: Vec<LayoutNode>, edges: Vec<LayoutEdge>, width: f32, height: f32 }`.
  - Sabitler: `NODE_W = 180.0`, `NODE_H = 64.0`, `H_GAP = 24.0`, `V_GAP = 56.0`.
  - `pub fn ancestors_layout(conn, root: i64, max_gen: u32) -> Result<TreeLayout>` — kök altta, ataları yukarı; babalar solda; ikinci kez görülen kişi `repeated=true` ve onun ataları açılmaz.
  - `pub fn descendants_layout(conn, root: i64, max_gen: u32) -> Result<TreeLayout>` — kök üstte; eşler kişinin yanına yan düğüm olarak (`key` "s:{id}:{spouse}").
  - `pub fn relationship_layout(conn, rel: &Relationship) -> Result<TreeLayout>` — en yakın her ortak ata üstte, iki yol aşağı iner, tüm düğüm/kenar `highlight=true`.
  - Yerleşim algoritması: alt ağaç genişliği = max(NODE_W, çocuk genişlikleri + aralar toplamı); düğüm, çocuklarının ortasına; y = nesil × (NODE_H+V_GAP). Örtüşme olmamalı.

- [ ] **Step 1: Failing tests:** `no_overlap_in_ancestors` (fixture DB, Ali, 4 kuşak: aynı y'deki düğümler arası mesafe ≥ NODE_W+H_GAP), `repeated_ancestor_marked` (ortak ata bir kez `repeated=false`, bir kez `true`), `parent_centered_over_children` (descendants: tek çocuklu ebeveynle çocuğun x'leri eşit), `relationship_layout_highlights_paths` (Ali–Ayşe: düğüm sayısı = iki yol uzunluğu toplamı − ortak atalar tekrar edilmeden).
- [ ] **Step 2:** FAIL → uygula → PASS. Commit `feat: ağaç diyagramı yerleşimi`.

### Task 8: JSON yedek/geri yükleme

**Files:**
- Create: `core/src/backup.rs`
- Test: unit

**Interfaces:**
- Produces: `pub fn export_json(conn) -> Result<String>` (`{"format":"familytree-backup","version":1,"persons":[…],"parent_links":[…],"spouse_links":[…],"sources":[…],"person_sources":[…],"not_same":[…]}`), `pub fn import_json(conn, json: &str) -> Result<()>` (mevcut veriyi transaction içinde tamamen değiştirir; format/version yanlışsa `Error::Invalid("Bu dosya bir FamilyTree yedeği değil")`).
- [ ] **Step 1: Failing tests** `roundtrip_preserves_everything` (fixture içe aktar → export → boş DB'ye import → export eşit), `rejects_foreign_json`.
- [ ] **Step 2:** FAIL → uygula → PASS. Commit `feat: JSON yedekleme`.

### Task 9: Tauri kabuğu ve IPC komutları

**Files:**
- Create: `src-tauri/Cargo.toml`, `src-tauri/build.rs`, `src-tauri/tauri.conf.json`, `src-tauri/capabilities/default.json`, `src-tauri/icons/*` (Python/PIL ile basit ağaç simgesi: 32x32, 128x128, 128x128@2x, icon.ico, icon.png), `src-tauri/src/main.rs`, `package.json` (`"@tauri-apps/cli": "^2"` güncel 2.x)
- Test: `src-tauri/src/main.rs` içinde `--selftest` bayrağı (pencere açmadan geçici DB'ye fixture içe aktarır, rapor üretir, 0 ile çıkar)

**Interfaces:**
- Consumes: core'un tüm public API'si.
- Produces (komutlar; build.rs `COMMANDS` listesi = `generate_handler!` listesi = `capabilities/default.json` `allow-*`, MDViewer düzeni):
  `pick_and_preview_pdf() -> Option<ImportPreview>` (dialog eklentisi Rust tarafında, filtre "PDF"), `commit_import(preview, decisions) -> ImportSummary`, `layout_dump_for_pdf() -> Option<String>` (seçilen PDF için `anonymized_layout_dump`, ayrıştırma hata verdiğinde UI önerir), `list_persons`, `get_person(id)`, `create_person(input, relation: Option<{to: i64, kind: "father"|"mother"|"child"|"spouse"|"sibling"}>) -> i64`, `update_person(id, input)`, `delete_person(id)`, `set_parent(child, parent, role)`, `remove_parent(child, role)`, `set_spouse(a,b)`, `remove_spouse(a,b)`, `merge_persons(keep, drop)`, `mark_not_same(a,b)`, `duplicate_suggestions() -> Vec<(i64,i64,u8)>` (mevcut DB'deki skor ≥ 85 çiftler), `find_relationship(a,b)`, `consanguinity_report()`, `ancestors_layout(id, gens)`, `descendants_layout(id, gens)`, `relationship_layout(a,b)`, `export_backup() -> bool` (kaydet diyaloğu, varsayılan ad `familytree-yedek-YYYY-MM-DD.json`), `import_backup() -> bool`.
  Durum: `struct AppState(Mutex<Connection>)`; hatalar `String` olarak UI'a.
- `tauri.conf.json`: `frontendDist: "../ui"`, `withGlobalTauri: true`, pencere 1280×860 min 900×600, Global Constraints'teki CSP, `bundle.active: true`, `targets: ["deb","appimage","nsis"]`.

- [ ] **Step 1:** Dosyaları oluştur; `cargo build -p familytree` derlenir (sistem paketleri kuruluysa; değilse CI).
- [ ] **Step 2:** `cargo run -p familytree -- --selftest` → çıktıda `selftest ok: 16 kişi, 1 akraba evliliği` ve çıkış kodu 0.
- [ ] **Step 3:** Commit `feat: Tauri kabuğu ve komutlar`.

### Task 10: Arayüz

**Files:**
- Create: `ui/index.html`, `ui/style.css`, `ui/app.js` (durum, kişi listesi, sekmeler), `ui/tree.js` (TreeLayout → SVG, fare tekerleği yakınlaştırma, sürükleyerek kaydırma, "Sığdır" düğmesi), `ui/dialogs.js` (kişi formu, içe aktarma önizlemesi)

**Davranış (ekran):**
```
┌──────────────┬───────────────────────────────────────────────┐
│ [PDF içe akt]│  Sekmeler: Ata ağacı | Soy ağacı | Akrabalık  │
│ [Kişi ekle]  │                                               │
│ 🔍 ara...    │          (SVG ağaç, yakınlaştır/kaydır)       │
│ Ali YILMAZ   │                                               │
│ Ayşe DEMİR   │                                               │
│ …            ├───────────────────────────────────────────────┤
│ ⚠ Akraba     │ Seçili kişi: bilgiler · Düzenle · Ebeveyn/    │
│  evlilikleri │ Çocuk/Eş/Kardeş ekle · Sil · "Bu kişiyle      │
│  (1)         │ akrabalığı bul…"                              │
└──────────────┴───────────────────────────────────────────────┘
```
- Açılışta `consanguinity_report` > 0 ise sol altta uyarı rozeti; tıklayınca liste, her satır "X ile Y: 2. dereceden kuzen — ortak ata Z" ve tıklayınca Akrabalık sekmesinde yol çizimi.
- İçe aktarma önizlemesi: tablo (yakınlık, ad soyad, doğum, eşleşme açılır listesi: "Yeni kişi" / adaylar puanla); `suggested` önceden seçili; "İçe aktar" ile `commit_import`; sonuçta çakışmalar listelenir. Ayrıştırma hatasında: hata metni + "Düzen dökümünü kaydet" (kişisel veri içermediği açıklanır).
- Kişi düğmeleri: cinsiyete göre renk (tema değişkenleri), yer tutucu kesik çizgili "Bilinmiyor", `repeated` düğümde "↻" işareti, ölü kişide "†". Düğüme tıklama → seçili kişi; çift tıklama → o kişiyi kök yap.
- Mevcut kopya önerileri (`duplicate_suggestions`) kişi listesinde "Olası aynı kişi" bölümü: "Birleştir" / "Farklı kişiler".
- Açık/koyu tema `prefers-color-scheme` ile.

- [ ] **Step 1:** Uygula. `cargo run -p familytree` ile elle doğrula: iki fixture içe aktarılır, öneriler işaretli gelir, Ali–Ayşe "Birinci dereceden kuzeni" gösterilir, akraba evliliği rozeti 1, Ali'ye "Kardeş ekle" ve babasına "Kardeş ekle" (amca) çalışır, yedek al/geri yükle çalışır. Ekran görüntüsü yalnızca fixture verisiyle `docs/screenshot.png`.
- [ ] **Step 2:** Commit `feat: arayüz`.

### Task 11: CI, paketleme, README

**Files:**
- Create: `.github/workflows/test.yml` (ubuntu-22.04: WebKitGTK paketleri + `cargo test --workspace --locked`; windows-latest: `cargo test --workspace --locked`), `.github/workflows/release.yml` (release yayınlanınca ubuntu-22.04'te deb+AppImage, windows-latest'te NSIS: `npm ci && npx tauri build`, `gh release upload`), README (özellikler, gizlilik notu: veri yalnızca yerel; e-Devlet'ten PDF alma adımları; derleme önkoşulları; ekran görüntüsü).
- Action sürümleri MDViewer'daki ile aynı (`actions/checkout@v7`, `actions/setup-node@v7`, `dtolnay/rust-toolchain@stable`, `Swatinem/rust-cache@v2`).

- [ ] **Step 1:** Commit öncesi kişisel veri kontrolü: `git ls-files | grep -iE '\.(pdf|db|sqlite)$'` yalnızca `tests/fixtures/` gösterir; `git grep -nI` ile fixture dışı TC biçimli 11 haneli sayı yok.
- [ ] **Step 2:** Commit `ci: test ve sürüm iş akışları`.
