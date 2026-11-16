# FamilyTree — Tasarım

## Amaç ve neden
e-Devlet'ten alınan "Soy Ağacı" PDF çıktıları tablo halinde, okunması zor ve her kişi için ayrı. Kullanıcı:
- kendi, kuzenlerinin ve diğer akrabaların soy ağacı PDF'lerini **tamamen yerel** bir uygulamaya aktarmak,
- hepsini tek veritabanında birleştirmek,
- anne ile baba arasındaki uzak akrabalığı (ve genel olarak herhangi iki kişi arasındaki akrabalığı) bulmak,
- sonucu ağaç diyagramı olarak görmek istiyor.

Kişisel veriler hiçbir dış servise gitmez; geliştirme sırasında da gerçek PDF paylaşılmaz.

## Hedefler
1. e-Devlet soy ağacı PDF'ini (tarayıcıdan "PDF olarak yazdır" çıktısı, metin seçilebilir) içe aktarma.
2. Kişileri TC kimlik no ile tekilleştirme; TC yoksa ad+soyad+doğum tarihi+baba/anne adı ile eşleşme önerisi (kullanıcı onaylar).
3. "Yakınlık" sütunundan (ör. "Babasının Annesinin Babası") ebeveyn ilişkilerini kurma; PDF sahibi (kök kişi) belirlenir.
4. **Elle kişi ekleme/düzenleme/silme**: kardeş, amca, dayı, hala, teyze ve PDF'te çıkmayan herkes. Elle eklenen kişiye ebeveyn bağlanır (ör. amca = babanın babasının çocuğu).
5. İki kişi seçilince ortak ata(lar), her iki taraftan yol ve Türkçe akrabalık adı / kuzen derecesi (ör. "3. dereceden kuzen, 1 kuşak fark").
6. Uygulama açılışında tüm aile içinde **akraba evlilikleri** (ebeveynleri ortak ataya sahip çiftler) otomatik listelenir.
7. Ağaç görünümü: yakınlaştır/kaydır, kişiye tıklayınca detay, seçilen iki kişi arasındaki yolu vurgulama, ata ağacı (yukarı) ve soy (aşağı) modları.
8. Linux (AppImage/deb) ve Windows (NSIS) kurulumları.

## Hedef dışı
- Çevrimiçi senkronizasyon, hesap, bulut.
- Taranmış (resim) PDF için OCR (ilk sürümde yok; gerekirse sonra).
- GEDCOM dışa aktarma (sonraya bırakılabilir; JSON yedekleme/geri yükleme ilk sürümde var).

## Anti-hedefler
- Herhangi bir veri ağa çıkarsa başarısızlıktır (Tauri ağ yetkisi kapalı, harici CDN yok).
- Yanlış birleştirme sessizce yapılırsa başarısızlıktır: TC'siz eşleşmeler daima onay ister.
- İçe aktarma, elle girilmiş veriyi ezmemeli.

## Kısıtlar
- Diğer projelerle aynı yapı: Cargo workspace, `core/` (saf Rust, testli) + `src-tauri/` + `ui/`.
- Bağımlılıkların hepsi güncel sürümler (Dependabot uyumlu); `.github/dependabot.yml` eklenecek.
- Arayüz Türkçe.
- **Depo herkese açık (public) paylaşılabilir; repoya hiçbir gerçek kişisel veri girmez:**
  - `.gitignore`: `*.pdf` (yalnızca `tests/fixtures/` altındaki sahte PDF'ler hariç), `*.db`, `*.sqlite*`, yedek `*.json` dışa aktarımları, `.claude/`.
  - Test verileri, örnekler, ekran görüntüleri ve README yalnızca uydurma isim ve geçersiz TC no (ör. `00000000000` biçimli) kullanır.
  - Gerçek veritabanı repo dışında, kullanıcı veri klasöründe durur.
  - Commit öncesi kontrol: `git diff --cached` içinde gerçek isim/TC/PDF olmadığından emin olunur.

## Yaklaşımlar
- **Tauri 2 + Rust (seçilen):** projelerle tutarlı, küçük kurulum, CLauncher'a eklenebilir.
- Python + PySide6: daha hızlı geliştirme, ama büyük Windows paketi ve tutarsız yapı. Reddedildi.

## Mimari
- `core/`
  - `pdf`: PDF'ten metin çıkarma (pdf-extract veya lopdf; sütunlar satır içi konumdan ayrıştırılır).
  - `parse`: e-Devlet tablo satırlarını `Kayit { sira, cinsiyet, yakinlik, ad, soyad, baba_adi, ana_adi, dogum_yeri_tarihi, il_ilce, mahalle_koy, cilt_hane_birey, medeni_hal, durum (sağ/ölü), olum_tarihi }` yapısına çevirir. TC no varsa alınır.
  - `relation`: "Babasının Annesinin…" zincirini köke göre ebeveyn yoluna çevirir ("Kendisi", "Annesi", "Babası", "Kızı", "Oğlu", "Kardeşi", "Eşi" vb.).
  - `db`: SQLite (rusqlite, bundled). Tablolar: `person`, `parent_link(child, parent, role)`, `spouse_link`, `source(pdf adı, içe aktarma tarihi, kök kişi)`, `person_source`. Her alanın elle mi PDF'ten mi geldiği işaretlenir.
  - `merge`: tekilleştirme ve eşleşme önerileri.
  - `kinship`: en yakın ortak atalar (BFS), kuzen derecesi, Türkçe ad.
- `src-tauri/`: komutlar (import_pdf, list/get/upsert/delete person, link parent, find_relation, consanguinity_report, tree_data, backup/restore). Dosya seçici: tauri-plugin-dialog.
- `ui/`: düz HTML/JS/CSS (paketleyici yok), ağaç çizimi yerel olarak paketlenmiş bir kütüphane (d3 veya family-chart) ile.
- Veritabanı: kullanıcı veri klasörü (`dirs::data_dir()/FamilyTree/familytree.db`).

## Test
- Gerçek veri yerine sahte isimli örnek PDF'ler (e-Devlet düzeninde) üretilip `tests/fixtures` altında tutulur.
- `core` için birim testleri: yakınlık zinciri, ayrıştırma, birleştirme, akrabalık hesabı.
- Gerçek PDF'in sütun düzeni farklı çıkarsa kullanıcıdan yalnızca başlık satırı / kimliksiz bir satır istenir.

## Uygulayıcıya kalanlar
- PDF metin çıkarma crate'i seçimi (fixture üzerinde denenerek).
- Ağaç kütüphanesi seçimi ve görsel tasarım ayrıntıları.
