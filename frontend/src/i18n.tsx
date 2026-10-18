import { createContext, useContext, useEffect, useState, type ReactNode } from "react";
import type { Lang } from "./clubs";

export type { Lang };

/* Every user-visible string, as [English, Turkish]. Keys read as English so
   the code stays legible. */
const DICT = {
  siteName: ["Süper Lig 2026–27 forecast", "Süper Lig 2026–27 tahmini"],
  pages: ["Pages", "Sayfalar"],
  pageOverview: ["Overview", "Özet"],
  pageFixtures: ["Fixtures", "Maçlar"],
  pageTable: ["Table", "Puan durumu"],
  pageProbabilities: ["Probabilities", "Olasılıklar"],
  pageResults: ["Results", "Sonuçlar"],

  // App states
  whatIfNotice: [
    "This forecast assumes the results you picked",
    "Bu tahmin seçtiğiniz sonuçları varsayıyor",
  ],
  picksRemoved: [
    "Assumed results were removed for matches that have now been played",
    "Oynanmış maçlar için varsaydığınız sonuçlar kaldırıldı",
  ],
  simulating: ["Simulating the season…", "Sezon simüle ediliyor…"],
  errorLabel: ["Error", "Hata"],
  serverBusy: [
    "The server is busy. Please try again in a moment.",
    "Sunucu şu anda yoğun. Lütfen biraz sonra yeniden deneyin.",
  ],
  tooManyRequests: [
    "Too many requests — try again in {n} s.",
    "Çok fazla istek — {n} sn sonra tekrar deneyin.",
  ],
  tooManyRequestsSoon: [
    "Too many requests — try again in a moment.",
    "Çok fazla istek — biraz sonra tekrar deneyin.",
  ],
  settingsRejected: ["These settings were not accepted.", "Bu ayarlar kabul edilmedi."],
  tffUnreachable: ["Could not reach TFF for live data.", "Canlı veri için TFF'ye ulaşılamadı."],
  connectionFailed: [
    "Connection failed. Please try again.",
    "Bağlantı kurulamadı. Lütfen yeniden deneyin.",
  ],
  requestFailed: ["The request failed.", "İstek başarısız oldu."],
  crashed: [
    "Something went wrong. Reload the page to try again.",
    "Bir sorun oluştu. Yeniden denemek için sayfayı yenileyin.",
  ],
  reload: ["Reload", "Sayfayı yenile"],
  noForecast: ["No forecast yet.", "Henüz tahmin yok."],
  runSimulation: ["Run simulation", "Simülasyonu çalıştır"],
  calendarLoading: ["Loading fixtures…", "Fikstür yükleniyor…"],
  calendarUnavailable: [
    "Fixtures could not be loaded. Try Update live data at the bottom of the page.",
    "Fikstür yüklenemedi. Sayfanın altındaki Canlı veriyi güncelle düğmesiyle yeniden deneyin.",
  ],

  // Overview
  simulatedLead: ["seasons simulated from the current table.", "sezon, güncel puan durumundan itibaren simüle edildi."],
  thisWeek: ["This week", "Bu hafta"],
  allFixtures: ["All fixtures", "Tüm maçlar"],
  noFixturesLeft: ["No fixtures left this season.", "Bu sezon oynanacak maç kalmadı."],
  colHome: ["Home", "Ev"],
  colDraw: ["Draw", "Ber."],
  colAway: ["Away", "Dep."],

  // Shared race copy
  club: ["Club", "Kulüp"],
  title: ["Title", "Şampiyonluk"],
  topTwo: ["Top two", "İlk iki"],
  thirdFourth: ["3rd–4th", "3.–4."],
  topFour: ["Top four", "İlk dört"],
  relegation: ["Relegation", "Küme düşme"],
  noClubAbove: ["No club is above 0.5%.", "%0,5'in üzerinde kulüp yok."],

  // Fixtures
  matchday: ["Matchday", "Hafta"],
  prevMatchday: ["Previous matchday", "Önceki hafta"],
  nextMatchday: ["Next matchday", "Sonraki hafta"],
  fixturesNote: [
    "Probabilities come from the model only. No bookmaker odds are used. Pick a result to see how it changes the season.",
    "Olasılıklar yalnızca modelden gelir, bahis oranı kullanılmaz. Bir sonuç seçerek sezonu nasıl değiştirdiğini görebilirsiniz.",
  ],
  assumedResults: ["Assumed results", "Varsayılan sonuçlar"],
  recalculate: ["Recalculate", "Yeniden hesapla"],
  recalculating: ["Calculating…", "Hesaplanıyor…"],
  clearPicks: ["Clear", "Temizle"],
  pickLimit: [
    "You can assume at most {n} results. Remove one to pick another.",
    "En fazla {n} sonuç varsayabilirsiniz. Başka bir maç seçmek için birini kaldırın.",
  ],
  pickLabel: ["Assume", "Varsay"],
  pickHome: ["Home", "Ev"],
  pickDraw: ["Draw", "Beraberlik"],
  pickAway: ["Away", "Deplasman"],
  outHome: ["Home win", "Ev sahibi"],
  outDraw: ["Draw", "Beraberlik"],
  outAway: ["Away win", "Deplasman"],
  threePlusGoals: ["3+ goals", "3+ gol"],
  bothScore: ["both teams score", "iki takım da gol atar"],
  likely: ["Likely scores", "Olası skorlar"],
  playedThisWeek: ["Played", "Oynanan maçlar"],

  // Table
  projectedTable: ["Projected final table", "Tahmini sezon sonu puan durumu"],
  colPos: ["Pos", "Sıra"],
  colP: ["P", "O"],
  colW: ["W", "G"],
  colD: ["D", "B"],
  colL: ["L", "M"],
  colF: ["F", "A"],
  colA: ["A", "Y"],
  colGD: ["GD", "Av"],
  colPts: ["Pts", "P"],

  // Probabilities
  seasonOutcomes: ["Season outcomes", "Sezon sonu olasılıkları"],
  topTwoShort: ["Top 2", "İlk 2"],
  thirdShort: ["3rd", "3."],
  fourthShort: ["4th", "4."],
  topFourShort: ["Top 4", "İlk 4"],
  relegationShort: ["Rel.", "Düşme"],
  xPts: ["Exp. pts", "Bkl. puan"],
  xGD: ["Exp. GD", "Bkl. av."],
  tipTitle: ["Finishes 1st", "1. bitirir"],
  tipTopTwo: ["Finishes 1st or 2nd", "1. veya 2. bitirir"],
  tipThird: ["Finishes exactly 3rd", "Tam olarak 3. bitirir"],
  tipFourth: ["Finishes exactly 4th", "Tam olarak 4. bitirir"],
  tipTopFour: ["Finishes from 1st through 4th", "1-4. sıralar arasında bitirir"],
  tipRelegation: ["Finishes in the bottom three", "Son üçte bitirir"],
  tipXpts: ["Expected points over 34 matches", "34 maçta beklenen puan"],
  tipXgd: ["Expected goal difference", "Beklenen averaj"],
  pointsNeeded: ["Points needed", "Gereken puan"],
  place: ["Place", "Sıra"],
  chance50: ["50% chance", "%50 ihtimalle"],
  chance75: ["75% chance", "%75 ihtimalle"],
  chance90: ["90% chance", "%90 ihtimalle"],
  champion: ["Champion", "Şampiyon"],
  lastTopTwoPlace: ["2nd place", "2. sıra"],
  lastTopFourPlace: ["4th place", "4. sıra"],
  lastSafePlace: ["15th place (safe)", "15. sıra (kalır)"],
  aboveWhom: ["Who finishes above whom", "Kim kimin üstünde bitirir"],
  /* Turkish would need a genitive suffix that varies by club name
     ("Trabzonspor'un üstünde"), so the comparison uses a symbol there. */
  above: ["above", ">"],
  finishingPositions: ["Finishing positions", "Sıralama olasılıkları"],
  topFourPlaces: ["Top four (1–4)", "İlk dört (1–4)"],
  relegationZone: ["Relegation (16–18)", "Küme düşme (16–18)"],

  // Accuracy
  accuracyTitle: ["Forecast accuracy", "Tahmin başarısı"],
  accuracyEmpty: ["No matches scored yet.", "Henüz puanlanan maç yok."],
  scoredMatches: ["Matches scored", "Puanlanan maç"],
  calledCorrectly: ["Called correctly", "Doğru bilinen"],
  logLoss: ["Log-loss", "Log kaybı"],
  vsBaseRates: ["vs. base rates", "Genel oranlara göre"],
  calibration: ["Calibration", "Kalibrasyon"],
  predicted: ["predicted", "tahmin"],
  happened: ["happened", "gerçekleşen"],

  // Forecast history ({name} placeholders are filled by history.ts `fill`)
  forecastHistory: ["Forecast history", "Tahmin geçmişi"],
  runningLogLoss: [
    "Log-loss so far, after each matchday · lower is better",
    "Haftalara göre birikimli log kaybı · düşük olan daha iyi",
  ],
  runningLogLossTable: ["Log-loss so far, after each matchday", "Haftalara göre birikimli log kaybı"],
  model: ["Model", "Model"],
  baseRates: ["Base rates", "Genel oranlar"],
  chartSummary: [
    "Log-loss after matchday {round}: model {model}, base rates {base}. Lower is better.",
    "{round}. haftadan sonra log kaybı: model {model}, genel oranlar {base}. Düşük olan daha iyi.",
  ],
  matchdayCalled: ["Called {hits} of {n}", "{n} maçta {hits} doğru"],
  matchdayLogLoss: ["log-loss {model} vs {base} for base rates", "log kaybı {model}, genel oranlar {base}"],
  forecastMade: ["forecast made {date}", "tahmin tarihi {date}"],
  calledIt: ["called correctly", "doğru bilindi"],
  missedIt: ["missed", "bilinemedi"],
  matchesCol: ["Matches", "Maç"],

  // Club page
  close: ["Close", "Kapat"],
  expectedPoints: ["Expected points", "Beklenen puan"],
  averageFinish: ["Average finish", "Ortalama sıra"],
  whereItFinishes: ["Finishing position", "Bitiriş sırası"],
  played: ["Results", "Oynanan maçlar"],
  remainingFixtures: ["Remaining fixtures", "Kalan maçlar"],
  md: ["MD", "H"],
  date: ["Date", "Tarih"],
  opponent: ["Opponent", "Rakip"],
  winCol: ["Win", "Galibiyet"],
  drawCol: ["Draw", "Beraberlik"],
  runIn: ["Remaining", "Kalan"],
  toPlayAvg: ["matches, average win chance", "maç, ortalama galibiyet olasılığı"],
  hardest: ["Hardest", "En zor"],
  easiest: ["Easiest", "En kolay"],
  resW: ["W", "G"],
  resD: ["D", "B"],
  resL: ["L", "M"],

  // Results
  matchesPlayedLower: ["played", "maç oynandı"],
  remainingLower: ["to play", "maç kaldı"],
  goalsLower: ["goals", "gol"],
  goalsPerMatch: ["goals per match", "gol/maç"],

  // Footer
  sims: ["Simulations", "Simülasyon"],
  seed: ["Seed", "Tohum"],
  run: ["Run", "Çalıştır"],
  running: ["Running…", "Çalışıyor…"],
  updateLive: ["Update live data", "Canlı veriyi güncelle"],
  updating: ["Updating…", "Güncelleniyor…"],
  darkMode: ["Dark mode", "Koyu tema"],
  lastUpdate: ["Live data updated", "Canlı veri güncellendi"],
  notAdvice: [
    "Independent model forecast. Not betting advice.",
    "Bağımsız model tahminidir, bahis tavsiyesi değildir.",
  ],
  sourcesNote: [
    "Results from the Turkish Football Federation (TFF).",
    "Sonuçlar Türkiye Futbol Federasyonu'ndan (TFF) alınır.",
  ],
} satisfies Record<string, [string, string]>;

export type Key = keyof typeof DICT;

const LangContext = createContext<{ lang: Lang; setLang: (l: Lang) => void }>({
  lang: "en",
  setLang: () => {},
});

function initialLang(): Lang {
  try {
    const saved = localStorage.getItem("lang");
    if (saved === "tr" || saved === "en") return saved;
  } catch {
    /* site data blocked: the choice just isn't remembered */
  }
  // A Turkish browser gets a Turkish page by default; anyone can switch.
  return navigator.language?.toLowerCase().startsWith("tr") ? "tr" : "en";
}

export function LangProvider({ children }: { children: ReactNode }) {
  const [lang, setLang] = useState<Lang>(initialLang);
  useEffect(() => {
    try {
      localStorage.setItem("lang", lang);
    } catch {
      /* site data blocked */
    }
    document.documentElement.lang = lang;
  }, [lang]);
  return <LangContext.Provider value={{ lang, setLang }}>{children}</LangContext.Provider>;
}

/** `const t = useT()` then `t("title")`. */
export function useT() {
  const { lang } = useContext(LangContext);
  return (key: Key) => DICT[key][lang === "tr" ? 1 : 0];
}

export function useLang() {
  return useContext(LangContext);
}
