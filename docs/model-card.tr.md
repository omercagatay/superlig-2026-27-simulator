# Model kartı: Süper Lig 2026-27 tahmin modeli

[English](model-card.md) | [Türkçe](model-card.tr.md)

Bu kart, deponun tahminlerini üreten modeli `2026-10b` etiketiyle yayındaki hâliyle anlatır (19 Ekim 2026). Başlıklar Mitchell ve arkadaşlarının "Model Cards for Model Reporting" (FAT* 2019) makalesini izler. `data/metrics/` dosyalarından alınan her rakam, CI'da `python3 scripts/check_metrics.py docs` ile denetlenen gizli bir işaretçinin içindedir. Diğer her rakam nereden geldiğini söyler ve denetlenmez. Her bölümün ardındaki ayrıntılar [CLAUDE.md](../CLAUDE.md) içindedir (İngilizce); Türkçe özeti [README.tr.md](../README.tr.md) verir.

## Model ayrıntıları

| | |
|---|---|
| Etiket | `2026-10b` (`src/model_version.rs` içindeki `LABEL`). Önceki etiketler: `2026-08` (sezon öncesi puanlar ClubElo'dan, sezon içinde K 20, `D_DIV` 1600, güçsüz taraf düzeltmesi yok) ve `2026-10a` (yayındaki kurallar, ama 2024-25'in 10-12. haftaları hâlâ eksik olan geçmişle). |
| Parmak izi | `FINGERPRINT` `0xd46b_9fb0_56c6_d09c`: bir maç sonucu tahminini oynatan her girdinin (model sabitleri, varsayılan ağırlıklar, kulüp listesi, `dc_params.json` ve sonuç geçmişi) FNV-1a özeti. `PROBE` `0x4522_bdaa_bfe1_e492`: modelin her eşleşme için yaptığı sezon öncesi tahminlerin özeti. Testler ikisini de sabitler; böylece değişmiş bir model bu etiketle yayına çıkamaz. |
| Yayın | 19 Ekim 2026: Ekim 2026 ayarı, düzeltilmiş geçmiş üzerinde ve Dixon–Coles yeniden kalibre edilerek. |
| Sahibi | Çağatay (GitHub'da `omercagatay`), depo `omercagatay/superlig-2026-27-simulator`. Sorular ve sorunlar için GitHub issues. |
| Lisans | Kod: MIT (`LICENSE`). Veriler kendi koşullarına tabidir (`THIRD_PARTY_NOTICES.md`): maç arşivi MIT, openfootball CC0; tff.org'dan alınan 2025-26 sonuçları ile 2026-27 fikstürü ve sonuçları MIT Lisansı kapsamında değildir. |

**Maç modeli.** Her maçta iki tarafın beklenen gol oranı (λ), Süper Lig sonuçlarına kalibre edilmiş üç güç modelinin ağırlıklı harmanıdır (`ENSEMBLE_WEIGHTS`, varsayılan `0.5,0.3,0.2`):

| Bileşen | Ağırlık | Kulüpleri nasıl değerlendirir |
|---|---:|---|
| Elo–Poisson | 0,5 | λ<sub>ev</sub> = 1,35 × 10^((fark + 80) / 1200) ve λ<sub>deplasman</sub> = 1,35 × 10^(−(fark + 80) / 1200); fark = ev sahibinin Elo puanı − deplasman takımının Elo puanı, 80 de ev sahibi avantajı. Sezon öncesi puanlar, 2012-13'ten bu yana her üst lig sonucunu K 20 ile yeniden oynatır; hiç üst lig sonucu olmayan bir kulüp 1450'den başlar. Sezon içinde her sonuç, puanları K 60 ile ve gol farkının karekökünü çarpan alarak oynatır; puanlar her yenilemede sezon öncesi başlangıçtan yeniden hesaplanır. |
| Dixon–Coles | 0,3 | Hücum, savunma, ev sahibi avantajı ve az gollü skor düzeltmesi ρ; çevrimdışı kalibre edilir (`examples/fit_dc.rs`, dört yıllık yarılanma ömrü, `data/dc_params.json`). Taraf başına 10 gole kadar uzanan ortak skor tablosu skor dağılımını verir. Yeniden kalibre edilene kadar sabit kalır. |
| Pi-rating | 0,2 | Constantinou–Fenton ev ve deplasman puanları; açılışta aynı geçmiş üzerinden tek geçişte hesaplanır. Sezon içinde sabit kalır. |

Harmanlamadan sonra güçsüz taraf düzeltmesi 0,9'un altındaki her λ'yı aradaki farkın %70'i kadar yukarı çeker; favorinin λ'sı da bu göreli artışın yarısı kadar yükselir. Maç olasılıkları (ev/beraberlik/deplasman, 3+ gol, karşılıklı gol, skorlar) Monte Carlo ile değil, skor tablosu üzerinden kesin toplamlarla hesaplanır.

**Sezon simülatörü.** Varsayılan olarak 50.000 sezon (tohum 12345; istenirse 100 ile 200.000 arası). Oynanmış maçların sonuçları sabittir; diğer her maç harmandan çekilir. Her kulüp, simüle edilen sezon boyunca sabit kalan tek bir güç şoku çeker: N(0, 75) Elo puanı (`RATING_SIGMA`). Simüle edilen sonuçlar Elo puanlarını oynatmaz (soğuk simülasyon); kullanıcının sabitlediği bir sonuç ise onları aynı gerçek sonucun oynatacağı gibi oynatır. Sezon sonu puan durumu lig kurallarına uyar: ikili averaj genel averajdan önce gelir, play-off'un yerini rastgele bir çekiliş alır. Çıktılar: sıralama olasılıkları; şampiyonluk, ilk iki, üçüncülük, dördüncülük, ilk dört ve küme düşme olasılıkları; beklenen galibiyet, beraberlik ve mağlubiyet sayılarından tahminî bir puan durumu; her sıra için gereken puan; kimin kimin üstünde bitireceği.

**Senaryo araçları.** Elle yapılan bir puan değişikliği ya da bir LLM'in puan değişikliklerine çevirdiği doğal dilde bir senaryo (varsayılan olarak kapalı bir API uç noktası), o kulübün harmanın tamamındaki gücünü kaydırır. Bunlar modelin parçası değil, modelin üstüne eklenen kullanıcı varsayımlarıdır; aşağıdaki değerlendirme hiçbirini kullanmaz.

## Kullanım amacı

- **Birincil:** 2026-27 Süper Lig'ini izleyenler için olasılıklı tahminler: şampiyonluk, ilk dört ve küme düşme olasılıkları, tahminî sezon sonu puan durumu, gelecek haftanın ev/beraberlik/deplasman olasılıkları, her sıra için yetmiş olan puanlar ve senaryo denemeleri.
- **İkincil:** bir spor tahmininin maç maç, sezon sezon ve piyasaya karşı nasıl değerlendirileceğine dair yeniden üretilebilir bir örnek.

Kapsam dışı:

- **Bahis ya da herhangi bir mali karar.** Model, ölçülen her dönemde bahis şirketi oranlarına yenilir ([Piyasaya karşı](#piyasaya-karşı-çevrimdışı)); bahis için bir dayanak değildir. Uygulamada, API'sinde ve arayüzünde hiçbir bahis şirketi oranı ya da piyasa rakamı yoktur. Piyasa karşılaştırması yalnızca çevrimdışı bir değerlendirmedir: oranları depoya hiç eklenmez ve uygulamada hiçbir şey onları okumaz. API'deki `*_odds` alanları fiyat değil, modelin kendi 1 / p değeridir; panel hiç oran göstermez.
- **Avrupa kupalarına katılım.** Katılım kupa şampiyonuna ve o sezonun erişim listesine de bağlı olduğundan site turnuva adları değil, sıralama bantları verir (ilk iki, üçüncü, dördüncü, ilk dört).
- **Kesin puan toplamları.** Sezon aralıkları geliştirme sezonlarında fazla dardı ([Sezon düzeyi](#sezon-düzeyi-sezon-değerlendirmesi)); şampiyonluk olasılıkları en olası şampiyonun kim olduğunu söyler, kaç puanla şampiyon olacağını değil.
- **Yeniden kalibre edilmeden başka ligler, kümeler ya da sezonlar**, ayrıca kulüp düzeyinin altındaki her şey: model yalnızca sonuçları görür; ilk on birleri, sakatlıkları, transferleri ya da xG'yi hiç görmez.
- **Tahmin yerine konan LLM senaryoları:** bunlar modelin ürettiği varsayımlardır, keşif içindir.

## Etkenler

Modelin isabetini oynattığı ölçülen ya da bilinen etkenler:

- **Sezonun evresi.** Sonuçlar geldikçe tahminler keskinleşir (aşağıdaki sezon düzeyi tablosu). Piyasayla aradaki farkın hafta bantlarına göre dağılımı [Piyasaya karşı](#piyasaya-karşı-çevrimdışı) bölümündedir.
- **Yükselen kulüplerin** (2026-27 için Amedspor ve Çorum) üst lig geçmişi yoktur: Dixon–Coles ve pi-rating onları lig ortalaması sayar, bu yüzden onları yalnızca Elo puanları ayırır.
- **Güç farkı büyük maçlar.** Belirgin biçimde güçsüz takımlar, harmanın 0,66 dediği yerde maç başına 0,88 gol attı (güçsüz taraf düzeltmesinin dayandığı ölçüm).
- **Sezonun gol düzeyi** sezondan sezona değişir (2012-13'ten bu yana maç başına 2,64 ile 3,05 gol arası; `tests/calibration.rs`'ten) ve modelde bunu sezon içinde izleyen bir şey yoktur: 2025-26'da maç başına 2,988 gol tahmin edildi, 2,654 gol atıldı (geriye dönük testin sezon satırı; denetlenmez).

Model hiçbir kişisel ya da demografik veri kullanmaz; tek birim kulüplerdir.

## Veri

**Geçmiş** (`data/superlig_results.csv`, derleme sırasında gömülür): 2012-13'ten 2025-26'ya her Süper Lig maçı; hükmen kararlar çıkarıldığında 4.588 oynanmış maç.

| Sezonlar | Kaynak | Notlar |
|---|---|---|
| 2012-13 – 2024-25 | Club Football Match Data arşivi (MIT; Football-Data.co.uk verilerinden derlenmiş) | gerçek tarihler; kullanılan satırlar SHA-256 ile sabitlenir (`ARCHIVE_SLICE_SHA256`) |
| 2024-25, 10-12. haftalar (27 maç) | openfootball `2024-25/tr.1.json` (CC0) | kaynakta eksik; arşivin o sezondaki her satırı bununla uyuşmalıdır; SHA-256 ile sabitlenir |
| 2025-26 | tff.org haftalık sayfaları | windows-1254 ile çözülür; TFF'nin eksik TLS zinciri sabitlenmiş bir ara sertifikayla tamamlanır |

Düzeltmeler: Şubat 2023 depreminden sonra hükmen sonuçlandırılan 29 maç dosyada kalır ve yüklenirken çıkarılır; 2024-25'teki Galatasaray - Adana Demirspor hükmen kararı daha kaynakta dışarıda bırakılır; Karagümrük'ün iki farklı yazımı birleştirilir; her sezon, bu kararlar dışında eksiksiz bir çift devreli lig olmalıdır; sezon sınırının ağustosta olması Temmuz 2020 maçlarını 2019-20 sezonunda tutar. Ayrıntılar: [README.tr.md, "Verileri yenileme"](../README.tr.md#verileri-yenileme); tamamı (İngilizce) [CLAUDE.md, "Data-source traps"](../CLAUDE.md#data-source-traps).

**Canlı sezon:** `data/fixtures_2026_27.json`, TFF'nin 306 maçlık fikstürü ve sonuçları. Fikstürü `scripts/fetch_fixtures.py` (perşembe ve cuma günleri zamanlanmış olarak) ve sunucunun veri çekicisi (varsayılan olarak 30 dakikada bir) yeniler; çekici herhangi bir sonucu uygulamadan önce 306 satırın tamamını doğrular.

**Yalnızca değerlendirme için:** aynı arşivden Bet365 ve en iyi oran maç öncesi oranları. `scripts/fetch_odds.py` bunları `data/odds/` altına yazar; `.gitignore` ve `.dockerignore` bu klasörü dışarıda bırakır. `src/` ya da `frontend/` içinde hiçbir şey onları okumaz.

## Değerlendirme tasarımı

| Değerlendirme | Araç | Neyi puanlar | Maçlar ya da sezonlar | Denetim |
|---|---|---|---|---|
| Geriye dönük test | `examples/backtest.rs` | her maç, üretimdeki kurallarla ve yalnızca önceki tarihlerin bilgisiyle fiyatlanır; aynı günün sonuçları fiyatlamadan sonra açılır | geliştirme 2016-17 – 2023-24 (<!-- metric:dev_n:,d:tr -->2.717<!-- /metric -->), ayrılmış 2024-25 ve 2025-26 (<!-- metric:hold_n:,d:tr -->647<!-- /metric -->) | `backtest.json`, CI'da yeniden çalıştırılıp karşılaştırılır |
| Sezon düzeyinde değerlendirme | `examples/season_eval.rs` | sıralama, şampiyonluk, ilk dört, küme düşme ve sezon sonu puanı; hiç maç oynanmamışken (s0), üçte biri (s33) ve üçte ikisi (s67) oynanmışken | <!-- metric:season:dev_n_seasons -->8<!-- /metric --> geliştirme ve <!-- metric:season:hold_n_seasons -->2<!-- /metric --> ayrılmış sezon, her biri için <!-- metric:season:n_sims:,d:tr -->2.000<!-- /metric --> simüle edilmiş sezon | `season.json`, CI'da yeniden çalıştırılıp karşılaştırılır |
| Kalibrasyon kontrolü | `tests/calibration.rs` | yayındaki harman, sezon öncesinde, geçmişe karşı | 4.588 maç | testin içinde sabitlenir |
| Canlı izleme | `examples/log_forecast.rs`, `src/accuracy.rs` | başlama düdüğünden önce dondurulan son tahmin, model etiketine göre puanlanır | 2026-27 | denetlenmez (`GET /api/accuracy`) |
| Piyasa karşılaştırması | `scripts/fetch_odds.py`, `examples/market.rs` | geriye dönük testin tahminleri, Bet365 maç öncesi oranlarına karşı; marj Shin yöntemiyle çıkarılır | geliştirme <!-- metric:market:dev_n:,d:tr -->2.714<!-- /metric -->, ayrılmış <!-- metric:market:hold_n:,d:tr -->620<!-- /metric -->, canlı | `market.json`; işaretçiler CI'da denetlenir, dosya elle yeniden üretilir |

- **Ayrılmış sezonlar hiç dokunulmamış değil, az kullanılmıştır.** Ekim 2026 ayarındaki iki karara bilgi verdiler, 4. aşamadaki yayılım değişikliğini veto ettiler ve `RATING_SIGMA` (75) onların puan tablosu yayılımına göre seçildi. Temiz sınav canlı 2026-27 sezonudur.
- **İstatistik.** Model karşılaştırmaları maç maç eşleştirilir: %95'lik tarih bloklu bootstrap aralığı (10.000 yeniden örnekleme; bir günün maçları aynı Elo puanlarıyla fiyatlanır), Diebold–Mariano tarzı bir t ve %95'lik sezon kümeli t aralığı (serbestlik derecesi sezon sayısının bir eksiği, yani ayrılmış sezonlarda bir). Beceri, genel oran tahmininin kaybından modelin giderdiği paydır; genel oranlar yalnızca puanlanan sezondan önceki sezonlardan alınır. Sezon rakamları %95'lik sezon kümeli bootstrap aralıkları taşır; 8 sezonda bunlar kabadır.
- **Ölçütler.** Maç düzeyi: maç sonucu (1X2) log kaybı (birincil skor), RPS, 3+ gol, karşılıklı gol ve tam skor için log kaybı, ortalamada kalibrasyon ve lojistik yeniden kalibrasyon eğimleri. Sezon düzeyi: kararı sıralama RPS'i ve puan CRPS'i verir; şampiyonluk, ilk dört ve küme düşme için Brier becerisi; aralık kapsaması, PIT ve hata oranı tanı amaçlıdır.

## Nicel sonuçlar

### Maç düzeyi (geriye dönük test)

| | Geliştirme | Ayrılmış |
|---|---:|---:|
| Maç | <!-- metric:dev_n:,d:tr -->2.717<!-- /metric --> | <!-- metric:hold_n:,d:tr -->647<!-- /metric --> |
| Maç sonucu log kaybı (genel oranlar) | <!-- metric:dev_1x2_ll:.4f:tr -->0,9991<!-- /metric --> (<!-- metric:dev_1x2_ll_base:.4f:tr -->1,0678<!-- /metric -->) | <!-- metric:hold_1x2_ll:.4f:tr -->0,9756<!-- /metric --> (<!-- metric:hold_1x2_ll_base:.4f:tr -->1,0669<!-- /metric -->) |
| Maç sonucu log kaybında beceri | <!-- metric:dev_1x2_skill:.1%:tr -->%6,4<!-- /metric --> | <!-- metric:hold_1x2_skill:.1%:tr -->%8,6<!-- /metric --> |
| RPS (beceri) | <!-- metric:dev_rps:.4f:tr -->0,2046<!-- /metric --> (<!-- metric:dev_rpss:.1%:tr -->%10,2<!-- /metric -->) | <!-- metric:hold_rps:.4f:tr -->0,1946<!-- /metric --> (<!-- metric:hold_rpss:.1%:tr -->%13,8<!-- /metric -->) |
| 3+ gol log kaybı (beceri) | <!-- metric:dev_ou25_ll:.4f:tr -->0,6821<!-- /metric --> (<!-- metric:dev_ou25_skill:.1%:tr -->%1,4<!-- /metric -->) | <!-- metric:hold_ou25_ll:.4f:tr -->0,6737<!-- /metric --> (<!-- metric:hold_ou25_skill:.1%:tr -->%2,4<!-- /metric -->) |
| Karşılıklı gol log kaybı (genel oranlar) | <!-- metric:dev_btts_ll:.4f:tr -->0,6858<!-- /metric --> (<!-- metric:dev_btts_ll_base:.4f:tr -->0,6862<!-- /metric -->) | <!-- metric:hold_btts_ll:.4f:tr -->0,6876<!-- /metric --> (<!-- metric:hold_btts_ll_base:.4f:tr -->0,6868<!-- /metric -->) |
| Tam skor log kaybı | <!-- metric:dev_score_ll:.3f:tr -->2,946<!-- /metric --> | <!-- metric:hold_score_ll:.3f:tr -->2,921<!-- /metric --> |
| İsabet oranı (en olası sonuç) | <!-- metric:dev_acc:.1%:tr -->%51,7<!-- /metric --> | <!-- metric:hold_acc:.1%:tr -->%54,1<!-- /metric --> |

Kalibrasyon, ortalama tahmin ve gerçekleşen oran:

| | Geliştirme, tahmin | Geliştirme, gerçekleşen | Ayrılmış, tahmin | Ayrılmış, gerçekleşen |
|---|---:|---:|---:|---:|
| Ev sahibi galibiyeti | <!-- metric:dev_cal_home_pred:.1%:tr -->%45,4<!-- /metric --> | <!-- metric:dev_cal_home_obs:.1%:tr -->%45,2<!-- /metric --> | <!-- metric:hold_cal_home_pred:.1%:tr -->%45,9<!-- /metric --> | <!-- metric:hold_cal_home_obs:.1%:tr -->%45,4<!-- /metric --> |
| Beraberlik | <!-- metric:dev_cal_draw_pred:.1%:tr -->%24,6<!-- /metric --> | <!-- metric:dev_cal_draw_obs:.1%:tr -->%25,4<!-- /metric --> | <!-- metric:hold_cal_draw_pred:.1%:tr -->%23,6<!-- /metric --> | <!-- metric:hold_cal_draw_obs:.1%:tr -->%26,1<!-- /metric --> |
| Deplasman galibiyeti | <!-- metric:dev_cal_away_pred:.1%:tr -->%30,0<!-- /metric --> | <!-- metric:dev_cal_away_obs:.1%:tr -->%29,4<!-- /metric --> | <!-- metric:hold_cal_away_pred:.1%:tr -->%30,5<!-- /metric --> | <!-- metric:hold_cal_away_obs:.1%:tr -->%28,4<!-- /metric --> |
| Maç başına gol | <!-- metric:dev_goals_pred:.3f:tr -->2,865<!-- /metric --> | <!-- metric:dev_goals_obs:.3f:tr -->2,816<!-- /metric --> | <!-- metric:hold_goals_pred:.3f:tr -->2,979<!-- /metric --> | <!-- metric:hold_goals_obs:.3f:tr -->2,802<!-- /metric --> |
| 3+ gol | <!-- metric:dev_ou25_pred:.1%:tr -->%54,3<!-- /metric --> | <!-- metric:dev_ou25_obs:.1%:tr -->%53,2<!-- /metric --> | <!-- metric:hold_ou25_pred:.1%:tr -->%56,5<!-- /metric --> | <!-- metric:hold_ou25_obs:.1%:tr -->%54,1<!-- /metric --> |
| Karşılıklı gol | <!-- metric:dev_btts_pred:.1%:tr -->%54,8<!-- /metric --> | <!-- metric:dev_btts_obs:.1%:tr -->%56,1<!-- /metric --> | <!-- metric:hold_btts_pred:.1%:tr -->%55,2<!-- /metric --> | <!-- metric:hold_btts_obs:.1%:tr -->%55,6<!-- /metric --> |

Yeniden kalibrasyon eğimleri (1, kalibre demektir; 1'in altı tahminlerin fazla yayıldığını gösterir): geliştirme sezonlarında ev sahibi <!-- metric:dev_cal_home_slope:.3f:tr -->0,996<!-- /metric --> ± <!-- metric:dev_cal_home_slope_se:.3f:tr -->0,064<!-- /metric -->, beraberlik <!-- metric:dev_cal_draw_slope:.3f:tr -->1,397<!-- /metric --> ± <!-- metric:dev_cal_draw_slope_se:.3f:tr -->0,247<!-- /metric -->, deplasman <!-- metric:dev_cal_away_slope:.3f:tr -->1,079<!-- /metric --> ± <!-- metric:dev_cal_away_slope_se:.3f:tr -->0,070<!-- /metric -->; ayrılmış sezonlarda <!-- metric:hold_cal_home_slope:.3f:tr -->0,894<!-- /metric --> ± <!-- metric:hold_cal_home_slope_se:.3f:tr -->0,110<!-- /metric -->, <!-- metric:hold_cal_draw_slope:.3f:tr -->0,614<!-- /metric --> ± <!-- metric:hold_cal_draw_slope_se:.3f:tr -->0,293<!-- /metric -->, <!-- metric:hold_cal_away_slope:.3f:tr -->1,163<!-- /metric --> ± <!-- metric:hold_cal_away_slope_se:.3f:tr -->0,130<!-- /metric -->. Hiçbiri 1'den ayırt edilemez. İki dönemde de beraberlikler az, goller fazla tahmin edilir; karşılıklı gol tahmininin genel orana göre becerisi yoktur.

Kalibrasyon kontrolü (`tests/calibration.rs`: sezon öncesindeki yayındaki harman, geçmişteki 4.588 maça karşı; `cargo test --test calibration -- --nocapture` ile yazdırılır, testte sabitlenir, metrik dosyasında yer almaz) aynı tabloyu bulur: ev sahibi galibiyeti maç tahminlerinde %44,9, gerçekte %45,4; beraberlik maç tahminlerinde %24,8, simüle edilen sezonlarda %24,4, gerçekte %25,9; deplasman galibiyeti %30,4, gerçekte %28,8; maç başına gol maç tahminlerinde 2,878, simüle edilen sezonlarda 2,915, gerçekte 2,787.

### Sezon düzeyi (sezon değerlendirmesi)

Sezonlar birleştirilmiştir; sıralama RPS'inde ve puan CRPS'inde düşük olan daha iyidir.

| | Geliştirme s0 | Geliştirme s33 | Geliştirme s67 | Ayrılmış s0 | Ayrılmış s33 | Ayrılmış s67 |
|---|---:|---:|---:|---:|---:|---:|
| Sıralama RPS | <!-- metric:season:dev_s0_pos_rps:.4f:tr -->0,1322<!-- /metric --> | <!-- metric:season:dev_s33_pos_rps:.4f:tr -->0,1121<!-- /metric --> | <!-- metric:season:dev_s67_pos_rps:.4f:tr -->0,0646<!-- /metric --> | <!-- metric:season:hold_s0_pos_rps:.4f:tr -->0,1283<!-- /metric --> | <!-- metric:season:hold_s33_pos_rps:.4f:tr -->0,0648<!-- /metric --> | <!-- metric:season:hold_s67_pos_rps:.4f:tr -->0,0485<!-- /metric --> |
| Puan CRPS (puan cinsinden) | <!-- metric:season:dev_s0_pts_crps:.2f:tr -->6,18<!-- /metric --> | <!-- metric:season:dev_s33_pts_crps:.2f:tr -->5,04<!-- /metric --> | <!-- metric:season:dev_s67_pts_crps:.2f:tr -->2,93<!-- /metric --> | <!-- metric:season:hold_s0_pts_crps:.2f:tr -->5,80<!-- /metric --> | <!-- metric:season:hold_s33_pts_crps:.2f:tr -->2,97<!-- /metric --> | <!-- metric:season:hold_s67_pts_crps:.2f:tr -->2,13<!-- /metric --> |
| Şampiyonluk, Brier becerisi | <!-- metric:season:dev_s0_title_bss:.3f:tr -->0,052<!-- /metric --> | <!-- metric:season:dev_s33_title_bss:.3f:tr -->0,307<!-- /metric --> | <!-- metric:season:dev_s67_title_bss:.3f:tr -->0,500<!-- /metric --> | <!-- metric:season:hold_s0_title_bss:.3f:tr -->0,717<!-- /metric --> | <!-- metric:season:hold_s33_title_bss:.3f:tr -->0,933<!-- /metric --> | <!-- metric:season:hold_s67_title_bss:.3f:tr -->0,581<!-- /metric --> |
| İlk dört, Brier becerisi | <!-- metric:season:dev_s0_top4_bss:.3f:tr -->0,430<!-- /metric --> | <!-- metric:season:dev_s33_top4_bss:.3f:tr -->0,584<!-- /metric --> | <!-- metric:season:dev_s67_top4_bss:.3f:tr -->0,813<!-- /metric --> | <!-- metric:season:hold_s0_top4_bss:.3f:tr -->0,610<!-- /metric --> | <!-- metric:season:hold_s33_top4_bss:.3f:tr -->0,900<!-- /metric --> | <!-- metric:season:hold_s67_top4_bss:.3f:tr -->0,933<!-- /metric --> |
| Küme düşme, Brier becerisi | <!-- metric:season:dev_s0_rel_bss:.3f:tr -->0,145<!-- /metric --> | <!-- metric:season:dev_s33_rel_bss:.3f:tr -->0,319<!-- /metric --> | <!-- metric:season:dev_s67_rel_bss:.3f:tr -->0,590<!-- /metric --> | <!-- metric:season:hold_s0_rel_bss:.3f:tr -->-0,101<!-- /metric --> | <!-- metric:season:hold_s33_rel_bss:.3f:tr -->0,520<!-- /metric --> | <!-- metric:season:hold_s67_rel_bss:.3f:tr -->0,687<!-- /metric --> |
| Şampiyona verilen olasılık | <!-- metric:season:dev_s0_p_champion:.0%:tr -->%19<!-- /metric --> | <!-- metric:season:dev_s33_p_champion:.0%:tr -->%38<!-- /metric --> | <!-- metric:season:dev_s67_p_champion:.0%:tr -->%57<!-- /metric --> | <!-- metric:season:hold_s0_p_champion:.0%:tr -->%63<!-- /metric --> | <!-- metric:season:hold_s33_p_champion:.0%:tr -->%82<!-- /metric --> | <!-- metric:season:hold_s67_p_champion:.0%:tr -->%62<!-- /metric --> |
| Sezon sonu puanı %80'lik aralığın içinde | %<!-- metric:season:dev_s0_cov80_pct:.1f:tr -->70,4<!-- /metric --> | %<!-- metric:season:dev_s33_cov80_pct:.1f:tr -->67,8<!-- /metric --> | %<!-- metric:season:dev_s67_cov80_pct:.1f:tr -->73,0<!-- /metric --> | %<!-- metric:season:hold_s0_cov80_pct:.1f:tr -->73,0<!-- /metric --> | %<!-- metric:season:hold_s33_cov80_pct:.1f:tr -->86,5<!-- /metric --> | %<!-- metric:season:hold_s67_cov80_pct:.1f:tr -->81,1<!-- /metric --> |
| Sezon sonu puanı %50'lik aralığın içinde | %<!-- metric:season:dev_s0_cov50_pct:.1f:tr -->42,1<!-- /metric --> | %<!-- metric:season:dev_s33_cov50_pct:.1f:tr -->42,8<!-- /metric --> | %<!-- metric:season:dev_s67_cov50_pct:.1f:tr -->50,0<!-- /metric --> | %<!-- metric:season:hold_s0_cov50_pct:.1f:tr -->48,6<!-- /metric --> | %<!-- metric:season:hold_s33_cov50_pct:.1f:tr -->75,7<!-- /metric --> | %<!-- metric:season:hold_s67_cov50_pct:.1f:tr -->51,4<!-- /metric --> |
| PIT standart sapması (tekdüze dağılımda 0,289) | <!-- metric:season:dev_s0_pit_sd:.3f:tr -->0,318<!-- /metric --> | <!-- metric:season:dev_s33_pit_sd:.3f:tr -->0,325<!-- /metric --> | <!-- metric:season:dev_s67_pit_sd:.3f:tr -->0,302<!-- /metric --> | <!-- metric:season:hold_s0_pit_sd:.3f:tr -->0,305<!-- /metric --> | <!-- metric:season:hold_s33_pit_sd:.3f:tr -->0,240<!-- /metric --> | <!-- metric:season:hold_s67_pit_sd:.3f:tr -->0,270<!-- /metric --> |
| Gerçekleşen hata / tahminin standart sapması | <!-- metric:season:dev_s0_pts_err_ratio:.2f:tr -->1,26<!-- /metric --> | <!-- metric:season:dev_s33_pts_err_ratio:.2f:tr -->1,30<!-- /metric --> | <!-- metric:season:dev_s67_pts_err_ratio:.2f:tr -->1,17<!-- /metric --> | <!-- metric:season:hold_s0_pts_err_ratio:.2f:tr -->1,27<!-- /metric --> | <!-- metric:season:hold_s33_pts_err_ratio:.2f:tr -->0,82<!-- /metric --> | <!-- metric:season:hold_s67_pts_err_ratio:.2f:tr -->0,85<!-- /metric --> |
| Puan tablosunun standart sapması, simüle / gerçek | <!-- metric:season:dev_s0_table_sd_sim:.1f:tr -->13,6<!-- /metric --> / <!-- metric:season:dev_s0_table_sd_actual:.1f:tr -->15,1<!-- /metric --> | <!-- metric:season:dev_s33_table_sd_sim:.1f:tr -->14,3<!-- /metric --> / <!-- metric:season:dev_s33_table_sd_actual:.1f:tr -->15,1<!-- /metric --> | <!-- metric:season:dev_s67_table_sd_sim:.1f:tr -->14,4<!-- /metric --> / <!-- metric:season:dev_s67_table_sd_actual:.1f:tr -->15,1<!-- /metric --> | <!-- metric:season:hold_s0_table_sd_sim:.1f:tr -->15,2<!-- /metric --> / <!-- metric:season:hold_s0_table_sd_actual:.1f:tr -->16,4<!-- /metric --> | <!-- metric:season:hold_s33_table_sd_sim:.1f:tr -->16,8<!-- /metric --> / <!-- metric:season:hold_s33_table_sd_actual:.1f:tr -->16,4<!-- /metric --> | <!-- metric:season:hold_s67_table_sd_sim:.1f:tr -->17,4<!-- /metric --> / <!-- metric:season:hold_s67_table_sd_actual:.1f:tr -->16,4<!-- /metric --> |

Geliştirme sezonlarında tahminler her anlık görüntüde aşırı özgüvenlidir: %80'lik aralıkların kapsama oranı için güven aralıkları (`season_eval` yazdırır, denetlenmez) 63,6–76,7, 60,3–76,0 ve 66,9–79,1'dir; üçü de 80'in altında kalır. Sezon öncesi şampiyonluk tahmini genel orandan ayırt edilemez (Brier becerisi için aralık −0,099 ile 0,209). İki ayrılmış sezon s33'ten itibaren aşırı özgüvenli değildir; ama iki sezon hiçbir şeyi kesinleştirmez ve `RATING_SIGMA` da onların yayılımına göre seçilmiştir.

### Piyasaya karşı (çevrimdışı)

Yalnızca çevrimdışı değerlendirme: bu oranların ve rakamların hiçbiri uygulamaya, API'sine ya da arayüzüne ulaşmaz. Piyasa çizgisi, MIT arşivindeki Bet365 maç öncesi maç sonucu ve 2,5 alt/üst oranlarıdır (arşivde kapanış oranları yoktur); marj, hiçbir sonuca bakılmadan seçilen Shin yöntemiyle çıkarılır. Orantılı normalleştirme ve yaklaşık 17 şirket arasındaki en iyi oran yalnızca kontrol içindir; alt/üst tarafında aynı iki yöntem iki orana uygulanır. Model − piyasa pozitifse piyasanın kaybı daha düşüktür. Aralıklar yazdırılan aralıklardır, denetlenmez.

| | Geliştirme | Ayrılmış |
|---|---:|---:|
| Oranı olan maçlar | 2.717 maçtan <!-- metric:market:dev_n:,d:tr -->2.714<!-- /metric --> | 647 maçtan <!-- metric:market:hold_n:,d:tr -->620<!-- /metric --> (arşivin 2024-25 boşluğu) |
| Maç sonucu log kaybı, model / piyasa | <!-- metric:market:dev_model_1x2_ll:.4f:tr -->0,9993<!-- /metric --> / <!-- metric:market:dev_market_1x2_ll:.4f:tr -->0,9888<!-- /metric --> | <!-- metric:market:hold_model_1x2_ll:.4f:tr -->0,9754<!-- /metric --> / <!-- metric:market:hold_market_1x2_ll:.4f:tr -->0,9425<!-- /metric --> |
| Model − piyasa, maç sonucu log kaybı | <!-- metric:market:dev_diff_1x2_ll:+.4f:tr -->+0,0106<!-- /metric --> [+0,00465; +0,01661], sezon t [+0,00588; +0,01522] | <!-- metric:market:hold_diff_1x2_ll:+.4f:tr -->+0,0328<!-- /metric --> [+0,01744; +0,05018], sezon t [+0,00392; +0,06173] |
| Model − piyasa, RPS | <!-- metric:market:dev_diff_rps:+.4f:tr -->+0,0032<!-- /metric --> (bootstrap aralığı 0'ı içermez) | <!-- metric:market:hold_diff_rps:+.4f:tr -->+0,0111<!-- /metric --> (bootstrap aralığı 0'ı içermez, tek serbestlik dereceli sezon t aralığı içerir) |
| Model − piyasa, 3+ gol | <!-- metric:market:dev_diff_ou25_ll:+.4f:tr -->+0,0063<!-- /metric --> (bootstrap aralığı 0'ı içermez, sezon t aralığı içerir) | <!-- metric:market:hold_diff_ou25_ll:+.4f:tr -->+0,0092<!-- /metric --> (anlamlı değil) |
| Bet365 marjı, maç sonucu | <!-- metric:market:dev_b365_margin:.1%:tr -->%6,4<!-- /metric --> | <!-- metric:market:hold_b365_margin:.1%:tr -->%7,2<!-- /metric --> |
| En iyi oranların ima ettiği olasılıkların toplamı 1 ya da altında kalan maçlar, maç sonucu | <!-- metric:market:dev_max_book_le1_1x2:,d:tr -->580<!-- /metric --> (%21,4) | <!-- metric:market:hold_max_book_le1_1x2:,d:tr -->26<!-- /metric --> (%4,2) |

Orantılı ve en iyi oran çizgileri her ölçütte Shin çizgisiyle 0,002 içinde uyuşur. En iyi oranların ima ettiği olasılıklar toplamda 1'i geçmediğinde (bir şirketin güncellenmemiş oranı yüzünden), Shin yönteminin çözümü yoktur ve yerine orantılı çizgi geçer; bu maçların payı 2016-17'de %44,1 iken 2025-26'da %0,7'ye düşer (`market.rs` sezon sezon yazdırır). Birincil çizginin en iyi oran değil Bet365 olması bu gürültü yüzündendir. Modelin sabitleri geliştirme sezonlarında seçildi; bu orada modelin lehinedir, model yine de yenilir.

Kalibrasyon, oranı olan maçlarda model / piyasa / gerçekleşen: beraberlik geliştirme sezonlarında <!-- metric:market:dev_cal_draw_model:.1%:tr -->%24,6<!-- /metric --> / <!-- metric:market:dev_cal_draw_market:.1%:tr -->%25,2<!-- /metric --> / <!-- metric:market:dev_cal_draw_obs:.1%:tr -->%25,4<!-- /metric -->, ayrılmış sezonlarda <!-- metric:market:hold_cal_draw_model:.1%:tr -->%23,6<!-- /metric --> / <!-- metric:market:hold_cal_draw_market:.1%:tr -->%23,9<!-- /metric --> / <!-- metric:market:hold_cal_draw_obs:.1%:tr -->%26,6<!-- /metric -->; 3+ gol <!-- metric:market:dev_cal_over_model:.1%:tr -->%54,3<!-- /metric --> / <!-- metric:market:dev_cal_over_market:.1%:tr -->%53,6<!-- /metric --> / <!-- metric:market:dev_cal_over_obs:.1%:tr -->%53,2<!-- /metric --> ve <!-- metric:market:hold_cal_over_model:.1%:tr -->%56,5<!-- /metric --> / <!-- metric:market:hold_cal_over_market:.1%:tr -->%54,3<!-- /metric --> / <!-- metric:market:hold_cal_over_obs:.1%:tr -->%53,7<!-- /metric -->. Piyasanın beraberlikleri gerçeğe daha yakındır ve onun gol toplamları modelinkiler gibi şişkin değildir.

Hafta bandına göre (hafta = 1 + iki kulübün maçtan önce oynadığı ortalama maç sayısı), model − piyasa maç sonucu log kaybı ve bandın toplam farktaki payı:

| Bant | Geliştirme, maç | Geliştirme, fark | Geliştirme, pay | Ayrılmış, maç | Ayrılmış, fark | Ayrılmış, pay |
|---|---:|---:|---:|---:|---:|---:|
| 1-10 | <!-- metric:market:dev_md01_10_n:,d:tr -->766<!-- /metric --> | <!-- metric:market:dev_md01_10_diff_1x2_ll:+.4f:tr -->+0,0029<!-- /metric --> | <!-- metric:market:dev_md01_10_gap_share:.1%:tr -->%7,9<!-- /metric --> | <!-- metric:market:hold_md01_10_n:,d:tr -->171<!-- /metric --> | <!-- metric:market:hold_md01_10_diff_1x2_ll:+.4f:tr -->+0,0401<!-- /metric --> | <!-- metric:market:hold_md01_10_gap_share:.1%:tr -->%33,7<!-- /metric --> |
| 11-22 | <!-- metric:market:dev_md11_22_n:,d:tr -->912<!-- /metric --> | <!-- metric:market:dev_md11_22_diff_1x2_ll:+.4f:tr -->+0,0135<!-- /metric --> | <!-- metric:market:dev_md11_22_gap_share:.1%:tr -->%42,9<!-- /metric --> | <!-- metric:market:hold_md11_22_n:,d:tr -->211<!-- /metric --> | <!-- metric:market:hold_md11_22_diff_1x2_ll:+.4f:tr -->+0,0059<!-- /metric --> | <!-- metric:market:hold_md11_22_gap_share:.1%:tr -->%6,1<!-- /metric --> |
| 23'ten sona | <!-- metric:market:dev_md23_end_n:,d:tr -->1.036<!-- /metric --> | <!-- metric:market:dev_md23_end_diff_1x2_ll:+.4f:tr -->+0,0136<!-- /metric --> | <!-- metric:market:dev_md23_end_gap_share:.1%:tr -->%49,2<!-- /metric --> | <!-- metric:market:hold_md23_end_n:,d:tr -->238<!-- /metric --> | <!-- metric:market:hold_md23_end_diff_1x2_ll:+.4f:tr -->+0,0515<!-- /metric --> | <!-- metric:market:hold_md23_end_gap_share:.1%:tr -->%60,2<!-- /metric --> |

Farkın 10. haftadan sonra açılıp açılmadığını soran karşıtlık (10. haftadan sonraki ortalama fark eksi 1-10. haftalardaki ortalama fark) iki dönemde de sıfırdan ayırt edilemez: geliştirme <!-- metric:market:dev_md_late_minus_early_1x2_ll:+.4f:tr -->+0,0106<!-- /metric --> [−0,00221; +0,02350] (sezon t [−0,00099; +0,02221]), ayrılmış <!-- metric:market:hold_md_late_minus_early_1x2_ll:+.4f:tr -->-0,0100<!-- /metric --> [−0,04269; +0,02268].

Tahmin kapsama (forecast encompassing): (1 − w) · piyasa + w · model karışımında modelin en çok olabilirlikli ağırlığı geliştirme sezonlarında <!-- metric:market:dev_pool_w:.3f:tr -->0,073<!-- /metric --> çıkar (sezon kümeli bootstrap [0,000; 0,208]; sezonların kendi w değerleri 0 ile 0,48 arasında), örneklem içi kazanç 0,00007'dir. Bu ağırlıkta karışım, ayrılmış sezonlarda tek başına piyasadan <!-- metric:market:hold_pool_diff_1x2_ll:+.4f:tr -->+0,0012<!-- /metric --> [+0,00012; +0,00228] daha kötüdür. Piyasanın bilmediği bir bilgi örneklem dışında görünmüyor; bu örneklem büyüklükleriyle bu, modelin hiç taşımadığını göstermez.

### Canlı sezon: 2026-27

19 Ekim 2026'da 306 maçın 63'ü oynanmıştı. İzleme o güne kadar 55 dondurulmuş tahmini puanlamıştı ve hepsini `2026-08` modeli yapmıştı: genel oranlara göre log kaybı farkı −0,054, %95 aralık −0,123 ile +0,015, maçlar bağımsız sayılarak (o günkü `GET /api/accuracy`; denetlenmez). `2026-10b` modelinin henüz puanlanmış bir tahmini yok.

Piyasaya karşı: <!-- metric:market:live_n -->19<!-- /metric --> oynanmış maçın hem dondurulmuş bir tahmini hem Bet365 oranı var (hepsi `2026-08` tahmini; arşivin 2026-27 satırları 31 Ağustos 2026'da bitiyor): model <!-- metric:market:live_model_1x2_ll:.4f:tr -->1,0014<!-- /metric -->, piyasa <!-- metric:market:live_market_1x2_ll:.4f:tr -->0,9981<!-- /metric -->, model − piyasa <!-- metric:market:live_diff_1x2_ll:+.4f:tr -->+0,0033<!-- /metric --> (normal aralık [−0,09021; +0,09680]). Her tahmin, oran satırındaki maç tarihinden <!-- metric:market:live_age_days_min:d:tr -->0<!-- /metric --> ile <!-- metric:market:live_age_days_max:d:tr -->11<!-- /metric --> gün (ortanca <!-- metric:market:live_age_days_median:d:tr -->4<!-- /metric -->) önce donduruldu; yani maç öncesi fiyatın bildiğinden daha azını biliyor olabilir, bu da modeli dezavantajlı kılar. <!-- metric:market:live_no_forecast:d:tr -->8<!-- /metric --> oran satırının dondurulmuş tahmini yok (ilk tahmin kaydından önce oynanan 1. hafta) ve puanlanan <!-- metric:market:live_no_odds:d:tr -->36<!-- /metric --> tahminin oran satırı yok. Henüz bir sonuç çıkarmak mümkün değil.

## Olumsuz sonuçlar

- **Ekim 2026 ayarı ayrılmış sezonlarda anlamlı değildir.** Sezon içi K'yı 20'den 60'a çıkarıp `D_DIV`'i 1600'den 1200'e indirmek ve güçsüz taraf düzeltmesini eklemek, geliştirme sezonlarında her ölçütü iyileştirdi (maç sonucu −0,0077 [−0,0122; −0,0031]); ayrılmış sezonlarda ise yalnızca karşılıklı gol anlamlı biçimde iyileşti (−0,0044 [−0,0081; −0,0008]), maç sonucundaki değişim −0,0066 [−0,0170; +0,0043] oldu. Ayrı ayrı ölçüldüğünde maç sonucundaki kazancı K ve `D_DIV` değişikliği taşır ve bu değişiklik ayrılmış sezonlarda karşılıklı gol tahminini kötüleştirir; güçsüz taraf düzeltmesi yalnızca karşılıklı gol tahminine yarar. (`examples/compare.rs`; denetlenmez.)
- **4. aşamadaki maç taraması yayına bir şey çıkarmadı.** Gol düzeyi, `BASE`, ρ ve harman ağırlıklarından oluşan 202 geliştirme ayarının en iyisi üretimdeki modeli −0,0006 [−0,0015; +0,0003] farkla geçti; iç içe, her seferinde bir sezonu dışarıda bırakan seçimle seçildiğinde ise +0,0002 aldı. Bir gol ölçeği ya da `BASE` 1,30, geliştirme sezonlarındaki gol farkını kapatır ama karşılıklı gol tahminine zarar verir ve bu seçimden sağ çıkmaz; −0,10 civarında sabit bir ρ, maç sonucunda gürültü içinde kalan bir değişiklik karşılığında geliştirme sezonlarının beraberliklerini %25,3'e getirirdi (gerçekleşen <!-- metric:dev_cal_draw_obs:.1%:tr -->%25,4<!-- /metric -->). Hiçbir ayar karşılıklı gol tahminine beceri kazandırmaz. (Tek seferlik tarama; denetlenmez.)
- **Sezon şokunu modelin tamamına uygulamak veto edildi.** 75 puanlık şoku harmanın tamamına uygulamak geliştirme sezonlarını nominal kapsamaya getirdi ve birleştirilmiş sıralama RPS'ini 8 sezonun 7'sinde iyileştirdi; ama puan CRPS'indeki kazancın aralığı sıfırı içeriyordu ve yöntem 47'den fazla ayar arasından seçilmişti. Ayrılmış sezonlar için kural (çalıştırmadan önce çalışma notlarına yazıldı, depoya ise sonuçtan sonra işlendi), birleştirilmiş puan CRPS'i ya da sıralama RPS'i kötüleşmedikçe yöntemin benimsenmesini söylüyordu; ikisi de kötüleşti (puan CRPS'i +0,064, 2 sezonun hiçbirinde daha iyi değil; sıralama RPS'i +0,00039, çok küçük ve tohuma duyarlı). Üretim Elo şokunu korur. Sezon içinde sıcak güncellemeler ve haftalık sürüklenme de denendi, yayına alınmadı. (`season_eval` ve `compare --season`; denetlenmez.)
- **Piyasa iki dönemde de modeli yener** ve modeli piyasaya karıştırmak örneklem dışında işe yaramaz (yukarıda).
- **Kaynağı olmayan bir gerekçe.** Ayar, kapanış oranlarıyla aradaki farkın çoğunun 10. haftadan sonra açıldığı iddiasına dayanıyordu. Depoda bu iddianın bir kaynağı yoktu ve piyasa karşılaştırması onu ne doğrular ne çürütür: erken-geç karşıtlığı iki dönemde de sıfırdan ayırt edilemez, ayrıca karşılaştırma kapanış değil maç öncesi oranları kullanır.
- **Model seçimi.** Arenada (`examples/arena.rs`, sezon boyunca puanları dondurur; denetlenmez) yayındaki harman ayrılmış iki sezonun hiçbirinde birinci olmaz, ama her sezonun birincisi öbür sezonda ondan kötüdür. İleriye doğru ilerleyen lojistik bir yığın test sezonunda onu az farkla geçer, doğrulama sezonunda geride kalır; gradyan artırma, rastgele orman ve yapay sinir ağları (bir yan deney; düzeltilmiş geçmişle yeniden çalıştırılmadı) 2025-26'da onun gerisinde kaldı: yalnızca sonuçlara dayanan değişkenler makine öğrenmesine pek yer bırakmaz.
- **Denendi ve reddedildi** (yeni bir fikir olmadan yeniden çalıştırmayın): yükselen kulüpler için ayrı başlangıç puanları, sezon öncesi puanları ortalamaya çekmek ya da aralarını açmak, sezon içinde haftalık Dixon–Coles ve pi-rating yeniden kalibrasyonları, sezonun ilk haftaları için ayrı K değerleri, sezonun evresine göre değişen harman ağırlıkları, logaritmik, doğrusal ya da hiç olmayan gol farkı çarpanları, gol farkına dayalı Elo, geometrik harmanlama, modelin ürettiği gol toplamları, pi-rating öğrenme hızında değişiklikler ve maçın dengesine bağlı bir beraberlik artırımı (geliştirme sezonlarında kazandı, ayrılmış sezonlarda kaybetti).

## Bilinen sınırlılıklar

- **Beraberlikler az tahmin edilir** (geliştirme sezonlarında tahmin <!-- metric:dev_cal_draw_pred:.1%:tr -->%24,6<!-- /metric -->, gerçekleşen <!-- metric:dev_cal_draw_obs:.1%:tr -->%25,4<!-- /metric -->; ayrılmış sezonlarda tahmin <!-- metric:hold_cal_draw_pred:.1%:tr -->%23,6<!-- /metric -->, gerçekleşen <!-- metric:hold_cal_draw_obs:.1%:tr -->%26,1<!-- /metric -->); eksik beraberlikler deplasman galibiyetine gider.
- **Goller fazla tahmin edilir**; ayrılmış sezonlarda (tahmin <!-- metric:hold_goals_pred:.3f:tr -->2,979<!-- /metric -->, atılan <!-- metric:hold_goals_obs:.3f:tr -->2,802<!-- /metric -->) geliştirme sezonlarından (tahmin <!-- metric:dev_goals_pred:.3f:tr -->2,865<!-- /metric -->, atılan <!-- metric:dev_goals_obs:.3f:tr -->2,816<!-- /metric -->) daha çok. Gol düzeyi canlı sezon için açık bir konudur.
- **Karşılıklı gol tahmininin becerisi yoktur**: goller iki taraf arasında fazla dengesiz paylaştırılır; bu, ayarlanacak bir sabit değil, yapısal bir sorundur.
- **Sezon aralıkları geliştirme sezonlarında aşırı özgüvenlidir** (%80'lik aralıklar kulüplerin yaklaşık %70'ini içerir), **ayrılmış sezonlarda ise** sezonun üçte birinden sonra **değildir**; iki ayrılmış sezon hangisinin geçerli olduğunu kesinleştiremez ve `RATING_SIGMA` onların yayılımına göre seçildi.
- **Piyasa modeli yener** ve buradaki oranlar kapanış değil maç öncesi oranlarıdır: kapanış çizgisiyle aradaki fark büyük olasılıkla daha geniştir. Kapanış oranlarıyla (football-data.co.uk) karşılaştırma ileride yapılacak bir iştir ve www.football-data.co.uk adresine ağ erişimi gerektirir.
- **Canlı örneklem küçüktür**: 55 puanlanmış tahmin, hepsi daha eski bir modelden; bunların 19'unun oranı var.
- **Eşitlik bozma varsayımları.** İkili averaj, puanı eşit kulüplerden oluşan her blok için bir kez uygulanır, hiçbir zaman yinelemeli olarak değil; yönetmelik bunu belirtmez. Sezon içindeki puan durumu, puanı eşit bir blokta ikili averajı ancak bloktaki her ikili iki maçını da oynadıktan sonra uygular; puan durumu genellikle böyle yayımlanır, ama yönetmelik yalnızca sezon sonundaki sıralamayı belirlediğinden bu da bir varsayımdır.
- **Yükselen kulüpleri** yalnızca Elo puanları ayırır; Dixon–Coles ve pi-rating sezon boyunca sabit kalır.
- **Yalnızca sonuçlar**: ilk on birler, sakatlıklar, transferler ya da xG yok.

## Etik değerlendirmeler

- **Kumar.** Futbol maçı tahminleri bahis tüyosu gibi okunabilir. Bu model öyle bir şey değildir: bahis şirketi oranlarına yenilir, uygulama hiçbir bahis şirketi oranı ya da piyasa rakamı göstermez ve piyasa karşılaştırması yalnızca modeli çevrimdışı ölçmek için vardır. Uygulamadaki bahis şirketi özellikleri (lisanslı bir operatörün bülteninin çekilmesi, modelle bahis şirketi arasındaki fark ve günlük kupon) 12 Ekim 2026'da sahibinin talimatıyla kaldırıldı ([CLAUDE.md, "No licensed betting sources"](../CLAUDE.md#no-licensed-betting-sources), İngilizce).
- **Veri.** Yalnızca maç sonuçları; kişisel veri yok. Üçüncü tarafların verileri kendi lisanslarını korur (`THIRD_PARTY_NOTICES.md`).

## Yeniden üretme

```bash
cargo test                                        # birim testleri ve tests/calibration.rs
EVAL_HOLDOUT=1 EVAL_METRICS=data/metrics/backtest.json cargo run --release --example backtest
EVAL_HOLDOUT=1 SEASON_METRICS=data/metrics/season.json cargo run --release --example season_eval
# Piyasa karşılaştırması: oranlar ağdan gelir ve depoya hiç eklenmez.
python3 scripts/fetch_fixtures.py                 # önce bu: canlı eşleştirme oynanan her maçı görsün
python3 scripts/fetch_odds.py
EVAL_HOLDOUT=1 EVAL_DUMP=/tmp/prod.csv cargo run --release --example backtest
MARKET_METRICS=data/metrics/market.json cargo run --release --example market -- /tmp/prod.csv
# İki geriye dönük test ya da iki sezon dökümünün eşleştirilmiş karşılaştırması.
cargo run --release --example compare -- A.csv B.csv
cargo run --release --example compare -- --season A.csv B.csv
# Aktarılan her rakam, data/metrics/ ile karşılaştırılır.
python3 scripts/check_metrics.py docs
```

**Rakam denetimi.** CI geriye dönük testi ve sezon değerlendirmesini yeniden çalıştırır ve `data/metrics/backtest.json` ile `season.json` dosyalarının birebir tutmasını ister; piyasa karşılaştırmasını yeniden çalıştıramaz, bu yüzden `market.json` elle yeniden üretilir. `check_metrics.py docs` ardından README.md, README.tr.md, CLAUDE.md, İngilizce model kartı (`docs/model-card.md`) ve bu karttaki her işaretçiyi depodaki dosyalarla karşılaştırır. Bir maç tahminini oynatan değişiklik model etiketini de yükseltir (`src/model_version.rs`); bir tahmini oynatan her değişiklikte metrik dosyaları ve burada aktarılan rakamlar yeniden üretilir. Ayrıntılar: [README.tr.md, "Doğrulama"](../README.tr.md#doğrulama); İngilizce olarak [CLAUDE.md, "Numbers gate"](../CLAUDE.md#walk-forward-backtest-examplesbacktestrs) ve ["Forecast accuracy"](../CLAUDE.md#forecast-accuracy-srcaccuracyrs-exampleslog_forecastrs).
