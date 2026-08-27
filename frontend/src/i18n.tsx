import { createContext, useContext, useEffect, useState, type ReactNode } from "react";

export type Lang = "en" | "tr";

/* Every user-visible string lives here. Keys read as English so the code
   stays legible; Turkish is the other half of the pair, not an afterthought —
   this is a Süper Lig tool and most of its readers are Turkish. */
const DICT = {
  brandSub: ["2026-27 Monte Carlo season simulator", "2026-27 Monte Carlo sezon simülatörü"],
  tabForecast: ["Forecast", "Tahmin"],
  tabPositions: ["Positions", "Sıralama"],
  tabRaces: ["Races", "Yarışlar"],
  tabTable: ["Table", "Puan durumu"],
  tabLive: ["Live", "Canlı"],
  sims: ["Sims", "Simülasyon"],
  seed: ["Seed", "Tohum"],
  run: ["Run", "Çalıştır"],
  running: ["Running…", "Çalışıyor…"],
  updateLive: ["Update live data", "Canlı veriyi güncelle"],
  updating: ["Updating…", "Güncelleniyor…"],
  updated: ["updated", "güncellendi"],
  noForecast: ["No forecast yet", "Henüz tahmin yok"],
  noForecastBody: [
    "Run the simulation to see how the 2026-27 season plays out.",
    "2026-27 sezonunun nasıl geçeceğini görmek için simülasyonu çalıştırın.",
  ],
  runSimulation: ["Run simulation", "Simülasyonu çalıştır"],
  simulating: ["Simulating", "Simüle ediliyor"],
  simulatingBody: ["seasons, in parallel. A few seconds.", "sezon, paralel olarak. Birkaç saniye."],
  toLight: ["Switch to light theme", "Açık temaya geç"],
  toDark: ["Switch to dark theme", "Koyu temaya geç"],

  // Forecast summary tiles
  titleFavorite: ["Title favourite", "Şampiyonluk favorisi"],
  relegationRisk: ["Relegation risk", "Küme düşme riski"],
  seasonProgress: ["Season progress", "Sezon ilerlemesi"],
  nextMatchday: ["Next matchday", "Gelecek hafta"],
  ofSeasons: ["of seasons", "sezonda"],
  fairOddsShort: ["fair odds", "adil oran"],
  fixturesToPlay: ["fixtures to play", "maç oynanacak"],
  fixtureToPlay: ["fixture to play", "maç oynanacak"],
  calendarLoading: ["calendar loading", "fikstür yükleniyor"],

  titleRace: ["Title race", "Şampiyonluk yarışı"],
  seasonsSeed: ["seasons · seed", "sezon · tohum"],
  aboveWhom: ["Who finishes above whom", "Kim kimin üstünde bitirir"],
  /* Turkish word order would need a genitive suffix that varies by club name
     ("Trabzonspor'un üstünde"), so the comparison uses a symbol there — which
     the panel heading already explains. */
  above: ["above", ">"],
  allLiveResults: ["All live results →", "Tüm canlı sonuçlar →"],

  // Season outcomes table
  seasonOutcomes: ["Season outcomes", "Sezon çıktıları"],
  clickAClub: ["Click a club", "Bir kulübe tıklayın"],
  club: ["Club", "Kulüp"],
  title: ["Title", "Şampiyon"],
  odds: ["Odds", "Oran"],
  topTwo: ["Top two", "İlk iki"],
  topFour: ["Top four", "İlk dört"],
  thirdPlace: ["3rd place", "3. sıra"],
  fourthPlace: ["4th place", "4. sıra"],
  topTwoShort: ["Top 2", "İlk 2"],
  thirdShort: ["3rd", "3."],
  fourthShort: ["4th", "4."],
  topFourShort: ["Top 4", "İlk 4"],
  relegation: ["Relegation", "Küme düşme"],
  xPts: ["xPts", "bPuan"],
  xGD: ["xGD", "bAverage"],
  tipTopTwo: ["Finishes 1st or 2nd", "1. veya 2. bitirir"],
  tipThird: ["Finishes exactly 3rd", "Tam olarak 3. bitirir"],
  tipFourth: ["Finishes exactly 4th", "Tam olarak 4. bitirir"],
  tipTopFour: ["Finishes from 1st through 4th", "1-4. sıralar arasında bitirir"],
  tipRelegation: ["Finishes in the bottom three", "Son üçte bitirir"],
  tipXpts: ["Expected points over 34 matches", "34 maçta beklenen puan"],
  tipXgd: ["Expected goal difference", "Beklenen averaj"],
  tipDecimalOdds: ["Decimal odds", "Ondalık oran"],

  // Matches
  matchPredictions: ["Match predictions", "Maç tahminleri"],
  matchday: ["Matchday", "Hafta"],
  prevMatchday: ["Previous matchday", "Önceki hafta"],
  nextMatchdayAria: ["Next matchday", "Sonraki hafta"],
  matchesNote: [
    "Fair odds are 100 / probability, with no bookmaker margin. Model estimates, not betting advice.",
    "Adil oran, 100 / olasılık şeklindedir ve bahis marjı içermez. Model tahminidir, bahis tavsiyesi değildir.",
  ],
  finalScores: ["Final scores", "Maç sonuçları"],
  market: ["Market", "Bahis"],
  probability: ["Probability", "Olasılık"],
  fairOdds: ["Fair odds", "Adil oran"],
  bookmaker: ["Bookmaker", "Bahis oranı"],
  win: ["win", "kazanır"],
  draw: ["Draw", "Beraberlik"],
  over25: ["Over 2.5 goals", "2.5 üstü gol"],
  under25: ["Under 2.5 goals", "2.5 altı gol"],
  btts: ["Both teams score", "Karşılıklı gol"],
  mostLikelyScores: ["Most likely scores", "En olası skorlar"],
  vsBookmaker: ["vs bookmaker", "bahis oranına karşı"],
  ptsOn: ["pts on", "puan farkı:"],
  bookMargin: ["book margin", "bahis marjı"],

  // What-if
  whatIf: ["What if…", "Ya olsaydı…"],
  whatIfNote: [
    "Pin results you want to assume, then run again. The outcome is fixed; the model still decides the scoreline.",
    "Varsaymak istediğiniz sonuçları sabitleyin ve yeniden çalıştırın. Sonuç sabittir; skoru yine model belirler.",
  ],
  clearPins: ["Clear", "Temizle"],
  assuming: ["Assuming", "Varsayım"],
  pinHome: ["Assume home win", "Ev sahibi kazanır"],
  pinDraw: ["Assume draw", "Beraberlik"],
  pinAway: ["Assume away win", "Deplasman kazanır"],

  // Positions grid
  finishingPositions: ["Finishing position probabilities", "Sıralama olasılıkları"],
  finishingNote: [
    "Each cell is the share of simulated seasons in which a club finished in that position. Rows are ordered by average finish.",
    "Her hücre, bir kulübün o sırada bitirdiği simüle edilmiş sezonların oranıdır. Satırlar ortalama sıralamaya göre dizilmiştir.",
  ],
  topFourPlaces: ["Top-four places (1-4)", "İlk dört sıra (1-4)"],
  relegationZone: ["Relegation (16-18)", "Küme düşme (16-18)"],

  // Projected table
  projectedTable: ["Projected final table", "Tahminî puan durumu"],
  projectedNote: [
    "Each club's expected record, averaged over every simulated season and ranked by expected points — the table the model considers most likely, not one sampled run.",
    "Her kulübün tüm simüle edilmiş sezonlar boyunca ortalanmış beklenen karnesi, beklenen puana göre sıralanmıştır — modelin en olası gördüğü tablo, tek bir örnek sezon değil.",
  ],
  // Races
  whatItTakes: ["What it takes", "Ne gerekiyor"],
  whatItTakesNote: [
    "Points held by the club that actually finished in each place, across every simulated season. Read a row as: reach the 90% column and you would have taken that place in nine seasons out of ten.",
    "Her sırayı alan kulübün, tüm simüle edilmiş sezonlardaki puanı. Bir satırı şöyle okuyun: %90 sütununa ulaşırsanız on sezonun dokuzunda o sırayı almış olurdunuz.",
  ],
  place: ["Place", "Sıra"],
  halfTheTime: ["Half the time", "Yarı yarıya"],
  threeInFour: ["3 in 4", "4'te 3"],
  nineInTen: ["9 in 10", "10'da 9"],
  champion: ["Champion", "Şampiyon"],
  lastTopTwoPlace: ["Last top-two place", "İlk iki için son sıra"],
  lastTopFourPlace: ["Last top-four place", "İlk dört için son sıra"],
  lastSafePlace: ["Last safe place", "Son kalan sıra"],
  raceTitleNote: ["Finishes 1st", "1. bitirir"],
  raceTopTwoNote: ["Finishes 1st or 2nd", "1. veya 2. bitirir"],
  raceTopFourNote: ["Finishes in the top 4", "İlk 4'te bitirir"],
  raceRelNote: ["Finishes in the bottom 3", "Son 3'te bitirir"],
  noClubAbove: ["No club is above 0.5%.", "%0,5'in üzerinde kulüp yok."],

  // Accuracy
  forecastAccuracy: ["Forecast accuracy", "Tahmin isabeti"],
  accuracyEmpty: [
    "Predictions are frozen before kick-off and scored once the matches are played. Nothing to score yet — the tracker fills in from the next completed matchday.",
    "Tahminler maç öncesinde dondurulur ve maçlar oynandıkça puanlanır. Henüz puanlanacak bir şey yok — takip, tamamlanan ilk haftadan itibaren dolmaya başlar.",
  ],
  scored: ["scored", "puanlandı"],
  calledCorrectly: ["Called correctly", "Doğru bilinen"],
  logLoss: ["Log-loss", "Log kaybı"],
  vsBaseRates: ["vs base rates", "temel oranlara karşı"],
  calibration: ["Calibration", "Kalibrasyon"],
  said: ["said", "dedi"],
  happened: ["happened", "gerçekleşti"],

  // Club detail
  close: ["Close", "Kapat"],
  expectedPoints: ["Expected points", "Beklenen puan"],
  averageFinish: ["Average finish", "Ortalama sıra"],
  whereItFinishes: ["Where it finishes", "Nerede bitirir"],
  played: ["Played", "Oynanan"],
  remainingFixtures: ["Remaining fixtures", "Kalan maçlar"],
  md: ["MD", "H"],
  mdShort: ["MD", "Hafta"],
  date: ["Date", "Tarih"],
  opponent: ["Opponent", "Rakip"],
  winCol: ["Win", "Galibiyet"],
  drawCol: ["Draw", "Beraberlik"],
  runIn: ["Run-in", "Kalan fikstür"],
  toPlayAvg: ["to play, average win probability", "maç kaldı, ortalama galibiyet olasılığı"],
  hardest: ["Hardest", "En zor"],
  easiest: ["Easiest", "En kolay"],

  // Live
  matchesPlayed: ["Matches played", "Oynanan maç"],
  remaining: ["Remaining", "Kalan"],
  goalsScored: ["Goals scored", "Atılan gol"],
  liveSource: [
    "Live results scraped from the Türkiye Futbol Federasyonu fixture page",
    "Canlı sonuçlar Türkiye Futbol Federasyonu fikstür sayfasından alınmıştır",
  ],
  fetched: ["fetched", "alındı"],
  projectedChampion: ["Projected champion", "Tahminî şampiyon"],
  matchesRemaining: ["Matches remaining", "Kalan maç"],
  mostLikelyChampion: ["Most likely champion", "En olası şampiyon"],
} satisfies Record<string, [string, string]>;

export type Key = keyof typeof DICT;

const LangContext = createContext<{ lang: Lang; setLang: (l: Lang) => void }>({
  lang: "en",
  setLang: () => {},
});

function initialLang(): Lang {
  const saved = localStorage.getItem("lang");
  if (saved === "tr" || saved === "en") return saved;
  // A Turkish browser gets a Turkish page by default; anyone can switch.
  return navigator.language?.toLowerCase().startsWith("tr") ? "tr" : "en";
}

export function LangProvider({ children }: { children: ReactNode }) {
  const [lang, setLang] = useState<Lang>(initialLang);
  useEffect(() => {
    localStorage.setItem("lang", lang);
    document.documentElement.lang = lang;
  }, [lang]);
  return <LangContext.Provider value={{ lang, setLang }}>{children}</LangContext.Provider>;
}

/** `const t = useT()` then `t("titleRace")`. */
export function useT() {
  const { lang } = useContext(LangContext);
  return (key: Key) => DICT[key][lang === "tr" ? 1 : 0];
}

export function useLang() {
  return useContext(LangContext);
}

/** Locale tag for Intl formatting, so dates read natively in both languages. */
export function useLocale() {
  const { lang } = useContext(LangContext);
  return lang === "tr" ? "tr-TR" : undefined;
}
