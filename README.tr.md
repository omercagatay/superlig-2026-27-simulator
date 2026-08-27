# Süper Lig 2026-27 Simülatörü

[English](README.md) | [Türkçe](README.tr.md)

[![CI](https://github.com/omercagatay/superlig-2026-27-simulator/actions/workflows/ci.yml/badge.svg)](https://github.com/omercagatay/superlig-2026-27-simulator/actions/workflows/ci.yml)

2026-27 Trendyol Süper Lig sezonu için uçtan uca Monte Carlo tahmin uygulaması. Rust ile yazılmış bir simülasyon motoru, React tabanlı bir kontrol paneli, Türkiye Futbol Federasyonu'ndan canlı sonuçlar ve isteğe bağlı olarak Kimi destekli bir senaryo çözümleyicisi bir arada çalışır.

> Bu projenin ürettiği olasılıklar ve adil oranlar model tahminidir; bahis ya da yatırım tavsiyesi değildir.

## Öne çıkanlar

- 18 kulübün tamamı arasındaki 306 maçlık, 34 haftalık çift devreli fikstürün tamamını simüle eder. Rayon ile paralel olarak 100–200.000 sezon; panelde varsayılan 50.000.
- Beklenen gol tahminlerinde Elo, Dixon–Coles ve pi-rating modellerini harmanlar. Son ikisi 14 sezonluk gerçek Süper Lig sonuçları üzerine kalibre edilmiştir.
- Skorları Dixon–Coles ortak dağılımından çeker; bu, az gollü ve berabere biten maçları bağımsız Poisson'lara göre daha iyi temsil eder.
- TFF'nin resmî fikstüründeki oynanmış maçları sabitler ve yalnızca kalan maçları simüle eder.
- Süper Lig'in kendi sıralama kurallarını uygular: **averaj değil, önce ikili averaj (head-to-head)** belirleyicidir.
- Şampiyonluk, ilk iki, tam üçüncülük/dördüncülük, ilk dört ve küme düşme olasılıkları; 18×18 sıralama ızgarası; tahminî final tablosu; gelecek hafta tahminleri ve "kim kimin üstünde bitirir" olasılıkları üretir.
- Bağımsız modeli güncel İddaa 1X2 oranlarıyla karşılaştıran, bütün eşikleri aşan seçim yoksa zorla tahmin üretmeyen kontrollü bir **Günün Kuponu** görünümü sunar.
- Doğal dildeki senaryoları Kimi ile doğrulanmış Elo değişikliklerine çevirir ve sezonu yeniden simüle eder.
- IP başına hız sınırı, istek doğrulama, deterministik tohum (seed), açık/koyu tema, Docker desteği ve GitHub Actions CI içerir.

## Teknoloji yığını

| Katman | Teknoloji |
|---|---|
| Sunucu/API | Rust 1.87+, Axum, Tokio |
| Simülasyon | Rayon, Rand, Dixon–Coles, pi-rating, Elo/Poisson |
| Arayüz | React 18, TypeScript, Vite |
| Canlı veri | Türkiye Futbol Federasyonu (tff.org) fikstür sayfası |
| Piyasa oranları | Nesine İddaa maç önü bülteni (sezon modeline girmez; karşılaştırma ve kupon süzme için kullanılır) |
| Geçmiş veri | MIT lisanslı Club Football Match Data (2012-13 – 2024-25) ve resmî TFF sonuçları (2025-26) |
| Senaryo analizi | Moonshot API üzerinden Kimi |
| Dağıtım | Çok aşamalı Docker imajı; Railway uyumlu |

## Model nasıl çalışıyor

Saf Elo bileşeni, puan farkını ve ev sahibi avantajını beklenen gole çevirir:

```text
lambda_ev      = 1,35 × 10^(( Elo_ev - Elo_deplasman + 80) / 1600)
lambda_deplasman = 1,35 × 10^((-Elo_ev + Elo_deplasman - 80) / 1600)
```

Ev sahipliği avantajı kulübün değil, **maçın** bir özelliğidir: çift devreli fikstürün her iki yarısında da her kulüp kendi sahasında bu avantajı alır. (Dünya Kupası sürümünde bu, kulüp başına sabit bir "ev sahibi ülke" bayrağıydı; ligde bu yapı anlamsız olduğu için maç başına role dönüştürüldü.)

Varsayılan olarak bu oranlar, gerçek Süper Lig geçmişiyle eğitilmiş iki modelle harmanlanır:

- **Elo (0,5):** ClubElo ölçeğinde güncel kulüp gücü ve 80 puanlık ev sahibi düzeltmesi.
- **Dixon–Coles (0,3):** dört yıllık yarılanma ömrüyle zaman ağırlıklandırılmış hücum/savunma güçleri ve az gollü skor korelasyonu; gerçek tarihleri korunan 4.590 maç üzerinde kalibre edilmiştir.
- **Pi-rating (0,2):** aynı geçmiş üzerinde ardışık ev/deplasman güç güncellemeleri.

Harmanı değiştirmek için `ENSEMBLE_WEIGHTS` kullanılır; `1,0,0` saf Elo modelini seçer. Dixon–Coles ağırlığı etkinken skorlar onun ortak dağılımından örneklenir.

1. Lig'den yükselen kulüplerin (2026-27 için Amedspor ve Çorum) üst lig geçmişi olmadığından Dixon–Coles ve pi-rating bileşenleri onlara lig ortalaması bir profil verir; ayrımı yalnızca Elo puanları yapar.

Elle yapılan değişiklikler ve Kimi senaryoları yalnızca Elo bileşenini günceller. Gömülü Dixon–Coles ve pi-rating parametreleri, modeller açıkça yeniden kalibre edilene kadar değişmez.

### Sıralama kuralları

Puan durumu şu sırayla belirlenir:

1. Puan
2. İkili maçlardaki puan
3. İkili maçlardaki averaj
4. İkili maçlarda atılan gol
5. Genel averaj
6. Genel atılan gol
7. Play-off

İkili maç sonuçları averajdan **önce** gelir; bu, FIFA/UEFA grup aşaması sıralamasından farklıdır ve dönüşümün en kritik ayrıntısıdır.

İkili karşılaştırma, puanı eşit kulüplerden oluşan her blok için **bir kez** uygulanır. Bu turdan sonra hâlâ eşit kalan kulüpler, kalanlar arasında yeni bir mini puan tablosuna değil, doğrudan genel averaja düşer. Yayımlanan kural bu durumu açıkça belirtmediği için bu bir modelleme varsayımıdır ve kendi testiyle korunmaktadır (`head_to_head_is_applied_once_not_recursively`).

Panel ilk iki, üçüncü, dördüncü ve ilk dört gibi kesin lig sıralamalarını gösterir; bu sıralara doğrudan UEFA turnuvası adı vermez. Gerçek katılım Türkiye Kupası şampiyonuna ve o sezonun UEFA erişim listesine de bağlıdır. Son üç takım küme düşer.

### Günün Kuponu ve sorumlu oyun

**Günün Kuponu** görünümü, önce modelin bahis oranlarından bağımsız hesaplanan 1X2 olasılıklarını üretir; ardından bunları güncel yasal piyasa görüntüsüyle karşılaştırır. Her maçtan en fazla bir seçim alınır ve seçim ancak şu koşulların tamamını sağlarsa gösterilir:

- model olasılığı en az %30;
- model olasılığı, marjı çıkarılmış piyasa olasılığından en az 2 puan yüksek;
- model olasılığı × güncel ondalık oran en az 1,02;
- güncel oran en fazla 4,00.

Sıradaki aktif hafta değerlendirilir ve en fazla üç seçim döner. 90 dakikadan eski oranlar reddedilir. Hiçbir seçim uygun değilse API `no_value` döndürür ve arayüz kupon göstermediğini açıkça belirtir. Toplam model yüzdesi yalnızca tekil tahminlerin çarpımıdır; sonuçları yaklaşık olarak bağımsız varsayar ve garanti değildir.

Oranlar halka açık [Nesine İddaa bülteninden](https://www.nesine.com/iddaa) alınır. Arayüz yalnızca 27 Ağustos 2026 tarihinde kendi sayfalarında Spor Toto'nun yasal bayisi olduklarını belirten birinci taraf sitelere bağlantı verir: [Nesine](https://www.nesine.com/), [Bilyoner](https://www.bilyoner.com/), [Misli](https://www.misli.com/hakkimizda), [Oley](https://www.oley.com/hakkimizda), [Birebin](https://www.birebin.com/) ve [iddaa.com](https://www.iddaa.com/yardim/detay/neden-bayi-secmeliyim-29874). Düzenleyici çerçeve ve resmî oyun planları [Spor Toto Teşkilat Başkanlığı](https://www.sportoto.gov.tr/) tarafından yayımlanır.

Bu projenin bayilerle reklam ortaklığı yoktur; bahis iletmez ve ödeme işlemez. Özellik yalnızca 18 yaş ve üzeri yetişkinler içindir ve deneysel bilgi sunar—bahis tavsiyesi veya kazanç vaadi değildir. Sınır belirleyin ve kayıplarınızın peşinden gitmeyin. Kumar kaynaklı zararlar için [YEDAM](https://www.yedam.org.tr/bagimlilik-turleri/kumar-bagimliligi) **115** hattından ücretsiz destek verir.

### Kalibrasyon

Elo sabitleri devralınmaz, gerçek lig verisine karşı doğrulanır:

| | Gerçek (14 sezon) | Simülasyon |
|---|---:|---:|
| Maç başına ev sahibi golü | 1,566 | — |
| Maç başına deplasman golü | 1,222 | — |
| Ev sahibi galibiyeti | %45,3 | %44,7 |
| Beraberlik | %25,8 | %24,5 |

Bu değerler birbirinden uzaklaşırsa `tests/calibration.rs` derlemeyi düşürür. Beraberlik oranındaki ~2 puanlık fark, bağımsız Poisson örneklemesinin bilinen bir zayıflığıdır; Dixon–Coles ağırlığı etkinken ρ düzeltmesi bunu telafi eder.

## Yerelde çalıştırma

### Gereksinimler

- Rust 1.87 veya üzeri
- Node.js 20.19 veya üzeri ve npm

Temel simülatör için API anahtarı gerekmez. `KIMI_API_KEY` yalnızca doğal dil senaryoları için gereklidir.

### Geliştirme modu

Vite geliştirme sunucusu `/api` isteklerini 3001 portuna yönlendirir; bu nedenle arka ucu o portta çalıştırın.

Terminal 1:

```bash
git clone https://github.com/omercagatay/superlig-2026-27-simulator.git
cd superlig-2026-27-simulator
cp .env.example .env
PORT=3001 cargo run --release
```

Terminal 2:

```bash
cd superlig-2026-27-simulator/frontend
npm ci
npm run dev
```

<http://localhost:5173> adresini açın. İlk tahmin kendiliğinden başlar.

### Üretime yakın yerel derleme

Önce arayüzü derleyin; Axum `frontend/dist` klasörünü API ile birlikte 3000 portundan sunar.

```bash
cd frontend
npm ci
npm run build
cd ..
cargo run --release
```

<http://localhost:3000> adresini açın.

## Yapılandırma

`.env.example` dosyasını `.env` olarak kopyalayın ve gerektiği gibi düzenleyin:

| Değişken | Varsayılan | Amaç |
|---|---:|---|
| `KIMI_API_KEY` | tanımsız | `/api/scenario` uç noktasını etkinleştirir; anahtarı Moonshot platformundan alın. |
| `PORT` | `3000` | Arka uç HTTP portu. Vite geliştirme sunucusuyla `3001` kullanın. |
| `RUST_LOG` | `superlig_sim=info` | Rust tracing filtresi. |
| `LIVE_REFRESH_MINUTES` | `30` | TFF yenileme aralığı; `0` arka plan yenilemesini kapatır. |
| `MAX_CONCURRENT_SIMULATIONS` | `1` | Aynı anda çalışan Rayon simülasyonlarına genel sınır; fazlası HTTP 429 alır. |
| `ENSEMBLE_WEIGHTS` | `0.5,0.3,0.2` | Virgülle ayrılmış Elo, Dixon–Coles ve pi-rating ağırlıkları. |
| `TRUST_PROXY` | `0` | `X-Forwarded-For` başlığına yalnızca temizleyici bir ters vekil sunucu arkasında güvenin. |

## Panelin kullanımı

1. Simülasyon sayısını ve tohumu seçip **Run** düğmesine basın. Aynı tohum, aynı yapılandırmayı yeniden üretilebilir kılar.
2. Altı sekmeyi inceleyin: **Forecast**, **Positions**, **Races**, **Table**, **Günün kuponu** ve **Live**.
3. En güncel TFF sonuçlarını çekip tahmini, maç kartlarını ve isabet görünümünü yeniden hesaplamak için **Update live data** düğmesini kullanın.
4. `Galatasaray'ın ilk kalecisi beş maç cezalı` gibi bir senaryo yazın. Kimi etkiyi açıklar, doğrulanmış kulüp puanları döndürür ve yeni bir simülasyon başlatır.

## API

| Uç nokta | Yöntem | IP başına sınır | Açıklama |
|---|---|---:|---|
| `/api/health` | `GET` | — | Servis sürümü, model yapılandırması ve son canlı yenileme. |
| `/api/simulate` | `POST` | 30/dk | İsteğe bağlı Elo değişiklikleriyle temel simülasyon. |
| `/api/scenario` | `POST` | 10/dk | İstemi Kimi ile çözümler ve dönen Elo değişiklikleriyle yeniden çalıştırır. |
| `/api/refresh` | `POST` | 5/dk | Güncel TFF sonuçlarını çeker ve uygular. |
| `/api/live` | `GET` | — | Önbellekteki son canlı veri anlık görüntüsünü döndürür. |
| `/api/upcoming` | `GET` | 30/dk | Gelecek haftanın oynanmamış maçları için ev/beraberlik/deplasman tahminleri. |
| `/api/matches` | `GET` | 30/dk | 306 maçlık fikstürün tamamı: oynanan maçların gerçek skorları, kalanlar için 1X2 / 2,5 alt-üst / karşılıklı gol olasılıkları ve adil oranlar. |
| `/api/coupon` | `GET` | 30/dk | Sıradaki aktif hafta için güncel yasal İddaa oranlarıyla süzülmüş en fazla üç 1X2 seçimi; ulaşılamayan, eski veya değersiz piyasa durumlarını açıkça bildirir. |

Simülasyon istekleri 100–200.000 deneme kabul eder. Senaryo istemleri 2.000 karakterle, Elo değişiklikleri tanınan kulüp adlarıyla ve 1.200–2.000 aralığıyla (ClubElo kulüp ölçeği, uluslararası Elo'ya göre daha dardır), istek gövdeleri 1 MiB ile sınırlıdır.

### Temel simülasyon

```bash
curl -X POST http://localhost:3000/api/simulate \
  -H 'Content-Type: application/json' \
  -d '{"n_sims":50000,"seed":12345}'
```

### Elle puan değişikliğiyle simülasyon

`elo_overrides` fark değil, yerine geçecek mutlak puanları içerir.

```bash
curl -X POST http://localhost:3000/api/simulate \
  -H 'Content-Type: application/json' \
  -d '{"n_sims":50000,"seed":12345,"elo_overrides":{"Trabzonspor":1720}}'
```

### Doğal dil senaryosu

```bash
curl -X POST http://localhost:3000/api/scenario \
  -H 'Content-Type: application/json' \
  -d '{"prompt":"Osimhen sezon sonuna kadar sakatlandı","n_sims":50000,"seed":12345}'
```

## Docker

```bash
docker build -t superlig-sim .
docker run --rm -p 3000:3000 \
  -e KIMI_API_KEY=anahtariniz \
  superlig-sim
```

Senaryo analizi gerekmiyorsa `KIMI_API_KEY` verilmeyebilir.

## Railway'e dağıtım

1. Bu GitHub deposundan bir Railway servisi oluşturun ya da bir klondan `railway init && railway up` çalıştırın.
2. Railway kökteki `Dockerfile` dosyasını algılar; Rust arka ucunu ve React arayüzünü derler.
3. Senaryo analizi isteniyorsa `KIMI_API_KEY` ekleyin.
4. Hız sınırlamasının Railway'in temizleyici uç vekilinden gelen istemci adresini kullanması için `TRUST_PROXY=1` ayarlayın.
5. İsteğe bağlı olarak `LIVE_REFRESH_MINUTES`, `ENSEMBLE_WEIGHTS` ve `RUST_LOG` değerlerini özelleştirin.
6. Sağlık kontrolü yolunu `/api/health` olarak ayarlayın.

Uygulama, Railway'in enjekte ettiği `PORT` değerini kendiliğinden okur.

## Doğrulama

GitHub Actions iş akışı aynı temel kontrolleri çalıştırır:

```bash
cargo fmt -- --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
cargo audit

cd frontend
npm ci
npm audit
npm run build
```

## Verileri yenileme

Depo, derleme sırasında kullanılan fikstürü, geçmiş sonuçları ve kalibre edilmiş Dixon–Coles parametrelerini zaten içerir. Yenilemek için:

```bash
# tff.org'dan resmî 2026-27 fikstürü ve şu ana kadarki sonuçlar.
# Çift devreli fikstürün yapısını baştan sona doğrular: 306 maç, 34 hafta,
# hafta başına 9 maç, 153 eşleşmenin her biri tam olarak iki kez.
python3 scripts/fetch_fixtures.py

# MIT lisanslı Club Football Match Data arşivinden tarih sıralı 2012-13 –
# 2024-25 sonuçları ve resmî TFF haftalık arşivinden 2025-26 sonuçları.
# CSV değiştirilmeden önce bütün sezon/hafta sayıları doğrulanır.
python3 scripts/fetch_history.py

# Yenilenen geçmişle Dixon–Coles'u yeniden kalibre et (~0,3 sn).
cargo run --release --example fit_dc
```

Yeni bir kalibrasyonu işlemeden önce `data/` altındaki değişiklikleri gözden geçirin.

Veri kaynaklarına dair bilinmesi gereken ayrıntılar:

- **Geçmiş maçların gerçek tarihleri korunur.** Dixon–Coles zaman ağırlıkları ve ardışık pi-rating modeli kronolojiye bağlıdır; üretici eksik sezon arşivini sessizce kullanmak yerine reddeder.
- Yeniden dağıtılan arşiv kaynaklı satırların MIT bildirimi [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md) içinde korunur.

- **tff.org sayfalarını windows-1254 ile sunar**, UTF-8 ile değil. UTF-8 varsayarak çözmek bütün Türkçe kulüp adlarını bozar.
- **tff.org eksik bir TLS zinciri sunar**: sertifikasını imzalayan GlobalSign ara sertifikasını göndermez. Tarayıcılar sertifikadaki AIA adresini kendiliğinden çekerek bunu telafi eder; `curl`, Python `urllib` ve `reqwest` etmez ve "unable to get local issuer certificate" hatası verir. Fikstür betiği bu AIA indirmesini kendisi yapar (SHA-256 ile sabitlenmiş), sunucu ise aynı ara sertifikayı `data/tff_intermediate.pem` içinde gömülü tutar. Sertifika doğrulaması hiçbir aşamada kapatılmaz.

## Proje yapısı

```text
.
├── src/
│   ├── main.rs           # Axum sunucusu, yapılandırma ve arka plan yenilemesi
│   ├── sim.rs            # Sezon simülasyonu ve paralel Monte Carlo motoru
│   ├── league.rs         # Puan durumu kayıtları ve Süper Lig sıralama kuralları
│   ├── data.rs           # Kulüpler, puanlar ve resmî fikstür
│   ├── dixoncoles.rs     # Dixon–Coles kalibrasyonu ve ortak skor olasılıkları
│   ├── piratings.rs      # Geçmişe dayalı pi-rating modeli
│   ├── history.rs        # Geçmiş sonuçların yüklenmesi ve kulüp adı eşlemesi
│   ├── scraper.rs        # TFF canlı sonuç alımı
│   ├── coupon.rs         # Kontrollü model-piyasa günlük seçimleri
│   ├── handlers.rs       # API işleyicileri
│   ├── llm.rs            # Kimi senaryo analizi
│   ├── models.rs         # API istek ve yanıt tipleri
│   ├── validation.rs     # İstek doğrulama
│   └── rate_limit.rs     # IP başına hız sınırı
├── data/                 # Fikstür, geçmiş sonuçlar, kalibre edilmiş parametreler
├── frontend/             # React ve TypeScript paneli
├── examples/             # Model kalibrasyon aracı
├── scripts/              # Veri toplama betikleri
├── tests/calibration.rs  # Elo sabitlerini gerçek lige karşı korur
├── .github/workflows/    # CI yapılandırması
└── Dockerfile            # Üretim için çok aşamalı imaj
```

## Veri ve model sınırlılıkları

- Canlı yenileme tff.org'a ve sayfanın güncel biçimine bağlıdır; yenileme başarısız olursa gömülü fikstür kullanılmaya devam eder.
- Adil oranlar, simüle edilmiş olasılıkların tersidir; bahis marjı, likidite ya da piyasa bilgisi içermez.
- Kupon seçimleri modelin kalibrasyonuna ve güncel halka açık piyasa görüntüsüne bağlıdır. Pozitif model değeri yine de kaybedebilir; kazanç vaadi yoktur.
- En zayıf modellenen kulüpler yeni yükselenlerdir: Dixon–Coles ve pi-rating bileşenleri için üst lig geçmişleri olmadığından tahminleri tamamen Elo puanlarına dayanır.
- Yukarıda anlatılan "ikili karşılaştırma bir kez uygulanır" kuralı yayımlanmış bir kural değil, açıkça belirtilmiş bir varsayımdır.
- Senaryo puanları modelin ürettiği varsayımlardır. Dönen açıklamayı okuyun ve çıktıyı keşif amaçlı değerlendirin.
- Tahmin kalitesi puanlara, geçmiş veri kapsamına, model varsayımlarına ve deneme sayısına bağlıdır.
