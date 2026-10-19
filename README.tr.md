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
- İsteğe bağlı olarak doğal dildeki senaryoları Kimi ile doğrulanmış Elo değişikliklerine çevirir ve sezonu yeniden simüle eder (`SCENARIO_ENABLED=1` olmadıkça kapalı olan bir API uç noktası).
- Varsayılan tahmini ligin her durumu için bir kez hesaplar ve önbellekten sunar; sınırlı bir kuyruk, yeni simülasyon isteklerindeki ani yoğunluğu karşılar.
- IP başına hız sınırı, istek doğrulama, deterministik tohum (seed), açık/koyu tema, Docker desteği ve GitHub Actions CI içerir.

## Teknoloji yığını

| Katman | Teknoloji |
|---|---|
| Sunucu/API | Rust 1.87+, Axum, Tokio |
| Simülasyon | Rayon, Rand, Dixon–Coles, pi-rating, Elo/Poisson |
| Arayüz | React 18, TypeScript, Vite |
| Canlı veri | Türkiye Futbol Federasyonu (tff.org) fikstür sayfası |
| Geçmiş veri | MIT lisanslı Club Football Match Data (2012-13 – 2024-25), arşivde eksik olan 2024-25 sezonunun 10-12. haftaları için openfootball (CC0) ve resmî TFF sonuçları (2025-26) |
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

- **Elo (0,5):** 2012-13'ten bu yana tüm üst lig sonuçlarından hesaplanan ve bu sezon her maçtan sonra güncellenen kulüp gücü, artı 80 puanlık ev sahibi düzeltmesi.
- **Dixon–Coles (0,3):** dört yıllık yarılanma ömrüyle zaman ağırlıklandırılmış hücum/savunma güçleri ve az gollü skor korelasyonu; gerçek tarihleri korunan 4.588 maç üzerinde kalibre edilmiştir.
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

Temel simülatör için API anahtarı gerekmez. `KIMI_API_KEY` yalnızca doğal dil senaryoları için gereklidir; bu senaryolar ayrıca `SCENARIO_ENABLED=1` olmadıkça kapalıdır.

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

<http://localhost:5173> adresini açın. Sayfa, arka ucun açılıştan sonra bir kez hesapladığı varsayılan tahmini yükler; bu hesaplama bitene kadar ilk yükleme birkaç saniye sürebilir.

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
| `KIMI_API_KEY` | tanımsız | `/api/scenario` için anahtar; Moonshot platformundan alın. Uç nokta ayrıca `SCENARIO_ENABLED` gerektirir. |
| `SCENARIO_ENABLED` | `0` | `1` ya da `true`, her çağrıda para harcayan `/api/scenario` uç noktasını açar; aksi hâlde uç nokta 404 döner. Panel bu uç noktayı kullanmaz. |
| `PORT` | `3000` | Arka uç HTTP portu. Vite geliştirme sunucusuyla `3001` kullanın. |
| `RUST_LOG` | `superlig_sim=info` | Rust tracing filtresi. |
| `LIVE_REFRESH_MINUTES` | `30` | TFF yenileme aralığı; `0` arka plan yenilemesini kapatır. Her yenileme denemesi, varsayılan tahmini önceden hesaplayarak biter; `0` ile bu hesaplama açılışta bir kez yapılır. |
| `MAX_CONCURRENT_SIMULATIONS` | `1` | Aynı anda çalışabilecek simülasyon sayısı (Rayon tek bir çalıştırmada zaten bütün çekirdekleri kullanır). |
| `MAX_SIMULATION_QUEUE` | `8` | Boş bir yer için bekleyebilecek ek, birbirinden farklı simülasyon sayısı; bunun ötesinde sunucu `Retry-After` ile 503 döner. |
| `SIM_QUEUE_TIMEOUT_SECS` | `20` | Kuyruktaki bir simülasyonun, sunucu 503 dönmeden önce boş yer için bekleyeceği saniye. |
| `ENSEMBLE_WEIGHTS` | `0.5,0.3,0.2` | Virgülle ayrılmış Elo, Dixon–Coles ve pi-rating ağırlıkları. |
| `TRUST_PROXY` | `0` | `X-Forwarded-For` başlığına yalnızca temizleyici bir ters vekil sunucu arkasında güvenin. |

## Panelin kullanımı

1. Sayfa, sunucunun varsayılan tahminiyle açılır (50.000 sezon, tohum 12345). Değiştirmek için simülasyon sayısını ve tohumu seçip **Çalıştır** düğmesine basın. Aynı tohum, aynı yapılandırmayı yeniden üretilebilir kılar.
2. Beş sayfayı inceleyin: **Özet**, **Maçlar**, **Puan durumu**, **Olasılıklar** ve **Sonuçlar**. İlk maçlar puanlandıktan sonra **Özet** sayfasındaki **Tahmin başarısı** bölümünde **Tahmin geçmişi**'ni açarak puanlanan her maçın dondurulmuş tahminini, sonucuyla yan yana ve haftalara göre gruplanmış olarak görebilirsiniz; birikimli log kaybını genel oranlarla karşılaştıran bir grafik de vardır.
3. **Maçlar** sayfasında varsaymak istediğiniz sonuçları seçin (en fazla 20, API'nin sınırı) ve **Yeniden hesapla** düğmesine basın.
4. En güncel TFF sonuçlarını çekip tahmini, maç kartlarını ve isabet görünümünü yeniden hesaplamak için **Canlı veriyi güncelle** düğmesini kullanın. Varsaydığınız sonuçlardan, o arada oynanmış maçlara ait olanlar bir bildirimle kaldırılır.
5. Doğal dil senaryoları panelin parçası değildir. Bunlar isteğe bağlı bir API uç noktasıdır (`/api/scenario`, aşağıda) ve sunucuda `SCENARIO_ENABLED=1` ile `KIMI_API_KEY` ayarlanmadıkça kapalıdır.

Simülasyon kuyruğu doluysa (503) sayfa sunucunun istediği kadar, ama her seferinde en fazla 10 sn bekleyip üç kez yeniden dener, ardından "biraz sonra yeniden deneyin" iletisini gösterir. IP başına sınır aşıldığında (429) sınır 10 sn içinde açılacaksa bekleyip yeniden dener; aksi hâlde hemen "Çok fazla istek — N sn sonra tekrar deneyin" der. Diğer hatalarda kısa, yerelleştirilmiş bir ileti gösterilir; sunucu bir açıklama gönderdiyse onun metni de parantez içinde yer alır. Son tahmin ekranda kalır. Bir simülasyon başarısız olsa bile fikstür ve sonuçlar görüntülenmeye devam eder.

## API

| Uç nokta | Yöntem | IP başına sınır | Açıklama |
|---|---|---:|---|
| `/api/health` | `GET` | — | Servis sürümü, model yapılandırması, son canlı yenileme, `world_version` ve varsayılan tahminin önbellekte olup olmadığı (`forecast_cached`). |
| `/api/forecast` | `GET` | 60/dk | Varsayılan tahmin (50.000 sezon, tohum 12345, değişiklik yok): `POST /api/simulate {}` ile aynı JSON; ligin her durumu için bir kez hesaplanır. |
| `/api/simulate` | `POST` | 30/dk | İsteğe bağlı Elo değişiklikleri ve sabitlenmiş sonuçlarla simülasyon. Aynı istekler, sonuçlar ya da puanlar değişene kadar önbellekten yanıtlanır. |
| `/api/scenario` | `POST` | 10/dk | İstemi Kimi ile çözümler ve dönen Elo değişiklikleriyle yeniden çalıştırır. `SCENARIO_ENABLED=1` değilse 404 döner. |
| `/api/refresh` | `POST` | 5/dk | Güncel TFF sonuçlarını çeker ve uygular. |
| `/api/live` | `GET` | — | Önbellekteki son canlı veri anlık görüntüsünü döndürür. |
| `/api/accuracy` | `GET` | 30/dk | Modelin, başlama düdüğünden önce dondurulmuş tahminlerine karşı karnesi: isabet oranı, temel orana göre log-loss ve kalibrasyon bantları; ayrıca puanlanan her maç, dondurulmuş tahmini ve kendi log-loss değeriyle. |
| `/api/upcoming` | `GET` | 30/dk | Gelecek haftanın oynanmamış maçları için ev/beraberlik/deplasman tahminleri. |
| `/api/matches` | `GET` | 30/dk | 306 maçlık fikstürün tamamı: oynanan maçların gerçek skorları, kalanlar için 1X2 / 2,5 alt-üst / karşılıklı gol olasılıkları ve adil oranlar. |

`/api/accuracy`, `/api/upcoming` ve `/api/matches`, IP başına tek bir 30/dk hakkını paylaşır; `/api/forecast` için ayrı bir 60/dk hakkı vardır. Hız sınırına takılan istek, `Retry-After` başlığıyla HTTP 429 alır.

Simülasyonlar varsayılan olarak birer birer çalışır (`MAX_CONCURRENT_SIMULATIONS`). En fazla `MAX_SIMULATION_QUEUE` ek çalıştırma, sıra için `SIM_QUEUE_TIMEOUT_SECS` saniyeye kadar bekler. Bunun ötesinde `/api/simulate`, `/api/forecast` ve `/api/scenario`, `Retry-After: 10` ile HTTP 503 döner. Önbellekten yanıtlanan istekler ve o anda çalışan bir çalıştırmayla aynı olan istekler kuyrukta hiç beklemez. İstemcilerinin hepsi bağlantıyı kesen, kuyrukta bekleyen bir çalıştırma çalışmadan kuyruktan çıkar.

Gelecek hafta (`/api/upcoming` ve fikstürdeki `current_round`), bugün ya da sonrasına (İstanbul saatiyle) tarihlenmiş en erken oynanmamış maçın haftasıdır. Böylece ertelenen tek bir maç onu geride tutmaz. Ertelenen bir maç bir haftanın maçlarıyla aynı güne denk gelirse, o gün (oynanmış ya da oynanmamış) daha çok maçı olan hafta esas alınır; eşitlikte sonraki hafta, yani normalde o haftanın kendisi kazanır. O gün hafta, sonraki günlerde de olsa oynanacak maçı kaldıkça gelecek hafta olarak kalır; haftanın maçları bittiğinde gelecek hafta, ertelenen maçın haftası olur. Yalnızca tarihi geçmiş maçlar kaldığında, hâlâ oynanmamış maçı olan en erken haftadır; sezon tamamlandığında son haftadır.

API CORS başlığı göndermez: panel aynı kökenden sunulur, Vite geliştirme sunucusu da `/api` isteklerini yönlendirir.

Simülasyon istekleri 100–200.000 deneme kabul eder. Senaryo istemleri 2.000 karakterle, Elo değişiklikleri tanınan kulüp adlarıyla ve 1.200–2.000 aralığıyla, LLM senaryolarının döndürdüğü puan değişiklikleri ±150 ile, istek gövdeleri 1 MiB ile sınırlıdır. Senaryonun analiz metni 600 karakterle sınırlıdır. LLM çağrısı başarısız olursa istemci genel bir HTTP 502 alır; ayrıntılar yalnızca sunucu günlüğüne yazılır.

### Varsayılan tahmin

```bash
curl http://localhost:3000/api/forecast
```

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

Sunucuda `SCENARIO_ENABLED=1` ve `KIMI_API_KEY` gerektirir.

```bash
curl -X POST http://localhost:3000/api/scenario \
  -H 'Content-Type: application/json' \
  -d '{"prompt":"Osimhen sezon sonuna kadar sakatlandı","n_sims":50000,"seed":12345}'
```

## Docker

```bash
docker build -t superlig-sim .
docker run --rm -p 3000:3000 \
  -e SCENARIO_ENABLED=1 \
  -e KIMI_API_KEY=anahtariniz \
  superlig-sim
```

Senaryo analizi gerekmiyorsa `SCENARIO_ENABLED` ve `KIMI_API_KEY` verilmeyebilir. Derleme bağlamı; yerel derleme çıktılarını, `node_modules` klasörlerini, `.env` dosyalarını, Git verilerini ve belgeleri dışarıda bırakır (bkz. `.dockerignore`).

## Railway'e dağıtım

1. Bu GitHub deposundan bir Railway servisi oluşturun ya da bir klondan `railway init && railway up` çalıştırın.
2. Railway kökteki `Dockerfile` dosyasını algılar; Rust arka ucunu ve React arayüzünü derler.
3. Senaryo analizinin kullanılabilir kalması için hem `SCENARIO_ENABLED=1` hem de `KIMI_API_KEY` ayarlayın; `SCENARIO_ENABLED` olmadan `/api/scenario` 404 döner.
4. Hız sınırlamasının Railway'in temizleyici uç vekilinden gelen istemci adresini kullanması için `TRUST_PROXY=1` ayarlayın.
5. İsteğe bağlı olarak `LIVE_REFRESH_MINUTES`, `ENSEMBLE_WEIGHTS`, `MAX_CONCURRENT_SIMULATIONS`, `MAX_SIMULATION_QUEUE`, `SIM_QUEUE_TIMEOUT_SECS` ve `RUST_LOG` değerlerini özelleştirin.
6. Sağlık kontrolü yolunu `/api/health` olarak ayarlayın.

Uygulama, Railway'in enjekte ettiği `PORT` değerini kendiliğinden okur. Zamanlanmış veri yenileme iş akışı (bkz. [Verileri yenileme](#verileri-yenileme)) yeni veriyi işledikten sonra yeniden dağıtım da yapar, ancak yalnızca depoda bir `RAILWAY_TOKEN` sırrı varsa.

## Doğrulama

GitHub Actions iş akışı aynı temel kontrolleri çalıştırır:

```bash
cargo fmt -- --check
cargo clippy --all-targets -- -D warnings
cargo test
# Örneklerin kendi testleri; --all-targets örnekleri bunlar olmadan derler.
cargo clippy --examples --profile test -- -D warnings
cargo test --examples
cargo build --release
cargo audit

cd frontend
npm ci
npm audit
npx tsc --noEmit
npm run build
npm test            # Node.js 22.6+ gerektirir (--experimental-strip-types)
cd ..

# Fikstür ve geçmiş veri betiklerinin çevrimdışı testleri (ağ gerekmez).
python3 -m unittest discover -s scripts -p 'test_*.py'
```

## Verileri yenileme

Depo, derleme sırasında kullanılan fikstürü, geçmiş sonuçları ve kalibre edilmiş Dixon–Coles parametrelerini zaten içerir. Yenilemek için:

```bash
# tff.org'dan resmî 2026-27 fikstürü ve şu ana kadarki sonuçlar.
# Çift devreli fikstürün yapısını baştan sona doğrular: 306 maç, 34 hafta,
# hafta başına 9 maç, 153 eşleşmenin her biri tam olarak iki kez. TFF'nin
# tarihsiz ("Ertelendi" ya da boş) listelediği ertelenmiş bir maç, depodaki
# tarihini ve başlama saatini korur (bir çalıştırmada en fazla 18 maç); 9
# maçının hepsini vermeyen bir hafta sayfası ya da tanınmayan biçimde bir
# tarih hücresi çalıştırmayı durdurur. Depodaki bir sonuç kaybolur ya da
# değişirse betik dosyayı yazmadan durur; gerçek bir TFF düzeltmesi ya da
# hükmen karar için ALLOW_RESULT_CHANGES=1 kullanın.
python3 scripts/fetch_fixtures.py

# MIT lisanslı Club Football Match Data arşivinden tarih sıralı 2012-13 –
# 2024-25 sonuçları, arşivde eksik olan 2024-25 sezonunun 10-12. haftaları
# için openfootball (CC0; arşivin o sezondaki satırlarıyla karşılaştırılarak)
# ve resmî TFF haftalık arşivinden 2025-26 sonuçları. İki kaynak dosya da
# SHA-256 ile sabitlenir (arşiv, betiğin kullandığı satırlar üzerinden);
# özet değişirse çalıştırma durur. CSV değiştirilmeden önce her sezonun,
# listelenen tek 2024-25 hükmen kararı dışında, eksiksiz çift devreli
# olduğu doğrulanır.
python3 scripts/fetch_history.py
# Aynısı; ancak tff.org'dan çekmek yerine depodaki 2025-26 satırlarını
# yeniden doğrulayıp kullanır.
python3 scripts/fetch_history.py --keep-tff

# Yenilenen geçmişle Dixon–Coles'u yeniden kalibre et (~0,3 sn).
cargo run --release --example fit_dc
```

Yeni bir kalibrasyonu işlemeden önce `data/` altındaki değişiklikleri gözden geçirin.

Depodaki fikstür ayrıca kendiliğinden yenilenir. Zamanlanmış bir GitHub Actions iş akışı (`.github/workflows/refresh-data.yml`) perşembe 15:17 UTC'de, cuma 06:00 UTC'de ve elle tetiklendiğinde çalışır. Fikstürü çeker, yaklaşan maçların tahminlerini dondurur, test paketini çalıştırır ve değişiklikleri işler. Başarısız bir çalıştırma "Weekly data refresh failed" başlıklı bir issue açar ya da açık olana yorum ekler; bir sonraki başarılı çalıştırma onu kapatır. Gerçek bir TFF sonuç düzeltmesini kabul etmek için iş akışını `allow_result_changes` seçeneğiyle elle çalıştırın.

Veri kaynaklarına dair bilinmesi gereken ayrıntılar:

- **Geçmiş maçların gerçek tarihleri korunur.** Dixon–Coles zaman ağırlıkları ve ardışık pi-rating modeli kronolojiye bağlıdır; üretici eksik sezon arşivini sessizce kullanmak yerine reddeder.
- **Arşivde 2024-25 sezonunun üç haftası eksiktir.** Sezonun 342 maçından 314'ü vardır: 10-12. haftaların 27 maçı ve bir hükmen karar kaynakta yoktur. Üretici, arşivin o sezondaki her satırının openfootball ile uyuştuğunu doğruladıktan sonra 27 maçı oradan ekler; hükmen kararı ise, 3-0 skor sahada belirlenmediği için dışarıda bırakır.
- Yeniden dağıtılan arşiv kaynaklı satırların MIT bildirimi [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md) içinde korunur; openfootball (CC0 1.0) da orada anılır.

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
│   ├── handlers.rs       # API işleyicileri ve simülasyon kuyruğu
│   ├── forecast_cache.rs # Lig durumuna göre önbelleğe alınan simülasyon yanıtları
│   ├── llm.rs            # Kimi senaryo analizi
│   ├── models.rs         # API istek ve yanıt tipleri
│   ├── validation.rs     # İstek doğrulama
│   └── rate_limit.rs     # IP başına hız sınırı
├── data/                 # Fikstür, geçmiş sonuçlar, kalibre edilmiş parametreler
├── frontend/             # React ve TypeScript paneli
├── examples/             # Model kalibrasyon aracı
├── scripts/              # Veri toplama betikleri ve çevrimdışı testleri
├── tests/calibration.rs  # Elo sabitlerini gerçek lige karşı korur
├── .github/workflows/    # CI ve zamanlanmış veri yenilemesi
└── Dockerfile            # Üretim için çok aşamalı imaj
```

## Veri ve model sınırlılıkları

- Canlı yenileme tff.org'a ve sayfanın güncel biçimine bağlıdır; yenileme başarısız olursa gömülü fikstür kullanılmaya devam eder.
- Adil oranlar, simüle edilmiş olasılıkların tersidir; bahis marjı, likidite ya da piyasa bilgisi içermez.
- En zayıf modellenen kulüpler yeni yükselenlerdir: Dixon–Coles ve pi-rating bileşenleri için üst lig geçmişleri olmadığından tahminleri tamamen Elo puanlarına dayanır.
- Yukarıda anlatılan "ikili karşılaştırma bir kez uygulanır" kuralı yayımlanmış bir kural değil, açıkça belirtilmiş bir varsayımdır.
- Senaryo puanları modelin ürettiği varsayımlardır. Dönen açıklamayı okuyun ve çıktıyı keşif amaçlı değerlendirin.
- Tahmin kalitesi puanlara, geçmiş veri kapsamına, model varsayımlarına ve deneme sayısına bağlıdır.
