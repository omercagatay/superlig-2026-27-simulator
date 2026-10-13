import { createContext, useContext, useEffect, useState, type ReactNode } from "react";

export type Lang = "en" | "tr";

/* Every user-visible string lives here. Keys read as English so the code
   stays legible; Turkish is the other half of the pair, not an afterthought —
   this is a Süper Lig tool and most of its readers are Turkish. */
const DICT = {
  sims: ["Sims", "Simülasyon"],
  seed: ["Seed", "Tohum"],
  updateLive: ["Update live data", "Canlı veriyi güncelle"],
  updating: ["Updating…", "Güncelleniyor…"],
  noForecast: ["No forecast yet", "Henüz tahmin yok"],
  noForecastBody: [
    "Run the simulation to see how the 2026-27 season plays out.",
    "2026-27 sezonunun nasıl geçeceğini görmek için simülasyonu çalıştırın.",
  ],
  runSimulation: ["Run simulation", "Simülasyonu çalıştır"],

  // Shared race and calendar copy
  calendarLoading: ["calendar loading", "fikstür yükleniyor"],

  titleRace: ["Title race", "Şampiyonluk yarışı"],
  aboveWhom: ["Who finishes above whom", "Kim kimin üstünde bitirir"],
  /* Turkish word order would need a genitive suffix that varies by club name
     ("Trabzonspor'un üstünde"), so the comparison uses a symbol there — which
     the panel heading already explains. */
  above: ["above", ">"],

  // Season outcomes table
  seasonOutcomes: ["Season outcomes", "Sezon çıktıları"],
  club: ["Club", "Kulüp"],
  title: ["Title", "Şampiyon"],
  topTwo: ["Top two", "İlk iki"],
  topFour: ["Top four", "İlk dört"],
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

  // Matches
  matchday: ["Matchday", "Hafta"],

  // What-if
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
  accuracyEmpty: [
    "Predictions are frozen before kick-off and scored once the matches are played. Nothing to score yet — the tracker fills in from the next completed matchday.",
    "Tahminler maç öncesinde dondurulur ve maçlar oynandıkça puanlanır. Henüz puanlanacak bir şey yok — takip, tamamlanan ilk haftadan itibaren dolmaya başlar.",
  ],
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
  date: ["Date", "Tarih"],
  opponent: ["Opponent", "Rakip"],
  winCol: ["Win", "Galibiyet"],
  drawCol: ["Draw", "Beraberlik"],
  runIn: ["Run-in", "Kalan fikstür"],
  toPlayAvg: ["to play, average win probability", "maç kaldı, ortalama galibiyet olasılığı"],
  hardest: ["Hardest", "En zor"],
  easiest: ["Easiest", "En kolay"],

  // Live
  liveSource: [
    "Live results scraped from the Türkiye Futbol Federasyonu fixture page",
    "Canlı sonuçlar Türkiye Futbol Federasyonu fikstür sayfasından alınmıştır",
  ],
  fetched: ["fetched", "alındı"],

  // Masthead and contents
  mastKicker: ["Trendyol Süper Lig · 2026–27", "Trendyol Süper Lig · 2026–27"],
  mastTitle: ["Matchday Programme", "Maç Programı"],
  issue: ["Issue", "Sayı"],
  contents: ["Contents", "İçindekiler"],
  pageCover: ["Cover", "Kapak"],
  pageFixtures: ["This Week", "Bu Hafta"],
  pageTable: ["Table", "Puan Durumu"],
  pageRaces: ["Races", "Yarışlar"],
  pageResults: ["Results", "Sonuçlar"],
  pageAbbr: ["p.", "s."],
  conditionalEdition: ["What-if edition", "Varsayımlı baskı"],

  // Cover
  coverStandfirst: [
    "seasons simulated from today's table onwards.",
    "sezon, bugünkü tablodan itibaren simüle edildi.",
  ],
  dropLine: ["Drop zone", "Küme hattı"],
  thisWeek: ["This week", "Bu hafta"],
  allFixtures: ["All fixtures", "Tüm maçlar"],
  noFixturesLeft: ["No fixtures left this season.", "Bu sezon oynanacak maç kalmadı."],
  reportCard: ["Forecast report card", "Tahmin karnesi"],
  scoredMatches: ["Matches scored", "Puanlanan maç"],
  logLossNote: [
    "Lower log-loss is better; a minus means better than always guessing the league's base rates.",
    "Log kaybı ne kadar düşükse o kadar iyi; eksi değer, ligin genel oranlarını tahmin etmekten iyi demektir.",
  ],

  // This week
  fixturesNote: [
    "Chances come from the model alone, never from bookmakers. Stamp a result to assume it, then reprint to see how the season changes.",
    "Olasılıklar yalnızca modelden gelir, bahis oranı kullanılmaz. Bir sonucu damgalayıp varsayın, sezonun nasıl değiştiğini görmek için yeniden basın.",
  ],
  prevIssue: ["Previous matchday", "Önceki hafta"],
  nextIssue: ["Next matchday", "Sonraki hafta"],
  outHome: ["Home win", "Ev sahibi"],
  outDraw: ["Draw", "Beraberlik"],
  outAway: ["Away win", "Deplasman"],
  over25Short: ["over 2.5", "2,5 üst"],
  bttsShort: ["both score", "KG var"],
  likely: ["Likely", "Olası skor"],
  playedThisWeek: ["Played", "Oynananlar"],
  reprintWithPins: ["Reprint with these", "Bunlarla yeniden bas"],
  stampHome: ["H", "E"],
  stampDraw: ["D", "B"],
  stampAway: ["A", "D"],
  pinsLabel: ["What if", "Ya olursa"],

  // Table
  tapForClub: ["Tap a club for its page.", "Kulüp sayfası için bir kulübe dokunun."],
  colPos: ["Pos", "Sıra"],
  colP: ["P", "O"],
  colW: ["W", "G"],
  colD: ["D", "B"],
  colL: ["L", "M"],
  colF: ["F", "A"],
  colA: ["A", "Y"],
  colGD: ["GD", "Av"],
  colPts: ["Pts", "P"],
  resW: ["W", "G"],
  resD: ["D", "B"],
  resL: ["L", "M"],

  // Races
  relegationShort: ["Rel.", "Düşme"],
  outcomesNote: [
    "The share of simulated seasons in which each club finishes in each band. Tap a club for its page.",
    "Her kulübün simüle edilmiş sezonlarda her aralıkta bitirme oranı. Kulüp sayfası için bir kulübe dokunun.",
  ],

  // Results
  matchesPlayedLower: ["played", "maç oynandı"],
  remainingLower: ["to play", "maç kaldı"],
  goalsLower: ["goals", "gol"],
  goalsPerMatch: ["goals per match", "maç başına gol"],
  noLiveData: [
    "No live results yet. Use \"Update live data\" in the printer's note below.",
    "Henüz canlı sonuç yok. Aşağıdaki baskı notundaki \"Canlı veriyi güncelle\" ile alın.",
  ],

  // Printer's note
  printRun: ["Print run", "Baskı"],
  seasonsWord: ["seasons", "sezon"],
  lastUpdate: ["last update", "son güncelleme"],
  reprint: ["Reprint", "Yeniden bas"],
  printing: ["Printing…", "Basılıyor…"],
  nightEdition: ["Night edition", "Gece baskısı"],
  notAdvice: [
    "An independent model forecast — not betting advice.",
    "Bağımsız model tahminidir, bahis tavsiyesi değildir.",
  ],
  sourcesNote: [
    "Results from the Turkish Football Federation (TFF).",
    "Sonuçlar Türkiye Futbol Federasyonu'ndan (TFF) alınır.",
  ],
  inPress: ["In press", "Baskıda"],
  inPressBody: ["Simulating the season…", "Sezon simüle ediliyor…"],
  pressError: ["Print error", "Baskı hatası"],
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
