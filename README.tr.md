# Süper Lig 2026-27 Simülatörü

[English](README.md) | [Türkçe](README.tr.md)

[![CI](https://github.com/omercagatay/superlig-2026-27-simulator/actions/workflows/ci.yml/badge.svg)](https://github.com/omercagatay/superlig-2026-27-simulator/actions/workflows/ci.yml)

2026-27 Trendyol Süper Lig sezonu için uçtan uca Monte Carlo tahmin uygulaması. Rust ile yazılmış bir simülasyon motoru, React tabanlı bir kontrol paneli, Türkiye Futbol Federasyonu'ndan canlı sonuçlar ve isteğe bağlı olarak Kimi destekli bir senaryo çözümleyicisi bir arada çalışır.

> Bu projenin ürettiği olasılıklar ve adil oranlar model tahminidir; bahis ya da yatırım tavsiyesi değildir.

[Model kartı](docs/model-card.tr.md), modelin ne olduğunu, ne için kullanılıp ne için kullanılmaması gerektiğini, nasıl değerlendirildiğini, aralıklarıyla birlikte sonuçlarını ve yetersiz kaldığı yerleri anlatır ([İngilizcesi](docs/model-card.md)).

## Öne çıkanlar

- 18 kulübün tamamı arasındaki 306 maçlık, 34 haftalık çift devreli fikstürün tamamını simüle eder. Rayon ile paralel olarak 100–200.000 sezon; panelde varsayılan 50.000.
- Beklenen gol tahminlerinde Elo, Dixon–Coles ve pi-rating modellerini harmanlar. Son ikisi 14 sezonluk gerçek Süper Lig sonuçlarına uydurulmuştur.
- Skorları Dixon–Coles ortak dağılımından çeker; bu, az gollü ve berabere biten maçları bağımsız Poisson'lara göre daha iyi temsil eder.
- TFF'nin resmî fikstüründeki oynanmış maçları sabitler ve yalnızca kalan maçları simüle eder.
- Süper Lig'in kendi sıralama kurallarını uygular: **averaj değil, önce ikili averaj (head-to-head)** belirleyicidir.
- Şampiyonluk, ilk iki, tam üçüncülük/dördüncülük, ilk dört ve küme düşme olasılıkları; 18×18 sıralama ızgarası; güncel puan durumunun yanında tahminî sezon sonu puan durumu; simüle edilen sezonlarda şampiyonluk, ilk iki, ilk dört ve ligde kalma için %50, %75 ve %90 ihtimal veren en düşük sezon sonu puanları; gelecek hafta tahminleri ve "kim kimin üstünde bitirir" olasılıkları üretir.
- İsteğe bağlı olarak doğal dildeki senaryoları Kimi ile doğrulanmış Elo değişikliklerine çevirir ve sezonu yeniden simüle eder (`SCENARIO_ENABLED=1` olmadıkça kapalı olan bir API uç noktası).
- Varsayılan tahmini ligin her durumu için bir kez hesaplar ve önbellekten sunar; sınırlı bir kuyruk, yeni simülasyon isteklerindeki ani yoğunluğu karşılar.
- IP başına hız sınırı, istek doğrulama, katı bir Content-Security-Policy altında sıkıştırılmış yanıtlar, düzgün kapanma, deterministik tohum (seed), açık/koyu tema, root olmayan bir kullanıcıyla çalışan Docker imajı ve GitHub Actions CI içerir.

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
lambda_ev      = 1,35 × 10^(( Elo_ev - Elo_deplasman + 80) / 1200)
lambda_deplasman = 1,35 × 10^((-Elo_ev + Elo_deplasman - 80) / 1200)
```

Ev sahipliği avantajı kulübün değil, **maçın** bir özelliğidir: çift devreli fikstürün her iki yarısında da her kulüp kendi sahasında bu avantajı alır.

Varsayılan olarak bu oranlar, gerçek Süper Lig geçmişiyle eğitilmiş iki modelle harmanlanır:

- **Elo (0,5):** 2012-13'ten bu yana tüm üst lig sonuçlarından hesaplanan ve bu sezon her maçtan sonra güncellenen kulüp gücü, artı 80 puanlık ev sahibi düzeltmesi.
- **Dixon–Coles (0,3):** dört yıllık yarılanma ömrüyle zaman ağırlıklandırılmış hücum/savunma güçleri ve az gollü skor korelasyonu; gerçek tarihleri korunan 4.588 maça uydurulmuştur.
- **Pi-rating (0,2):** aynı geçmiş üzerinde ardışık ev/deplasman güç güncellemeleri.

Harmanı değiştirmek için `ENSEMBLE_WEIGHTS` kullanılır; `1,0,0` saf Elo modelini seçer. Dixon–Coles ağırlığı etkinken skorlar onun ortak dağılımından örneklenir.

Harmanlamadan sonra bir maçın iki gol beklentisinden düşük olanı, 0,9'un altındaysa, 0,9'a olan farkının %70'i kadar yukarı çekilir; favorinin beklentisi de bu göreli artışın yarısı kadar yükselir (ikisi de 0,9'un altındaysa yalnızca düşük olan yukarı çekilir). Belirgin biçimde güçsüz takımlar, puan farkının öngördüğünden daha sık gol atar. Geriye dönük testte bu düzeltme yalnızca iki takımın da gol atmasına dair tahmini iyileştirir, başka hiçbir şeyi değil (bkz. *Geriye dönük test sonuçları*).

1. Lig'den yükselen kulüplerin (2026-27 için Amedspor ve Çorum) üst lig geçmişi olmadığından Dixon–Coles ve pi-rating bileşenleri onlara lig ortalaması bir profil verir; ayrımı yalnızca Elo puanları yapar.

**Puanlar kesin değildir; simülasyon da bunu hesaba katar.** Simüle edilen her sezonda her kulübün gerçek gücü, güncel Elo puanı çevresindeki bir normal dağılımdan (`RATING_SIGMA`, 75 Elo puanı) bir kez çekilir ve o sezon boyunca sabit kalır. Puanları kesin kabul etmek favorilerin olasılıklarını şişirir, uçları ise fazla inceltir.

Sigma tahminle değil, kalibrasyonla belirlendi. Arena, ayrılmış her sezonu modelin kendi maç olasılıklarıyla yeniden oynatır ve ortaya çıkan puan tablosunun yayılımını gerçeğiyle karşılaştırır:

| | 2024-25 | 2025-26 |
|---|---:|---:|
| Oynanan maçlarda liderin puanı | 92 | 77 |
| Oynanan maçlardaki gerçek puan yayılımı (sd) | 17,3 | 15,2 |
| Model yayılımı, σ=0 | 13,4 | 13,0 |
| Model yayılımı, σ=75 | 15,0 | 14,4 |

Bağımsız maç örneklemesi, gücün sezon boyunca sürmesini olduğundan az gösterir; sezon başına ortak bir güç çekilişi bu açığın 2024-25'te yaklaşık beşte ikisini, 2025-26'da üçte ikisini kapatır. Bu çekiliş yalnızca sezon simülasyonunda yapıldığı için tek maç tahminlerine hiçbir maliyeti yoktur. 2024-25 geçmişi sezonun tek hükmen kararını (Galatasaray - Adana Demirspor, 3-0 hükmen) dışarıda bıraktığından bu karşılaştırma bilerek iki tarafta da yalnızca oynanan maçların puanlarını kullanır. Arena bu çekilişi her maçın galibiyet/mağlubiyet oranlarına uygular; simülatör ise onu Elo puanlarına ekler, yani mekanizma farklıdır. Simülatörün kendi ürettiğini *Sezon düzeyinde değerlendirme* denetler.

**Bilinen bir sınırlılık:** model 2024-25'te hâlâ fazla dar yayılır; oynanan maçlardan 92 puan toplayan bir lider, sezon başından yapılan yeniden oynatmaların yalnızca yaklaşık %14'ünde çıkar (`examples/backtest.rs` içindeki `EVAL_SEASON_SIM`). Şampiyonluk olasılıklarını "en olası şampiyon kim" sorusunun yanıtı olarak okuyun, şampiyonun toplayacağı puanın kesin bir tahmini olarak değil.

**Puanlar sezonla birlikte değişir.** Elo bileşeni, sonuç geçmişinden hesaplanan bir sezon öncesi puanla başlar ve şimdiye kadar oynanan her maçla, başlama saati sırasıyla ilerletilir: kaybedenden kazanana puan aktaran, `ELO_K` ile ölçeklenen ve gol farkının karekökünü çarpan olarak kullanan bir güncelleme. Puanlar üst üste eklenmez, her seferinde başlangıçtan yeniden hesaplanır; bu yüzden aynı canlı yenilemeyi tekrarlamak hiçbir şeyi değiştirmez. Dixon–Coles ve pi-rating bileşenleri çevrimdışı yeniden uydurulana kadar uydurulmuş değerlerinde kalır.

**Değişiklikler modelin tamamını etkiler.** Elle yapılan bir değişiklik ya da bir Kimi senaryosu bir kulüp için bir puan ister; bu puanın kulübün güncel Elo puanından farkı, aynı Elo farkının Elo bileşenini kaydırdığı gibi, kulübün bütün maçlarındaki harmanlanmış gol beklentisini kaydırır; böylece Dixon–Coles ve pi-rating bileşenleri de onu izler. Değişiklikler eskiden yalnızca Elo bileşenini değiştirdiği için yaklaşık yarı güçte etki ederdi. Sezon öncesinde +50 puan, bir kulübün beklenen sezon puanını artık ortalama yaklaşık 5,5 değiştirir (20.000 simüle edilmiş sezon; önceden yaklaşık 2,8). Kayma güçsüz taraf düzeltmesinden sonra uygulandığından, aynı puana çekilen iki kulübün tahmini aynı çıkmayabilir. Gömülü Dixon–Coles ve pi-rating parametrelerinin kendisi, modeller açıkça yeniden uydurulana kadar değişmez.

### Sıralama kuralları

Puan durumu şu sırayla belirlenir:

1. Puan
2. İkili maçlardaki puan
3. İkili maçlardaki averaj
4. İkili maçlarda atılan gol
5. Genel averaj
6. Genel atılan gol
7. Play-off

İkili maç sonuçları averajdan **önce** gelir; bu, FIFA/UEFA grup aşaması sıralamasından farklıdır.

İkili karşılaştırma, puanı eşit kulüplerden oluşan her blok için **bir kez** uygulanır. Bu turdan sonra hâlâ eşit kalan kulüpler, kalanlar arasında yeni bir mini puan tablosuna değil, doğrudan genel averaja düşer. Yayımlanan kural bu durumu açıkça belirtmediği için bu bir modelleme varsayımıdır ve kendi testiyle korunmaktadır (`head_to_head_is_applied_once_not_recursively`). Simüle edilen bir sezonda hiçbir kuralın ayıramadığı eşitlik, play-off yerine rastgele bir çekilişle çözülür.

TFF'nin yönetmeliği, basında aktarılan metnine göre (tff.org'a geliştirme ortamından erişilemedi), sezon sonundaki sıralamayı belirler. Sezon içindeki puan durumu konusunda basındaki kaynaklar ayrışır: kimine göre ikili averaj kulüpler arasındaki maçlar tamamlanınca hesaba katılır, kimine göre ise haftalık puan durumu sezon sonuna kadar genel averajı kullanır. Site, sezon sonu kuralıyla uyuşan ilk okumayı izler: güncel puan durumu, puanı eşit kulüplere ikili averajı ancak aralarındaki her kulüp çifti iki maçını da oynadıktan sonra uygular; o zamana kadar bu kulüpleri genel averaj, ardından genel atılan gol ayırır. Bu yayımlanmış bir kural olmadığından bu da bir varsayımdır. Güncel puan durumunda başvurulacak bir play-off da yoktur: hiçbir kuralın ayıramadığı kulüpler Türk alfabesine göre adlarıyla sıralanır.

Panel ilk iki, üçüncü, dördüncü ve ilk dört gibi kesin lig sıralamalarını gösterir; bu sıralara doğrudan UEFA turnuvası adı vermez. Gerçek katılım Türkiye Kupası şampiyonuna ve o sezonun UEFA erişim listesine de bağlıdır. Son üç takım küme düşer.

### Kalibrasyon

Yayındaki model (varsayılan harman, sezon öncesinde, hiç maç oynanmamışken), 2012-13'ten bu yana oynanan her Süper Lig maçıyla karşılaştırılır:

| | Geçmiş (4.588 maç) | Model, maç başına | Model, simüle edilen sezonlar |
|---|---:|---:|---:|
| Ev sahibi galibiyeti | %45,4 | %44,9 | — |
| Beraberlik | %25,9 | %24,8 | %24,4 |
| Deplasman galibiyeti | %28,8 | %30,4 | — |
| Maç başına gol | 2,79 | 2,88 | 2,91 |
| Maç başına ev sahibi golü | 1,57 | 1,63 | — |
| Maç başına deplasman golü | 1,22 | 1,25 | — |

Ev sahibi galibiyet oranı, geçmişin kendi örnekleme hatası içinde uyuşur. Beraberlikler yaklaşık bir puan eksik tahmin edilir (her sezonun güçlerini de çeken simüle edilmiş sezonlarda 1,4 puan) ve eksik beraberlikler deplasman galibiyeti olarak görünür; goller maç başına yaklaşık 0,1 fazla tahmin edilir. Bunlar bilinen ve henüz giderilmemiş farklardır. `tests/calibration.rs` bugünkü değerleri sabitler; bir değer kayarsa (bir oran bir puandan, gol sayısı maç başına 0,05'ten fazla) ya da bir fark belirtilen bandın dışına çıkarsa derlemeyi düşürür.

### Geriye dönük test sonuçları

`examples/backtest.rs`, 2016-17'den bu yana her sezonu hafta hafta yeniden oynatır ve her maçı yalnızca o tarihten önce bilinenlerle, üretimdeki kurallarla fiyatlar. Geliştirme sezonları modeli kurmak ve ayarlamak için kullanıldı. İki ayrılmış (holdout) sezon bunun dışında tutuldu; yalnızca Ekim 2026 ayarındaki iki karara bilgi verdiler, bu yüzden hiç dokunulmamış değil, az kullanılmış sayılırlar. Yayındaki model:

| | Geliştirme, 2016-17 – 2023-24 | Ayrılmış, 2024-25 ve 2025-26 |
|---|---:|---:|
| Maç | <!-- metric:dev_n:,d:tr -->2.717<!-- /metric --> | <!-- metric:hold_n:,d:tr -->647<!-- /metric --> |
| Sonuç (ev/beraberlik/deplasman) log kaybı | <!-- metric:dev_1x2_ll:.4f:tr -->0,9991<!-- /metric --> | <!-- metric:hold_1x2_ll:.4f:tr -->0,9756<!-- /metric --> |
| Genel oran tahmininin log kaybı | <!-- metric:dev_1x2_ll_base:.4f:tr -->1,0678<!-- /metric --> | <!-- metric:hold_1x2_ll_base:.4f:tr -->1,0669<!-- /metric --> |
| Genel oranlara göre beceri: sonuç log kaybı | <!-- metric:dev_1x2_skill:.1%:tr -->%6,4<!-- /metric --> | <!-- metric:hold_1x2_skill:.1%:tr -->%8,6<!-- /metric --> |
| Beceri: sıralı olasılık skoru (RPS) | <!-- metric:dev_rpss:.1%:tr -->%10,2<!-- /metric --> | <!-- metric:hold_rpss:.1%:tr -->%13,8<!-- /metric --> |
| Beceri: 3+ gol | <!-- metric:dev_ou25_skill:.1%:tr -->%1,4<!-- /metric --> | <!-- metric:hold_ou25_skill:.1%:tr -->%2,4<!-- /metric --> |
| Beceri: iki takımın da gol atması | <!-- metric:dev_btts_skill:.2%:tr -->%0,06<!-- /metric --> | <!-- metric:hold_btts_skill:.2%:tr -->-%0,12<!-- /metric --> |
| Beraberlik: tahmin / gerçekleşen | <!-- metric:dev_cal_draw_pred:.1%:tr -->%24,6<!-- /metric --> / <!-- metric:dev_cal_draw_obs:.1%:tr -->%25,4<!-- /metric --> | <!-- metric:hold_cal_draw_pred:.1%:tr -->%23,6<!-- /metric --> / <!-- metric:hold_cal_draw_obs:.1%:tr -->%26,1<!-- /metric --> |
| Maç başına gol: tahmin / atılan | <!-- metric:dev_goals_pred:.2f:tr -->2,86<!-- /metric --> / <!-- metric:dev_goals_obs:.2f:tr -->2,82<!-- /metric --> | <!-- metric:hold_goals_pred:.2f:tr -->2,98<!-- /metric --> / <!-- metric:hold_goals_obs:.2f:tr -->2,80<!-- /metric --> |
| İki takımın da gol atması: tahmin / gerçekleşen | <!-- metric:dev_btts_pred:.1%:tr -->%54,8<!-- /metric --> / <!-- metric:dev_btts_obs:.1%:tr -->%56,1<!-- /metric --> | <!-- metric:hold_btts_pred:.1%:tr -->%55,2<!-- /metric --> / <!-- metric:hold_btts_obs:.1%:tr -->%55,6<!-- /metric --> |

Log kaybı ne kadar düşükse o kadar iyidir. Beceri, genel oran tahmininin kaybından modelin giderdiği paydır; genel oranlar yalnızca puanlanan sezondan önceki sezonlardan hesaplanır. Model, sonuçlarda ve gol toplamlarında genel oranları açıkça geçer; ama iki takımın da gol atmasına dair tahmini genel orandan daha iyi değildir. Beraberlikler iki dönemde de biraz az tahmin edilir; yukarıdaki kalibrasyon kontrolü de bunu gösterir. Goller fazla tahmin edilir: geliştirme sezonlarında maç başına yaklaşık 0,05, ayrılmış sezonlarda 0,18 (yukarıdaki gol satırı). İki takımın da gol atması ise biraz az tahmin edilir; yani model golleri iki taraf arasında olması gerekenden daha dengesiz paylaştırır.

Ekim 2026 ayarı (sezon içinde daha hızlı puan güncellemesi ve güçsüz taraf düzeltmesi), maç maç kendinden önceki modelle karşılaştırıldığında (`examples/compare.rs`; %95 aralıklar, maç tarihlerinin bütün olarak yeniden örneklenmesiyle):

| Kayıptaki değişim, ayarlı eksi önceki | Geliştirme | Ayrılmış |
|---|---:|---:|
| Sonuç log kaybı | −0,0077 [−0,0122; −0,0031] | −0,0066 [−0,0170; +0,0043] |
| Sıralı olasılık skoru | −0,0023 [−0,0037; −0,0009] | −0,0026 [−0,0056; +0,0005] |
| İki takımın da gol atması, log kaybı | −0,0016 [−0,0026; −0,0006] | −0,0044 [−0,0081; −0,0008] |

Geliştirme sezonlarında kazanç sonuçlar, RPS, iki takımın da gol atması ve tam skor için istatistiksel olarak anlamlıdır. Ayrılmış sezonlarda yalnızca iki takımın da gol atmasındaki kazanç anlamlıdır; oradaki sonuç kazancı benzer büyüklüktedir ama <!-- metric:hold_n:,d:tr -->647<!-- /metric --> maçın gürültüsü içinde kalır. Sonuçları iyileştiren, daha hızlı puan güncellemesidir; güçsüz taraf düzeltmesi yalnızca iki takımın da gol atmasına dair tahmini iyileştirir. Ayar geliştirme sezonlarına bakılarak seçildiği için onların aralıkları iyimserdir. Asıl temiz sınav canlı 2026-27 sezonudur.

Sonraki bir taramada geliştirme sezonlarında 202 ayar denendi (genel gol düzeyi, az gollü skor düzeltmesi ve 0,1'lik adımlarla her harman ağırlığı); yayına alınmaya değer bir şey çıkmadı. En iyi ayar sonuç log kaybını −0,0006 [−0,0015; +0,0003] değiştirdi, yani gürültü içinde kaldı; ayar, puanlandığı sezon görülmeden seçildiğinde ise kazanç kayboldu (+0,0002). Golleri aşağı ölçeklemek gol toplamlarındaki farkı kapatır ama iki takımın da gol atmasına dair tahmine zarar verir; geliştirme sezonlarında ise, puanlandığı sezon görülmeden seçildiğinde kazancı tutmadı. Bu karardan sonra ayrılmış sezonlara bakıldığında, gollerin daha da fazla tahmin edildiği bu sezonlarda tam skor tahminini iyileştirdiği görüldü; bu yüzden genel gol düzeyi canlı sezon için açık bir soru olarak kalıyor. Daha güçlü bir az gollü skor düzeltmesi, gol toplamlarını değiştirmeden beraberlikleri %25,3'e çıkarırdı; ama onun kazancı da o kadar küçüktür (−0,0003 [−0,0010; +0,0003]). Hiçbir ayar iki takımın da gol atmasına dair tahmine beceri kazandırmaz.

### Piyasaya karşı

`examples/market.rs` geriye dönük testin aynı tahminlerini bahis şirketi oranlarıyla karşılaştırır: MIT lisanslı maç arşivindeki Bet365 maç öncesi oranları; şirketin marjı Shin yöntemiyle çıkarılır (yöntem karşılaştırma çalıştırılmadan önce, çalışma notlarında seçildi; depoya sonuçlarla birlikte işlendi). Bu yalnızca çevrimdışı bir değerlendirmedir. Oranlar depoya eklenmez; hiçbir bahis şirketi oranı ya da piyasa rakamı API'ye veya panele ulaşmaz. API'deki `*_odds` alanları fiyat değil, modelin kendi 1 / p değeridir.

| Sonuç log kaybı, oranı olan maçlar | Geliştirme | Ayrılmış |
|---|---:|---:|
| Maç | <!-- metric:market:dev_n:,d:tr -->2.714<!-- /metric --> | <!-- metric:market:hold_n:,d:tr -->620<!-- /metric --> |
| Model | <!-- metric:market:dev_model_1x2_ll:.4f:tr -->0,9993<!-- /metric --> | <!-- metric:market:hold_model_1x2_ll:.4f:tr -->0,9754<!-- /metric --> |
| Piyasa | <!-- metric:market:dev_market_1x2_ll:.4f:tr -->0,9888<!-- /metric --> | <!-- metric:market:hold_market_1x2_ll:.4f:tr -->0,9425<!-- /metric --> |
| Model eksi piyasa | <!-- metric:market:dev_diff_1x2_ll:+.4f:tr -->+0,0106<!-- /metric --> [+0,005; +0,017] | <!-- metric:market:hold_diff_1x2_ll:+.4f:tr -->+0,0328<!-- /metric --> [+0,017; +0,050] |

Piyasa iki dönemde de açıkça daha iyidir (%95 aralıklar, maç tarihlerinin bütün olarak yeniden örneklenmesiyle); sıralı olasılık skorunda da öndedir, 3+ golde ise daha az belirgin biçimde. Marjı orantılı olarak çıkarmak ya da şirketler arasındaki en iyi oranı kullanmak aynı tabloyu verir. Modeli piyasayla karıştırmak işe yaramaz: geliştirme sezonlarında modele en uygun ağırlık <!-- metric:market:dev_pool_w:.2f:tr -->0,07<!-- /metric --> çıkar (aralık 0 ile 0,21) ve bu ağırlıktaki karışım ayrılmış sezonlarda tek başına piyasadan daha kötü sonuç verir. Yani piyasanın bilmediği bir bilgi örneklem dışında görünmüyor; bu kadar az veriyle bu, modelin böyle bir bilgi hiç taşımadığını kanıtlamaz.

Farkın sezonun ilerleyen bölümünde açılıp açılmadığı da belli değil. Geliştirme sezonlarında fark ilk on haftada küçüktür, sonrasında daha büyüktür; ayrılmış sezonlarda ise baştan büyüktür. Ama iki dönem arasındaki fark (10. haftadan sonrası eksi 1-10. haftalar) geliştirme sezonlarında <!-- metric:market:dev_md_late_minus_early_1x2_ll:+.4f:tr -->+0,0106<!-- /metric --> [−0,002; +0,024], ayrılmış sezonlarda <!-- metric:market:hold_md_late_minus_early_1x2_ll:+.4f:tr -->-0,0100<!-- /metric --> [−0,043; +0,023]; ikisi de gürültü içinde kalır.

Arşivdeki oranlar kapanış değil maç öncesi oranlarıdır. Kapanış oranları daha da isabetli olduğundan onlarla aradaki fark büyük olasılıkla daha geniştir; kapanış oranlarıyla karşılaştırma ileride yapılacak bir iştir. Canlı sezonda şimdiye kadar hem dondurulmuş tahmini hem oranı olan yalnızca <!-- metric:market:live_n -->19<!-- /metric --> maç var: bir şey söylemek için çok az. Bunların hepsinin tahminini Ağustos modeli, maçtan <!-- metric:market:live_age_days_min:d:tr -->0<!-- /metric --> ile <!-- metric:market:live_age_days_max:d:tr -->11<!-- /metric --> gün önce yaptı; bu da modeli maç öncesi bir fiyat karşısında dezavantajlı duruma düşürebilir.

### Sezon düzeyinde değerlendirme

`examples/season_eval.rs`, sezon tahminlerinin kendisini (sıralama, şampiyonluk, ilk dört, küme düşme ve sezon sonu puanı) puanlar. Her geçmiş sezonu üç noktadan, yani sezon başlamadan önce, üçte biri ve üçte ikisi oynandığında, her seferinde yalnızca o an elde olan bilgiyle yeniden oynatır. <!-- metric:season:dev_n_seasons -->8<!-- /metric --> geliştirme sezonu üzerinden:

| | Sezon öncesi | Üçte biri oynanmışken | Üçte ikisi oynanmışken |
|---|---:|---:|---:|
| Sıralama RPS (düşük olan daha iyi) | <!-- metric:season:dev_s0_pos_rps:.4f:tr -->0,1322<!-- /metric --> | <!-- metric:season:dev_s33_pos_rps:.4f:tr -->0,1121<!-- /metric --> | <!-- metric:season:dev_s67_pos_rps:.4f:tr -->0,0646<!-- /metric --> |
| Şampiyonluk, genel orana göre Brier becerisi | <!-- metric:season:dev_s0_title_bss:.3f:tr -->0,052<!-- /metric --> | <!-- metric:season:dev_s33_title_bss:.3f:tr -->0,307<!-- /metric --> | <!-- metric:season:dev_s67_title_bss:.3f:tr -->0,500<!-- /metric --> |
| İlk dört, Brier becerisi | <!-- metric:season:dev_s0_top4_bss:.3f:tr -->0,430<!-- /metric --> | <!-- metric:season:dev_s33_top4_bss:.3f:tr -->0,584<!-- /metric --> | <!-- metric:season:dev_s67_top4_bss:.3f:tr -->0,813<!-- /metric --> |
| Küme düşme, Brier becerisi | <!-- metric:season:dev_s0_rel_bss:.3f:tr -->0,145<!-- /metric --> | <!-- metric:season:dev_s33_rel_bss:.3f:tr -->0,319<!-- /metric --> | <!-- metric:season:dev_s67_rel_bss:.3f:tr -->0,590<!-- /metric --> |
| Sezon sonu puanı %80'lik aralığın içinde | %<!-- metric:season:dev_s0_cov80_pct:.1f:tr -->70,4<!-- /metric --> | %<!-- metric:season:dev_s33_cov80_pct:.1f:tr -->67,8<!-- /metric --> | %<!-- metric:season:dev_s67_cov80_pct:.1f:tr -->73,0<!-- /metric --> |
| Sezon sonu puanı, puan cinsinden CRPS (düşük olan daha iyi) | <!-- metric:season:dev_s0_pts_crps:.2f:tr -->6,18<!-- /metric --> | <!-- metric:season:dev_s33_pts_crps:.2f:tr -->5,04<!-- /metric --> | <!-- metric:season:dev_s67_pts_crps:.2f:tr -->2,93<!-- /metric --> |

Tahminler sezon ilerledikçe iyileşir ama aşırı özgüvenlidir: kulüplerin %80'ini içermesi gereken puan aralıkları yaklaşık %70'ini içerdi; simüle edilen puan tabloları da gerçeklerinden daha az yayılır (standart sapma <!-- metric:season:dev_s0_table_sd_sim:.1f:tr -->13,6<!-- /metric -->–<!-- metric:season:dev_s67_table_sd_sim:.1f:tr -->14,4<!-- /metric --> puan, gerçekte <!-- metric:season:dev_s0_table_sd_actual:.1f:tr -->15,1<!-- /metric -->). Sezon öncesi şampiyonluk tahmini, yalnızca genel oranları kullanmaktan ayırt edilemez (becerisi için %95 aralık: −0,099 ile 0,209, sekiz sezon). Paneldeki aralıkları olması gerekenden biraz dar sayın. İki ayrılmış sezon iki yönü de gösterir ve hiçbir şeyi kesinleştirmez; `RATING_SIGMA` da onların yayılımına göre ayarlandığından (bkz. yukarı) onu bağımsız olarak sınamazlar.

CRPS, her kulübün sezon sonu puanı için tahmin edilen dağılımın tamamını puan cinsinden puanlar (tek bir sayıdan oluşan bir tahminde düz ıska olurdu) ve aralıkları yalnızca ıskaların gerektirdiği yerde genişletmeyi ödüllendirir. Aralıkları genişletmek denendi ama yayına alınmadı. Sezon başına güç çekilişini yalnızca Elo puanlarına değil modelin tamamına uygulamak, geliştirme sezonlarını nominal kapsamaya getirdi (%80'lik aralıklar kulüplerin %79,6–82,9'unu içerdi; anlık görüntü başına 5.000 sezonluk çalıştırmalarla) ve sıralama RPS'ini 8 sezonun 7'sinde iyileştirdi. Sezon sonu puanı CRPS'indeki kazancı daha zayıftı: yalnızca 8 sezonun 5'inde daha iyiydi, aralığı değişiklik olmamasını da içeriyordu; sezonun üçte ikisi oynanmışken ise daha iyi değildi. Bu aralıklar, yöntemin denenen 47'den fazla ayar arasından seçildiğini de hesaba katmaz. Ayrılmış sezonlar çalıştırılmadan önce yazılan bir kurala göre (çalışma notlarında; depoya ancak sonuçtan sonra işlendi), oradaki sezon sonu puanı CRPS'i ya da sıralama RPS'i (her biri üç anlık görüntü üzerinden birleştirilerek) kötüleşmedikçe bu yöntem benimsenecekti; ikisi de kötüleşti. Sıralama RPS'indeki fark çok küçüktü ve rastgele tohuma göre değişti; kararı CRPS'teki fark verdi: denenen her tohumda ve iki sezonda da daha kötüydü. Sezonun üçte biri oynandıktan sonra, yayındaki tahminlerin fazla dar olmadığı yerde, aralıkları fazla genişletti. Bu yüzden simülatör değişmedi. İki sezon pek az şeyi kesinleştirir; asıl temiz sınav canlı 2026-27 sezonudur.

### Model seçimi

Harman keyfî olarak değil, geriye dönük testle seçildi. `cargo run --release --example arena`, aday modelleri iki ayrılmış sezona karşı yarıştırır (2023-24'e kadarki sezonlara uydurulup 2024-25'te ayarlanır; 2024-25'e kadarki sezonlara yeniden uydurulup 2025-26'da değerlendirilir) ve log kaybı, RPS, Brier ve isabetle puanlar. Test sezonu sıralaması (2025-26, 306 maç, maç başına log kaybı):

| model | log kaybı | RPS |
|---|---:|---:|
| Dixon–Coles, 1,5 yıllık yarılanma ömrü | 0,9994 | 0,1954 |
| lojistik yığınlama (`logit-stack`) | 1,0003 | 0,1968 |
| **harman 0,5/0,3/0,2** | **1,0011** | **0,1965** |
| harman, doğrulama sezonuna göre ayarlanmış ağırlıklar | 1,0051 | 0,1970 |
| pi-rating | 1,0061 | 0,1978 |
| Dixon–Coles, 4 yıllık yarılanma ömrü | 1,0065 | 0,1979 |
| Elo–Poisson | 1,0122 | 0,1988 |
| ev sahibi avantajı temel modeli | 1,0866 | 0,2250 |

Arena her sezon için on iki satır yazdırır: dokuz uydurulmuş model ve üç temel model; tablo bunlardan bir seçkidir. Uydurulan her model temel modelleri açık farkla geçer. Yayındaki harman test sezonunun birincisinin 0,0017 log kaybı gerisindedir, doğrulama sezonunda ise ondan 0,0150 öndedir (0,9978'e karşı 0,9828). Yalnızca doğrulama sezonuna göre ayarlanan ağırlıklar (0,4/0/0,6) o sezonu kazanır ama test sezonunda geride kalır; bu yüzden daha geniş tabanlı 0,5/0,3/0,2 harmanı korunur.

Arena ayrıca, değişkenleri yalnızca önceki sezonlara uydurulan, sızıntısız ve zamanda ileriye doğru yeniden eğitilen çok terimli bir lojistik yığınlama (stacking) modeli (`logit-stack`) de kurar. Bu model test sezonunda 1,0003 ile harmandan 0,0008 öndedir; ama doğrulama sezonunda 0,9978 ile ondan 0,0150 geridedir. Tek sezonda bu kadar küçük bir üstünlük, yalnızca sonuçlara dayanan değişkenlerle daha basit modelin yerini almayı haklı çıkarmaz; xG ve kadro gibi daha zengin girdiler burada yok.

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
2. Beş sayfayı inceleyin: **Özet**, **Maçlar**, **Puan durumu**, **Olasılıklar** ve **Sonuçlar**. **Sonuçlar** dışındaki her sayfa, sezonun nereye geldiğini söyleyen bir satırla açılır ("63/306 maç oynandı · son sonuç 12 Ekim"). **Puan durumu** sayfası **Güncel** ve **Tahminî** (sezon sonu) puan durumu arasında geçiş yapar; ilk sonuç girildikten sonra güncel olanla açılır. **Olasılıklar** sayfası **Gereken puan** tablosuyla biter: **Şampiyon**, **İlk iki**, **İlk dört** ve **İlk 15 (kalır)** için, simüle edilen sezonlarda o sırayı ya da daha iyisini %50, %75 ya da %90 ihtimalle getiren en düşük sezon sonu puanı. Birkaç yüz sezonluk bir çalıştırma bu rakamların bazıları için fazla küçüktür; onların yerinde "–" görünür. Yaklaşık 100 sezonda hepsi boş kalır ve sayfa "Bunu görmek için daha fazla simülasyon çalıştırın" der. İlk maçlar puanlandıktan sonra **Özet** sayfasındaki **Tahmin başarısı** bölümü isabet oranını, log kaybını ve genel oranlara göre farkını %95 aralığı ve maç sayısıyla gösterir; aynısını her model sürümü için ayrı ayrı da verir (modelin önceki bir sürümünün yaptığı tahminler, güncel sürümünkülerden ayrı puanlanır). Oradaki **Tahmin geçmişi**'ni açarak puanlanan her maçın dondurulmuş tahminini, sonucu ve tahmini yapan modelle yan yana, haftalara göre gruplanmış olarak görebilirsiniz; birikimli log kaybını genel oranlarla karşılaştıran bir grafik de vardır.
3. **Maçlar** sayfasında varsaymak istediğiniz sonuçları seçin (en fazla 20, API'nin sınırı) ve **Yeniden hesapla** düğmesine basın.
4. En güncel TFF sonuçlarını çekip tahmini, maç kartlarını ve isabet görünümünü yeniden hesaplamak için **Canlı veriyi güncelle** düğmesini kullanın. Varsaydığınız sonuçlardan, o arada oynanmış maçlara ait olanlar bir bildirimle kaldırılır.
5. Doğal dil senaryoları panelin parçası değildir. Bunlar isteğe bağlı bir API uç noktasıdır (`/api/scenario`, aşağıda) ve sunucuda `SCENARIO_ENABLED=1` ile `KIMI_API_KEY` ayarlanmadıkça kapalıdır.

Simülasyon kuyruğu doluysa (503) sayfa sunucunun istediği kadar, ama her seferinde en fazla 10 sn bekleyip üç kez yeniden dener, ardından "biraz sonra yeniden deneyin" iletisini gösterir. IP başına sınır aşıldığında (429) sınır 10 sn içinde açılacaksa bekleyip yeniden dener; aksi hâlde hemen "Çok fazla istek — N sn sonra tekrar deneyin" der. Diğer hatalarda kısa, yerelleştirilmiş bir ileti gösterilir; sunucu bir açıklama gönderdiyse onun metni de parantez içinde yer alır. Son tahmin ekranda kalır. Bir simülasyon başarısız olsa bile fikstür ve sonuçlar görüntülenmeye devam eder.

## API

| Uç nokta | Yöntem | IP başına sınır | Açıklama |
|---|---|---:|---|
| `/api/health` | `GET` | — | Servis sürümü, model yapılandırması ve `model_version`, son canlı yenileme, `world_version` ve varsayılan tahminin önbellekte olup olmadığı (`forecast_cached`). |
| `/api/forecast` | `GET` | 60/dk | Varsayılan tahmin (50.000 sezon, tohum 12345, değişiklik yok): `POST /api/simulate {}` ile aynı JSON; ligin her durumu için bir kez hesaplanır. |
| `/api/simulate` | `POST` | 30/dk | İsteğe bağlı Elo değişiklikleri ve sabitlenmiş sonuçlarla simülasyon. Aynı istekler, sonuçlar ya da puanlar değişene kadar önbellekten yanıtlanır. |
| `/api/scenario` | `POST` | 10/dk | İstemi Kimi ile çözümler ve dönen Elo değişiklikleriyle yeniden çalıştırır. `SCENARIO_ENABLED=1` değilse 404 döner. |
| `/api/refresh` | `POST` | 5/dk | Güncel TFF sonuçlarını çeker ve uygular. |
| `/api/live` | `GET` | — | Önbellekteki son canlı veri anlık görüntüsünü döndürür. |
| `/api/accuracy` | `GET` | 30/dk | Modelin, başlama düdüğünden önce dondurulmuş tahminlerine karşı karnesi: isabet oranı, genel oranlara göre log kaybı ve aradaki farkın %95 aralığı, kalibrasyon bantları; hem toplu hem her model sürümü için ayrı (`by_model`); ayrıca puanlanan her maç, dondurulmuş tahmini, onu yapan model ve kendi log kaybı değeriyle. |
| `/api/upcoming` | `GET` | 30/dk | Gelecek haftanın oynanmamış maçları için ev/beraberlik/deplasman tahminleri. |
| `/api/matches` | `GET` | 30/dk | 306 maçlık fikstürün tamamı: oynanan maçların gerçek skorları, kalanlar için 1X2 / 2,5 alt-üst / iki takımın da gol atması olasılıkları ve adil oranlar; güncel puan durumu (`standings`) ve sezonun nereye geldiği (`as_of`: oynanan maç sayısı, toplam maç, son sonucun tarihi). |

`/api/accuracy`, `/api/upcoming` ve `/api/matches`, IP başına tek bir 30/dk hakkını paylaşır; `/api/forecast` için ayrı bir 60/dk hakkı vardır. Hız sınırına takılan istek, `Retry-After` başlığıyla HTTP 429 alır.

Simülasyonlar varsayılan olarak birer birer çalışır (`MAX_CONCURRENT_SIMULATIONS`). En fazla `MAX_SIMULATION_QUEUE` ek çalıştırma, sıra için `SIM_QUEUE_TIMEOUT_SECS` saniyeye kadar bekler. Bunun ötesinde `/api/simulate`, `/api/forecast` ve `/api/scenario`, `Retry-After: 10` ile HTTP 503 döner. Önbellekten yanıtlanan istekler ve o anda çalışan bir çalıştırmayla aynı olan istekler kuyrukta hiç beklemez. İstemcilerinin hepsi bağlantıyı kesen, kuyrukta bekleyen bir çalıştırma çalışmadan kuyruktan çıkar.

Gelecek hafta (`/api/upcoming` ve fikstürdeki `current_round`), bugün ya da sonrasına (İstanbul saatiyle) tarihlenmiş en erken oynanmamış maçın haftasıdır. Böylece ertelenen tek bir maç onu geride tutmaz. Ertelenen bir maç bir haftanın maçlarıyla aynı güne denk gelirse, o gün (oynanmış ya da oynanmamış) daha çok maçı olan hafta esas alınır; eşitlikte sonraki hafta, yani normalde o haftanın kendisi kazanır. O gün hafta, sonraki günlerde de olsa oynanacak maçı kaldıkça gelecek hafta olarak kalır; haftanın maçları bittiğinde gelecek hafta, ertelenen maçın haftası olur. Yalnızca tarihi geçmiş maçlar kaldığında, hâlâ oynanmamış maçı olan en erken haftadır; sezon tamamlandığında son haftadır.

API CORS başlığı göndermez: panel aynı kökenden sunulur, Vite geliştirme sunucusu da `/api` isteklerini yönlendirir.

API'den ya da panelden gelen yanıtlar, istemci kabul ediyorsa gzip ya da brotli ile sıkıştırılır (görseller ve çok küçük yanıtlar hariç). Her yanıt `X-Content-Type-Options: nosniff`, `Referrer-Policy: strict-origin-when-cross-origin` ve `X-Frame-Options: DENY` başlıklarını ve betiklere, stillere ve isteklere yalnızca sitenin kendisinden izin veren bir Content-Security-Policy taşır: satır içi betik çalışmaz, site başka bir sayfanın çerçevesine alınamaz.

`/api/simulate` ayrıca `what_if` kabul eder: oynanmamış maçlar için en fazla 20 sabitlenmiş sonuç, örneğin `[{"home":"Gaziantep","away":"Galatasaray","outcome":"home"}]`. Sabitlenen skor değil *sonuçtur*: skor, modelin o maç için skor olasılıklarından, sabitlenen sonuçla sınırlanarak çekilir; böylece averaj gerçekçi kalır. Sabitlenen sonuç, aynı gerçek sonucun yapacağı gibi simüle edilen sezonun Elo puanlarını da günceller ve sezon başlama saati sırasıyla oynanır; böylece yalnızca sonraki maçlar bundan etkilenir. 19 Ekim 2026 fikstüründe (50.000 sezon), 9. haftadaki Galatasaray - Fenerbahçe maçında Fenerbahçe galibiyetini sabitlemek Galatasaray'ın şampiyonluk olasılığını %59,1'den %32,1'e düşürür; bu, maçı gerçek bir 1-2 (%34,5) ile 1-3 (%29,8) olarak girmenin arasındadır. Oynanmış ya da var olmayan bir maçı ya da aynı maçı iki kez sabitlemek sessizce yok sayılmaz, hata döner.

Simülasyon istekleri 100–200.000 deneme kabul eder. Senaryo istemleri 2.000 karakterle, Elo değişiklikleri tanınan kulüp adlarıyla ve 1.200–2.000 aralığıyla, LLM senaryolarının döndürdüğü puan değişiklikleri ±150 ile (bir kulübü 1.200–2.000 aralığının dışına taşıyacak değişiklik sınırda durur), istek gövdeleri 1 MiB ile sınırlıdır. Senaryonun analiz metni 600 karakterle sınırlıdır. LLM çağrısı başarısız olursa istemci genel bir HTTP 502 alır; ayrıntılar yalnızca sunucu günlüğüne yazılır.

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

`elo_overrides` fark değil, istenen puanları içerir. Her biri, kulübün güncel Elo puanından farkı olarak modelin tamamına uygulanır (bkz. *Model nasıl çalışıyor*); Elo puanlarının kendisi değiştirilmez.

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

Senaryo analizi gerekmiyorsa `SCENARIO_ENABLED` ve `KIMI_API_KEY` verilmeyebilir. Derleme bağlamı; yerel derleme çıktılarını, `node_modules` klasörlerini, `.env` dosyalarını, Git verilerini, belgeleri ve piyasa karşılaştırmasının oranlarını dışarıda bırakır (bkz. `.dockerignore`).

Sunucu, hiçbir dosyanın sahibi olmayan ayrıcalıksız bir kullanıcıyla (UID ve GID 10001) çalışır. `docker stop` (SIGTERM) ya da Ctrl-C ile yeni bağlantı kabul etmeyi bırakır, açık isteklere bitmeleri için en fazla 5 saniye tanır ve sürmekte olan bir simülasyonu beklemeden kapanır.

## Railway'e dağıtım

1. Bu GitHub deposundan bir Railway servisi oluşturun ya da bir klondan `railway init && railway up` çalıştırın.
2. Railway kökteki `Dockerfile` dosyasını algılar; Rust arka ucunu ve React arayüzünü derler.
3. Senaryo analizinin kullanılabilir kalması için hem `SCENARIO_ENABLED=1` hem de `KIMI_API_KEY` ayarlayın; `SCENARIO_ENABLED` olmadan `/api/scenario` 404 döner.
4. Hız sınırlamasının Railway'in temizleyici uç vekilinden gelen istemci adresini kullanması için `TRUST_PROXY=1` ayarlayın.
5. İsteğe bağlı olarak `LIVE_REFRESH_MINUTES`, `ENSEMBLE_WEIGHTS`, `MAX_CONCURRENT_SIMULATIONS`, `MAX_SIMULATION_QUEUE`, `SIM_QUEUE_TIMEOUT_SECS` ve `RUST_LOG` değerlerini özelleştirin.
6. Sağlık kontrolü yolunu `/api/health` olarak ayarlayın.

Uygulama, Railway'in enjekte ettiği `PORT` değerini kendiliğinden okur ve Railway bir dağıtımı durdururken gönderdiği SIGTERM ile düzgün biçimde kapanır. Zamanlanmış veri yenileme iş akışı (bkz. [Verileri yenileme](#verileri-yenileme)) yeni veriyi işledikten sonra yeniden dağıtım da yapar, ancak yalnızca depoda bir `RAILWAY_TOKEN` sırrı varsa.

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

# Bu README'nin aktardığı değerlendirme rakamları hâlâ kodun ürettiği
# rakamlar olmalı.
EVAL_HOLDOUT=1 EVAL_METRICS=/tmp/backtest.json cargo run --release --example backtest
python3 scripts/check_metrics.py diff /tmp/backtest.json data/metrics/backtest.json
EVAL_HOLDOUT=1 SEASON_METRICS=/tmp/season.json cargo run --release --example season_eval
python3 scripts/check_metrics.py diff /tmp/season.json data/metrics/season.json

# Veri betiklerinin ve metrik denetleyicisinin çevrimdışı testleri (ağ
# gerekmez) ve README'lerdeki, CLAUDE.md'deki ve model kartındaki her rakamın
# data/metrics/ ile karşılaştırılması.
python3 -m unittest discover -s scripts -p 'test_*.py'
python3 scripts/check_metrics.py docs
```

*Geriye dönük test sonuçları* ve *Sezon düzeyinde değerlendirme* bölümlerindeki ve [model kartındaki](docs/model-card.tr.md) yayındaki modele ait rakamlar bu şekilde denetlenir. Metrik dosyası yazmayan araçlardan gelen rakamlar denetlenmez: ayar karşılaştırması (`examples/compare.rs`), sezon aralıkları (yalnızca yazdırılır), sonraki taramalar (202 maç ayarı ve sezon yayılımı denemeleri), sezon yayılımı değişikliğinin ayrılmış sezonlardaki karşılaştırması ve elle yapılan değişiklik ile sabitlenen sonuç örnekleri (tek seferlik çalıştırmalar; fikstüre bağlı olanlar tarihlidir). Bir değişiklik denetlenen rakamları değiştirirse depodaki dosyaları `EVAL_HOLDOUT=1 EVAL_METRICS=data/metrics/backtest.json cargo run --release --example backtest` ve `EVAL_HOLDOUT=1 SEASON_METRICS=data/metrics/season.json cargo run --release --example season_eval` ile yeniden üretin, ardından aktarılan rakamları güncelleyin.

*Piyasaya karşı* bölümündeki karşılaştırma `data/metrics/market.json` dosyasından aktarılır; CI bu dosyayı yeniden üretemez, çünkü oranlar ağdan çekilir ve depoya hiç eklenmez. CI yine de aktarılan her rakamı depodaki dosyayla karşılaştırır. Yeniden üretmek için önce `python3 scripts/fetch_fixtures.py`'ı (canlı karşılaştırma oynanan her maçı görsün diye), sonra `python3 scripts/fetch_odds.py`'ı, ardından ayrılmış sezonlu geriye dönük testi `EVAL_DUMP=/tmp/prod.csv` ile ve son olarak `MARKET_METRICS=data/metrics/market.json cargo run --release --example market -- /tmp/prod.csv` komutunu çalıştırın.

## Verileri yenileme

Depo, derleme sırasında kullanılan fikstürü, geçmiş sonuçları ve uydurulmuş Dixon–Coles parametrelerini zaten içerir. Yenilemek için:

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

# Yalnızca çevrimdışı piyasa karşılaştırması için, aynı arşivden bahis
# şirketi oranları; geçmişle karşılaştırılır ve SHA-256 ile sabitlenir.
# data/odds/ altına yazılır, depoya eklenmez; uygulamada hiçbir şey onları
# okumaz. Önce fetch_fixtures.py'ı çalıştırın: fikstürün henüz oynanmış
# göstermediği bir maça ait 2026-27 oran satırı atlanır ve sayılır.
python3 scripts/fetch_odds.py

# Dixon–Coles'u yenilenen geçmişe yeniden uydurur (~0,3 sn).
cargo run --release --example fit_dc
```

Yeniden uydurulmuş parametreleri işlemeden önce `data/` altındaki değişiklikleri gözden geçirin.

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
│   ├── dixoncoles.rs     # Dixon–Coles parametre kestirimi ve ortak skor olasılıkları
│   ├── piratings.rs      # Geçmişe dayalı pi-rating modeli
│   ├── history.rs        # Geçmiş sonuçların yüklenmesi ve kulüp adı eşlemesi
│   ├── scraper.rs        # TFF canlı sonuç alımı
│   ├── handlers.rs       # API işleyicileri ve simülasyon kuyruğu
│   ├── forecast_cache.rs # Lig durumuna göre önbelleğe alınan simülasyon yanıtları
│   ├── llm.rs            # Kimi senaryo analizi
│   ├── accuracy.rs       # Dondurulmuş tahmin günlüğünün model sürümüne göre puanlanması
│   ├── model_version.rs  # Bir tahmini hangi modelin yaptığı ve modelin parmak izi
│   ├── odds.rs           # API için adil oranlar (1 / p)
│   ├── models.rs         # API istek ve yanıt tipleri
│   ├── validation.rs     # İstek doğrulama
│   └── rate_limit.rs     # IP başına hız sınırı
├── data/                 # Fikstür, geçmiş sonuçlar, uydurulmuş parametreler, tahmin günlüğü
│   └── metrics/          # Depodaki geriye dönük test, sezon değerlendirmesi ve piyasa karşılaştırması rakamları
├── docs/                 # Model kartı, Türkçe ve İngilizce: model ne içindir, nasıl değerlendirildi, sınırları
├── frontend/             # React ve TypeScript paneli
├── examples/             # Parametre kestirimi, geriye dönük test, sezon değerlendirmesi, karşılaştırma, piyasa karşılaştırması, tahmin kaydı
├── scripts/              # Veri toplama betikleri, metrik denetleyicisi ve çevrimdışı testleri
├── tests/calibration.rs  # Yayındaki harmanı gerçek lige karşı denetler
├── .github/workflows/    # CI ve zamanlanmış veri yenilemesi
├── Dockerfile            # Üretim için çok aşamalı imaj
├── LICENSE               # MIT
└── THIRD_PARTY_NOTICES.md # Veri kaynakları ve koşulları
```

## Veri ve model sınırlılıkları

- Canlı yenileme tff.org'a ve sayfanın güncel biçimine bağlıdır; yenileme başarısız olursa gömülü fikstür kullanılmaya devam eder.
- Adil oranlar, simüle edilmiş olasılıkların tersidir; bahis marjı, likidite ya da piyasa bilgisi içermez.
- En zayıf modellenen kulüpler yeni yükselenlerdir: Dixon–Coles ve pi-rating bileşenleri için üst lig geçmişleri olmadığından tahminleri tamamen Elo puanlarına dayanır.
- Yukarıda anlatılan "ikili karşılaştırma bir kez uygulanır" kuralı ve güncel puan durumunun ikili averajı saymak için iki maçın da oynanmasını beklemesi, yayımlanmış kurallar değil, açıkça belirtilmiş varsayımlardır.
- Model bahis şirketi oranlarına yenilir (bkz. [Piyasaya karşı](#piyasaya-karşı)). Bahis için bir dayanak değildir; panel hiçbir bahis şirketi oranı ya da piyasa rakamı göstermez.
- Senaryo puanları modelin ürettiği varsayımlardır. Dönen açıklamayı okuyun ve çıktıyı keşif amaçlı değerlendirin.
- Sezon düzeyindeki aralıklar fazla dardır: geriye dönük testlerde kulüplerin %80'ini içermesi gereken sezon sonu puan aralıkları yaklaşık %70'ini içerdi. Güç çekilişini modelin tamamına uygulamak bunu geliştirme sezonlarında düzeltti ama ayrılmış sezonları kötüleştirdi; bu yüzden yayına alınmadı (bkz. *Sezon düzeyinde değerlendirme*).
- Beraberlikler biraz az, goller biraz fazla tahmin edilir (bkz. *Kalibrasyon*).
- Tahmin kalitesi puanlara, geçmiş veri kapsamına, model varsayımlarına ve deneme sayısına bağlıdır.

## Lisans

Kod [MIT Lisansı](LICENSE) ile yayımlanır. Depodaki veriler kendi koşullarına tabidir ve bunlar [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md) içinde listelenir: maç arşivi MIT, openfootball CC0 lisanslıdır; tff.org'un herkese açık sayfalarından alınan 2025-26 sonuçları ile 2026-27 fikstürü ve sonuçları MIT Lisansı kapsamında değildir.
