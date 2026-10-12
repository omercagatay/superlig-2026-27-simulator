# Matchday Programme UI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the dashboard UI with a printed Turkish matchday programme (maç programı): five numbered pages, club colours as scarf swatches, Turkish typesetting, day and night editions.

**Architecture:** Presentation-only rewrite of `frontend/src`. A pure module `clubs.ts` owns club colours and every number/date format. Small print primitives (`Print.tsx`) build five page components; `App.tsx` becomes a shell (masthead, contents strip, page, printer's note) that keeps today's data-loading state. `styles.css` is rewritten around two token sets. Backend and `api.ts` are untouched.

**Tech Stack:** React 18, TypeScript 5, Vite 8, self-hosted fonts via `@fontsource` (Big Shoulders Display, Newsreader, Archivo Narrow), Node 22 `node:test` for unit tests, Playwright (scratch only) for visual checks.

**Spec:** `docs/superpowers/specs/2026-10-13-matchday-programme-ui-design.md`

## Global Constraints

- No betting framing anywhere in the UI: no fair odds, no decimal odds, no "1X2" market labels. Chances are percentages only.
- Clubs are shown by colours and names only; never crests or logos.
- Day edition: paper `#F2ECDF`, ink `#1E1B17`, spot red `#C1272D`. Night edition: paper `#0F1724`, ink `#EDE6D6`, spot red `#E2514F`.
- Fonts: only `@fontsource/big-shoulders-display`, `@fontsource/newsreader`, `@fontsource/archivo-narrow` (5.3.0). No other font packages, no font CDN.
- Turkish edition prints `%59,9`, decimal commas, Turkish dates; English edition prints `59.9%`. Every number, percentage and date goes through `src/clubs.ts`.
- Club names are always rendered inside an element with `lang="tr"` (via `ClubName`), never uppercased in JavaScript.
- At 390 px (and 360 px) the document never scrolls horizontally; wide tables scroll inside `.table-frame`.
- Body and caption text contrast ≥ 4.5:1 against paper in both editions.
- `api.ts` and the Rust backend do not change. No new runtime dependencies beyond the three font packages.
- Work is committed locally; nothing is pushed or deployed until the user has reviewed screenshots.

## Review Focus

- English edition with Turkish club names (Başakşehir, Gençlerbirliği, Kasımpaşa): uppercase styling must give "BAŞAKŞEHİR", never "BAŞAKŞEHIR" — Task 8 asserts every `.club` carries `lang="tr"`.
- 360–390 px phones: no page-level sideways scroll on any of the five pages, either edition, either language — Task 8 asserts `scrollWidth <= innerWidth`.
- `/api/accuracy` or `/api/live` failing: the cover still renders and the results page shows a note instead of a blank screen — Task 8 aborts those routes and checks.
- A matchday where every fixture is played, and the first/last matchday: no stubs, a results column, and the ‹/› buttons disabled at the ends — Task 8 checks matchday 1 and 34.
- Zero and trace chances (0, NaN, 0.02 %): print "–" and "<%0,1", never "%0,0" — Task 1 unit test.

---

## File Map

| File | Responsibility | Task |
|---|---|---|
| `frontend/src/clubs.ts` (new) | Club colour map; `pct`, `num`, `signed`, `shortDate`, `dateSpan`, `matchdayLabel`, `stamp`, `coverHeadline` | 1 |
| `frontend/src/clubs.test.mjs` (new) | Unit tests for `clubs.ts` | 1 |
| `frontend/src/styles.css` | Rewritten: tokens, base, primitives; each later task appends its section | 1–7 |
| `frontend/src/i18n.tsx` | New programme vocabulary (Task 1); obsolete keys pruned (Task 7) | 1, 7 |
| `frontend/src/main.tsx`, `package.json` | Font packages; `test` script | 1 |
| `.github/workflows/ci.yml` | Frontend job on Node 22.12, runs `npm test` | 1 |
| `frontend/src/components/Print.tsx` (new) | `Swatch`, `ClubName`, `Bar`, `PageHead`, `ResultLine` | 2, 3 |
| `frontend/src/components/AccuracyPanel.tsx` | Restyled as "Tahmin karnesi" | 2 |
| `frontend/src/pages/CoverPage.tsx` (new) | Page 1 | 2 |
| `frontend/src/pages/FixturesPage.tsx` (new) | Page 2: ticket stubs, stamps, played column | 3 |
| `frontend/src/pages/TablePage.tsx` (new) | Page 3: projected table | 4 |
| `frontend/src/components/ClubDetail.tsx` | Restyled club page (overlay) | 4 |
| `frontend/src/pages/RacesPage.tsx` (new) | Page 4: outcomes table, race cards, thresholds, rivals | 5 |
| `frontend/src/components/PositionGrid.tsx` | Restyled 18×18 grid | 5 |
| `frontend/src/pages/ResultsPage.tsx` (new) | Page 5: played results | 6 |
| `frontend/src/components/Masthead.tsx`, `Colophon.tsx` (new) | Masthead + contents strip; printer's note footer | 7 |
| `frontend/src/App.tsx`, `frontend/index.html` | Shell, hash routing, theme colours | 7 |
| Deleted in Task 7 | `ForecastView.tsx`, `MatchesView.tsx`, `ResultsTable.tsx`, `RacesView.tsx`, `LeagueTable.tsx`, `LiveStats.tsx` | 7 |

All commands run from `frontend/` unless stated otherwise.

---

### Task 1: Foundation — formatting module, fonts, copy deck, base stylesheet

**Files:**
- Create: `frontend/src/clubs.ts`, `frontend/src/clubs.test.mjs`
- Modify: `frontend/package.json`, `frontend/package-lock.json`, `frontend/src/main.tsx`, `frontend/src/i18n.tsx` (add keys before `} satisfies`), `.github/workflows/ci.yml` (frontend job)
- Replace: `frontend/src/styles.css`

**Interfaces:**
- Produces (from `src/clubs.ts`): `type Lang = "en" | "tr"`; `interface ClubColours { c1: string; c2: string }`; `CLUB_COLOURS: Record<string, ClubColours>`; `clubColours(team: string): ClubColours`; `num(value: number, lang: Lang, digits = 0): string`; `pct(value: number, lang: Lang, digits = 1, dashZero = true): string`; `signed(value: number, lang: Lang, digits = 1): string`; `shortDate(iso: string, lang: Lang): string`; `dateSpan(isos: string[], lang: Lang): string`; `matchdayLabel(round: number, lang: Lang): string`; `stamp(rfc3339: string, lang: Lang): string | null`; `coverHeadline(team: string, titlePct: number, lang: Lang): string`.
- Produces: i18n keys listed in Step 6; CSS classes `.sheet .page .page-head .page-title .page-num .section-title .note .small-print .spread .club .club-flip .club-name .swatch .bar .bar-fill .bar-red .table-frame .print .l .rank .row-link .facts .btn .btn-red .erratum .in-press .legend .key .key-red .key-bold .sr-only .val`.

- [ ] **Step 1: Write the failing unit tests**

Create `frontend/src/clubs.test.mjs`:

```js
// npm test  (node --experimental-strip-types --test src/clubs.test.mjs)
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  CLUB_COLOURS,
  clubColours,
  coverHeadline,
  dateSpan,
  matchdayLabel,
  num,
  pct,
  shortDate,
  signed,
  stamp,
} from "./clubs.ts";

test("every club has two hex colours; an unknown club falls back to the inks", () => {
  assert.equal(Object.keys(CLUB_COLOURS).length, 18);
  for (const [club, c] of Object.entries(CLUB_COLOURS)) {
    assert.match(c.c1, /^#[0-9A-F]{6}$/i, club);
    assert.match(c.c2, /^#[0-9A-F]{6}$/i, club);
  }
  assert.deepEqual(clubColours("Real Madrid"), { c1: "var(--ink)", c2: "var(--paper)" });
});

test("percentages are typeset per edition, with dashes and traces", () => {
  assert.equal(pct(59.94, "tr"), "%59,9");
  assert.equal(pct(59.94, "en"), "59.9%");
  assert.equal(pct(51.3, "tr", 0), "%51");
  assert.equal(pct(0, "tr"), "–");
  assert.equal(pct(Number.NaN, "en"), "–");
  assert.equal(pct(0, "tr", 0, false), "%0");
  assert.equal(pct(0.02, "tr"), "<%0,1");
  assert.equal(pct(0.02, "en"), "<0.1%");
});

test("numbers use the edition's separators and a true minus", () => {
  assert.equal(num(50000, "tr"), "50.000");
  assert.equal(num(50000, "en"), "50,000");
  assert.equal(num(74.31, "tr", 1), "74,3");
  assert.equal(signed(43, "tr"), "+43,0");
  assert.equal(signed(-4.8, "en"), "−4.8");
  assert.equal(signed(-0.01, "en"), "0.0");
  assert.equal(signed(0, "tr", 0), "0");
});

test("dates read as a programme issue line", () => {
  assert.equal(dateSpan(["2026-10-19", "2026-10-16", "2026-10-17"], "tr"), "16–19 Ekim");
  assert.equal(dateSpan(["2026-10-16", "2026-10-19"], "en"), "16–19 October");
  assert.equal(dateSpan(["2026-09-30", "2026-10-02"], "tr"), "30 Eylül – 2 Ekim");
  assert.equal(dateSpan(["2026-10-17"], "en"), "17 October");
  assert.equal(dateSpan([], "tr"), "");
  assert.match(shortDate("2026-10-17", "tr"), /17 Eki/);
  assert.equal(shortDate("not-a-date", "tr"), "not-a-date");
  assert.equal(matchdayLabel(8, "tr"), "8. Hafta");
  assert.equal(matchdayLabel(8, "en"), "Matchday 8");
  assert.equal(stamp("garbage", "tr"), null);
  assert.match(stamp("2026-10-12T23:25:28Z", "tr"), /Eki/);
});

test("the cover headline says how settled the title race is", () => {
  assert.equal(coverHeadline("Galatasaray", 59.9, "tr"), "Zirvede Galatasaray: şampiyonluk ihtimali %59,9");
  assert.equal(coverHeadline("Galatasaray", 59.9, "en"), "Galatasaray on top: a 59.9% title chance");
  assert.equal(coverHeadline("Fenerbahçe", 31.2, "tr"), "Yarış açık: Fenerbahçe %31,2 ile önde");
  assert.equal(coverHeadline("Fenerbahçe", 31.2, "en"), "An open race: Fenerbahçe lead on 31.2%");
});
```

In `frontend/package.json`, add to `"scripts"`:

```json
"test": "node --experimental-strip-types --test src/clubs.test.mjs"
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `npm test`
Expected: FAIL — `Cannot find module '.../src/clubs.ts'`.

- [ ] **Step 3: Implement `src/clubs.ts`**

```ts
/* Programme typesetting: club colours, and every number, percentage and
   date the pages print. Pure functions with no React, so they are tested
   directly (clubs.test.mjs). Colours and names only — crests are
   trademarks and are never drawn. */

export type Lang = "en" | "tr";

export interface ClubColours {
  /** First scarf stripe. */
  c1: string;
  /** Second scarf stripe. */
  c2: string;
}

export const CLUB_COLOURS: Record<string, ClubColours> = {
  Galatasaray: { c1: "#FDB912", c2: "#A90432" },
  Fenerbahçe: { c1: "#FFED00", c2: "#002D72" },
  Beşiktaş: { c1: "#111111", c2: "#FFFFFF" },
  Trabzonspor: { c1: "#6E1E2E", c2: "#4FA3DB" },
  Başakşehir: { c1: "#F26722", c2: "#1B2A4A" },
  Amedspor: { c1: "#1E8F3E", c2: "#D21F26" },
  Göztepe: { c1: "#FFD200", c2: "#E30613" },
  Samsunspor: { c1: "#E30613", c2: "#FFFFFF" },
  Erzurumspor: { c1: "#0055A5", c2: "#FFFFFF" },
  Gençlerbirliği: { c1: "#E30613", c2: "#111111" },
  Rizespor: { c1: "#00843D", c2: "#0055A5" },
  Alanyaspor: { c1: "#F47920", c2: "#008D36" },
  Çorum: { c1: "#D71920", c2: "#111111" },
  Kocaelispor: { c1: "#00833E", c2: "#111111" },
  Eyüpspor: { c1: "#5B2C83", c2: "#FFD100" },
  Konyaspor: { c1: "#00843D", c2: "#FFFFFF" },
  Kasımpaşa: { c1: "#1B2A4A", c2: "#FFFFFF" },
  Gaziantep: { c1: "#E30613", c2: "#111111" },
};

const FALLBACK: ClubColours = { c1: "var(--ink)", c2: "var(--paper)" };

export function clubColours(team: string): ClubColours {
  return CLUB_COLOURS[team] ?? FALLBACK;
}

function locale(lang: Lang): string {
  return lang === "tr" ? "tr-TR" : "en-GB";
}

/** A plain number with the edition's separators: "50.000" / "50,000". */
export function num(value: number, lang: Lang, digits = 0): string {
  return new Intl.NumberFormat(locale(lang), {
    minimumFractionDigits: digits,
    maximumFractionDigits: digits,
  }).format(value);
}

/** "%59,9" in Turkish, "59.9%" in English. Zero or NaN prints "–" unless
 *  `dashZero` is false; a trace below half the last digit prints "<%0,1",
 *  so a real but tiny chance never reads as zero. */
export function pct(value: number, lang: Lang, digits = 1, dashZero = true): string {
  const format = (v: number) =>
    new Intl.NumberFormat(locale(lang), {
      style: "percent",
      minimumFractionDigits: digits,
      maximumFractionDigits: digits,
    }).format(v / 100);
  if (!Number.isFinite(value)) return "–";
  if (value <= 0) return dashZero ? "–" : format(0);
  const step = 10 ** -digits;
  if (value < step / 2) return `<${format(step)}`;
  return format(value);
}

/** Signed, with a true minus sign: "+43,0", "−4,8". Values that round to
 *  zero print unsigned. */
export function signed(value: number, lang: Lang, digits = 1): string {
  const magnitude = num(Math.abs(value), lang, digits);
  if (Number(Math.abs(value).toFixed(digits)) === 0) return magnitude;
  return value > 0 ? `+${magnitude}` : `−${magnitude}`;
}

/** ISO dates are calendar days: parse as local midnight so no time zone
 *  can shift them. */
function parseIso(iso: string): Date | null {
  const [y, m, d] = iso.split("-").map(Number);
  return y && m && d ? new Date(y, m - 1, d) : null;
}

/** A fixture date: "17 Eki Cmt" / "Sat 17 Oct". */
export function shortDate(iso: string, lang: Lang): string {
  const d = parseIso(iso);
  if (!d) return iso;
  return new Intl.DateTimeFormat(locale(lang), {
    weekday: "short",
    day: "numeric",
    month: "short",
  }).format(d);
}

/** A matchday's span for the issue line: "16–19 Ekim", or across months
 *  "30 Eylül – 2 Ekim". */
export function dateSpan(isos: string[], lang: Lang): string {
  const days = [...new Set(isos)]
    .sort()
    .map(parseIso)
    .filter((d): d is Date => d !== null);
  if (days.length === 0) return "";
  const first = days[0];
  const last = days[days.length - 1];
  const dayMonth = (d: Date) =>
    new Intl.DateTimeFormat(locale(lang), { day: "numeric", month: "long" }).format(d);
  if (first.getTime() === last.getTime()) return dayMonth(first);
  if (first.getMonth() === last.getMonth() && first.getFullYear() === last.getFullYear()) {
    const month = new Intl.DateTimeFormat(locale(lang), { month: "long" }).format(last);
    return `${first.getDate()}–${last.getDate()} ${month}`;
  }
  return `${dayMonth(first)} – ${dayMonth(last)}`;
}

/** "8. Hafta" / "Matchday 8". */
export function matchdayLabel(round: number, lang: Lang): string {
  return lang === "tr" ? `${round}. Hafta` : `Matchday ${round}`;
}

/** When the live data was fetched: "13 Eki 02:25"; null if unparseable. */
export function stamp(rfc3339: string, lang: Lang): string | null {
  const ms = Date.parse(rfc3339);
  if (!Number.isFinite(ms)) return null;
  return new Intl.DateTimeFormat(locale(lang), {
    day: "numeric",
    month: "short",
    hour: "2-digit",
    minute: "2-digit",
  }).format(new Date(ms));
}

/** The cover's lead line, written from the title favourite's chance. */
export function coverHeadline(team: string, titlePct: number, lang: Lang): string {
  const chance = pct(titlePct, lang);
  if (titlePct >= 50) {
    return lang === "tr"
      ? `Zirvede ${team}: şampiyonluk ihtimali ${chance}`
      : `${team} on top: a ${chance} title chance`;
  }
  return lang === "tr"
    ? `Yarış açık: ${team} ${chance} ile önde`
    : `An open race: ${team} lead on ${chance}`;
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `npm test`
Expected: `# pass 5`, `# fail 0`.

- [ ] **Step 5: Swap the fonts**

Run:

```bash
npm uninstall @fontsource/barlow @fontsource/barlow-condensed
npm install @fontsource/big-shoulders-display@5.3.0 @fontsource/newsreader@5.3.0 @fontsource/archivo-narrow@5.3.0
ls node_modules/@fontsource/newsreader/400-italic.css node_modules/@fontsource/big-shoulders-display/900.css node_modules/@fontsource/archivo-narrow/700.css
npm audit --audit-level=high
```

Expected: the three files listed; `found 0 vulnerabilities`.

Replace the six Barlow imports in `frontend/src/main.tsx` with:

```ts
import "@fontsource/big-shoulders-display/800.css";
import "@fontsource/big-shoulders-display/900.css";
import "@fontsource/newsreader/400.css";
import "@fontsource/newsreader/400-italic.css";
import "@fontsource/newsreader/600.css";
import "@fontsource/archivo-narrow/400.css";
import "@fontsource/archivo-narrow/600.css";
import "@fontsource/archivo-narrow/700.css";
```

- [ ] **Step 6: Add the programme vocabulary to `i18n.tsx`**

Insert before the line `} satisfies Record<string, [string, string]>;` (existing keys stay until Task 7):

```ts
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
```

- [ ] **Step 7: Replace `src/styles.css` with the base stylesheet**

Overwrite the whole file with:

```css
/* ============================================================
   Süper Lig 2026–27 · Maç Programı
   A printed matchday programme: two inks (black and programme red)
   on warm uncoated stock. Club colours appear only as scarf stripes.
   The night-match edition prints cream ink on night-blue paper.
   Type: Big Shoulders Display (signage), Newsreader (text),
   Archivo Narrow with tabular figures (tables and data).
   ============================================================ */

:root {
  color-scheme: light;
  --paper: #f2ecdf;
  --paper-2: #e8dfcc; /* recessed: tear-offs, tracks */
  --ink: #1e1b17;
  --ink-2: #4a443b;
  --ink-3: #5f574a; /* captions: 5.3:1 on --paper-2 */
  --rule: #1e1b17;
  --rule-soft: rgba(30, 27, 23, 0.2);
  --red: #c1272d;
  --red-ink: #a61f25; /* red for small text: 6.3:1 on --paper */
  --grain: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='180' height='180'%3E%3Cfilter id='g'%3E%3CfeTurbulence type='fractalNoise' baseFrequency='0.85' numOctaves='3' stitchTiles='stitch'/%3E%3CfeColorMatrix values='0 0 0 0 0.12 0 0 0 0 0.1 0 0 0 0 0.08 0 0 0 0.07 0'/%3E%3C/filter%3E%3Crect width='100%25' height='100%25' filter='url(%23g)'/%3E%3C/svg%3E");
  --display: "Big Shoulders Display", "Arial Narrow", sans-serif;
  --serif: "Newsreader", Georgia, "Times New Roman", serif;
  --narrow: "Archivo Narrow", "Arial Narrow", sans-serif;
  --gutter: 16px;
  --measure: 1200px;
}

/* Night-match edition: the evening-kickoff programme. */
:root[data-theme="dark"] {
  color-scheme: dark;
  --paper: #0f1724;
  --paper-2: #17223a;
  --ink: #ede6d6;
  --ink-2: #c3bba9;
  --ink-3: #958d7d;
  --rule: #ede6d6;
  --rule-soft: rgba(237, 230, 214, 0.22);
  --red: #e2514f;
  --red-ink: #ef6b68;
  --grain: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='180' height='180'%3E%3Cfilter id='g'%3E%3CfeTurbulence type='fractalNoise' baseFrequency='0.85' numOctaves='3' stitchTiles='stitch'/%3E%3CfeColorMatrix values='0 0 0 0 0.93 0 0 0 0 0.9 0 0 0 0 0.84 0 0 0 0.05 0'/%3E%3C/filter%3E%3Crect width='100%25' height='100%25' filter='url(%23g)'/%3E%3C/svg%3E");
}

@media (min-width: 760px) {
  :root {
    --gutter: 28px;
  }
}

*,
*::before,
*::after {
  box-sizing: border-box;
}

html {
  -webkit-text-size-adjust: 100%;
}

body {
  margin: 0;
  background-color: var(--paper);
  background-image: var(--grain);
  color: var(--ink);
  font-family: var(--serif);
  font-size: 17px;
  line-height: 1.45;
}

button,
input,
select {
  font: inherit;
  color: inherit;
}

:focus-visible {
  outline: 2px solid var(--red);
  outline-offset: 2px;
}

.sr-only {
  position: absolute;
  width: 1px;
  height: 1px;
  margin: -1px;
  padding: 0;
  overflow: hidden;
  clip: rect(0 0 0 0);
  white-space: nowrap;
  border: 0;
}

.sheet {
  max-width: var(--measure);
  margin: 0 auto;
  padding: 0 var(--gutter) 40px;
}

/* ---- Pages and type ---- */

.page {
  padding-top: 22px;
}

.page-head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 18px;
  padding-bottom: 6px;
  border-bottom: 1px solid var(--rule);
}

.page-title {
  margin: 0;
  font-family: var(--display);
  font-weight: 900;
  font-size: clamp(34px, 7vw, 56px);
  line-height: 0.9;
  letter-spacing: 0.01em;
  text-transform: uppercase;
}

.page-num {
  font-family: var(--narrow);
  font-weight: 700;
  font-size: 13px;
  color: var(--red-ink);
  white-space: nowrap;
}

.section-title {
  margin: 0 0 10px;
  padding-bottom: 4px;
  border-bottom: 3px double var(--rule);
  font-family: var(--display);
  font-weight: 800;
  font-size: 22px;
  line-height: 1.05;
  letter-spacing: 0.02em;
  text-transform: uppercase;
}

.note {
  max-width: 64ch;
  margin: 6px 0 16px;
  font-style: italic;
  font-size: 15.5px;
  color: var(--ink-2);
}

.small-print {
  margin: 4px 0 0;
  font-family: var(--narrow);
  font-size: 13.5px;
  color: var(--ink-2);
  font-variant-numeric: tabular-nums;
}

.val {
  text-align: right;
  font-family: var(--narrow);
  font-weight: 700;
  font-variant-numeric: tabular-nums;
}

.spread {
  display: grid;
  gap: 28px;
}

.spread > * {
  min-width: 0;
}

@media (min-width: 900px) {
  .spread {
    grid-template-columns: minmax(0, 1.4fr) minmax(0, 1fr);
    gap: 0;
  }
  .spread > :first-child {
    padding-right: 28px;
  }
  .spread > :last-child {
    padding-left: 28px;
    border-left: 1px solid var(--rule);
  }
}

/* ---- Clubs: scarf swatch + name ---- */

.club {
  display: inline-flex;
  align-items: center;
  gap: 7px;
  min-width: 0;
  max-width: 100%;
}

.club-flip {
  flex-direction: row-reverse;
}

.club-name {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.swatch {
  flex: none;
  width: 12px;
  height: 14px;
  border: 1px solid var(--ink);
  background: linear-gradient(90deg, var(--c1) 0 50%, var(--c2) 50% 100%);
}

/* ---- Halftone bars ---- */

.bar {
  display: block;
  position: relative;
  min-width: 24px;
  height: 10px;
  border: 1px solid var(--rule-soft);
  background: var(--paper-2);
}

.bar-fill {
  display: block;
  height: 100%;
  background-color: color-mix(in srgb, var(--ink) 16%, transparent);
  background-image: radial-gradient(circle at center, var(--ink) 1.1px, transparent 1.4px);
  background-size: 4px 4px;
}

.bar-red .bar-fill {
  background-color: color-mix(in srgb, var(--red) 18%, transparent);
  background-image: radial-gradient(circle at center, var(--red) 1.1px, transparent 1.4px);
}

/* ---- Printed tables ---- */

.table-frame {
  overflow-x: auto;
  -webkit-overflow-scrolling: touch;
  border-top: 2px solid var(--rule);
  border-bottom: 2px solid var(--rule);
}

table.print {
  width: 100%;
  border-collapse: collapse;
  font-family: var(--narrow);
  font-size: 15px;
  font-variant-numeric: tabular-nums lining-nums;
}

.print th {
  padding: 8px 8px 6px;
  border-bottom: 1px solid var(--rule);
  font-size: 12px;
  font-weight: 700;
  letter-spacing: 0.06em;
  text-align: right;
  text-transform: uppercase;
  white-space: nowrap;
}

.print td {
  padding: 6px 8px;
  border-bottom: 1px solid var(--rule-soft);
  text-align: right;
  white-space: nowrap;
}

.print .l {
  text-align: left;
}

.print tbody tr:last-child td {
  border-bottom: 0;
}

.print .rank {
  width: 2.4em;
  text-align: center;
  font-family: var(--display);
  font-weight: 800;
  font-size: 18px;
}

.row-link {
  display: inline-flex;
  max-width: 100%;
  padding: 0;
  border: 0;
  background: none;
  font-weight: 600;
  text-align: left;
  cursor: pointer;
}

.row-link:hover .club-name {
  text-decoration: underline;
  text-decoration-color: var(--red);
  text-underline-offset: 3px;
}

/* ---- Fact rows (dl) ---- */

.facts {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(118px, 1fr));
  margin: 0 0 16px;
  border-top: 1px solid var(--rule);
}

.facts > div {
  padding: 8px 10px 8px 0;
  border-bottom: 1px solid var(--rule-soft);
}

.facts dt {
  font-family: var(--narrow);
  font-size: 11.5px;
  font-weight: 700;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: var(--ink-3);
}

.facts dd {
  margin: 2px 0 0;
  font-family: var(--display);
  font-weight: 800;
  font-size: 26px;
  line-height: 1;
  font-variant-numeric: tabular-nums;
}

/* ---- Buttons ---- */

.btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 8px 14px;
  border: 1.5px solid var(--ink);
  border-radius: 0;
  background: transparent;
  color: var(--ink);
  font-family: var(--narrow);
  font-weight: 700;
  font-size: 13px;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  cursor: pointer;
}

.btn:hover:not(:disabled) {
  background: var(--ink);
  color: var(--paper);
}

.btn-red {
  border-color: var(--red);
  color: var(--red-ink);
}

.btn-red:hover:not(:disabled) {
  border-color: var(--red);
  background: var(--red);
  color: var(--paper);
}

.btn:disabled {
  opacity: 0.5;
  cursor: default;
}

/* ---- States and legends ---- */

.erratum {
  margin: 16px 0;
  padding: 10px 12px;
  border: 2px solid var(--red);
  color: var(--red-ink);
  font-family: var(--narrow);
  font-size: 14px;
  overflow-wrap: anywhere;
}

.in-press {
  padding: 72px 0;
  text-align: center;
  font-family: var(--display);
  font-weight: 900;
  font-size: 34px;
  text-transform: uppercase;
}

.in-press small {
  display: block;
  margin-top: 8px;
  font-family: var(--serif);
  font-weight: 400;
  font-style: italic;
  font-size: 17px;
  text-transform: none;
  color: var(--ink-2);
}

.legend {
  display: flex;
  flex-wrap: wrap;
  gap: 6px 18px;
  margin: 10px 0 0;
  padding: 0;
  list-style: none;
  font-family: var(--narrow);
  font-size: 13px;
  color: var(--ink-2);
}

.legend li {
  display: inline-flex;
  align-items: center;
  gap: 6px;
}

.key {
  display: inline-block;
  width: 22px;
  height: 0;
  border-top: 1px solid var(--rule);
}

.key-bold {
  border-top-width: 3px;
}

.key-red {
  border-top: 2px dashed var(--red);
}
```

- [ ] **Step 8: Run CI's unit tests on Node 22**

In `.github/workflows/ci.yml`, frontend job: change `node-version: 20.19.0` to `node-version: 22.12.0`, and after the `Build` step add:

```yaml
      - name: Unit tests
        run: npm test
```

- [ ] **Step 9: Verify**

Run: `npm test && npx tsc --noEmit && npm run build`
Expected: tests pass; no type errors; `✓ built`.

- [ ] **Step 10: Commit**

```bash
cd .. && git add frontend/src/clubs.ts frontend/src/clubs.test.mjs frontend/src/main.tsx frontend/src/i18n.tsx frontend/src/styles.css frontend/package.json frontend/package-lock.json .github/workflows/ci.yml
git commit -m "ui: programme foundation — typesetting module, fonts, copy, base styles"
```

---

### Task 2: Print primitives, report card, cover page

**Files:**
- Create: `frontend/src/components/Print.tsx`, `frontend/src/pages/CoverPage.tsx`
- Replace: `frontend/src/components/AccuracyPanel.tsx`
- Modify: `frontend/src/styles.css` (append)

**Interfaces:**
- Consumes: `clubColours`, `pct`, `num`, `signed`, `shortDate`, `coverHeadline` from `../clubs`; `useT`, `useLang` from `../i18n`; API types.
- Produces: `Swatch({ team })`, `ClubName({ team, flip? })`, `Bar({ value, max?, tone? })`, `PageHead({ title, page })` from `components/Print.tsx`; `AccuracyPanel({ report })` (same signature as today); `CoverPage({ data, matches, accuracy, onOpenFixtures, onSelectClub })` where `matches: MatchesResponse | null`, `accuracy: AccuracyReport | null`, `onOpenFixtures: () => void`, `onSelectClub: (team: string) => void`.

- [ ] **Step 1: Create `src/components/Print.tsx`**

```tsx
import type { CSSProperties } from "react";
import { clubColours } from "../clubs";
import { useT } from "../i18n";

/** Two-stripe scarf swatch in a club's colours. Decorative: the name next
 *  to it carries the meaning. */
export function Swatch({ team }: { team: string }) {
  const { c1, c2 } = clubColours(team);
  return (
    <span
      className="swatch"
      aria-hidden="true"
      style={{ "--c1": c1, "--c2": c2 } as CSSProperties}
    />
  );
}

/** A club name with its swatch. Marked Turkish so uppercase styling gives
 *  "İ" and "I" correctly in the English edition too. `flip` puts the
 *  swatch after the name (home sides in a result line). */
export function ClubName({ team, flip = false }: { team: string; flip?: boolean }) {
  return (
    <span className={flip ? "club club-flip" : "club"} lang="tr">
      <Swatch team={team} />
      <span className="club-name">{team}</span>
    </span>
  );
}

/** Halftone probability bar; `max` is the value that fills it. */
export function Bar({
  value,
  max = 100,
  tone = "ink",
}: {
  value: number;
  max?: number;
  tone?: "ink" | "red";
}) {
  const width = max > 0 ? Math.max(0, Math.min(100, (value / max) * 100)) : 0;
  return (
    <span className={`bar bar-${tone}`} aria-hidden="true">
      <span className="bar-fill" style={{ width: `${width}%` }} />
    </span>
  );
}

/** A programme page heading: the title, and the page number in red. */
export function PageHead({ title, page }: { title: string; page: number }) {
  const t = useT();
  return (
    <header className="page-head">
      <h2 className="page-title">{title}</h2>
      <span className="page-num">
        {t("pageAbbr")} {page}
      </span>
    </header>
  );
}
```

- [ ] **Step 2: Replace `src/components/AccuracyPanel.tsx`**

```tsx
import type { AccuracyReport } from "../api";
import { useLang, useT } from "../i18n";
import { num, pct, signed } from "../clubs";
import { Bar } from "./Print";

/** "Tahmin karnesi": how the model did against predictions frozen before
 *  kick-off. An empty log is the honest state early in a season. */
export function AccuracyPanel({ report }: { report: AccuracyReport }) {
  const t = useT();
  const { lang } = useLang();
  const band = (from: number, to: number) =>
    lang === "tr" ? `%${from}–${to}` : `${from}–${to}%`;

  return (
    <section className="boxed" aria-label={t("reportCard")}>
      <h3 className="section-title">{t("reportCard")}</h3>
      {report.scored === 0 ? (
        <p className="note">{t("accuracyEmpty")}</p>
      ) : (
        <>
          <dl className="facts">
            <div>
              <dt>{t("scoredMatches")}</dt>
              <dd>{num(report.scored, lang)}</dd>
            </div>
            <div>
              <dt>{t("calledCorrectly")}</dt>
              <dd>{pct(report.hit_rate_pct, lang, 0, false)}</dd>
            </div>
            <div>
              <dt>{t("logLoss")}</dt>
              <dd>{num(report.log_loss, lang, 3)}</dd>
            </div>
            <div>
              <dt>{t("vsBaseRates")}</dt>
              <dd>{signed(report.log_loss - report.baseline_log_loss, lang, 3)}</dd>
            </div>
          </dl>
          <p className="small-print">{t("logLossNote")}</p>
          {report.calibration.length > 0 && (
            <ul className="calib" aria-label={t("calibration")}>
              {report.calibration.map((b) => (
                <li key={b.band_from_pct}>
                  <span>{band(b.band_from_pct, b.band_to_pct)}</span>
                  <span className="calib-bars">
                    <Bar value={b.mean_predicted_pct} />
                    <Bar value={b.actual_pct} tone="red" />
                  </span>
                  <span className="calib-val">
                    {t("said")} {pct(b.mean_predicted_pct, lang, 0, false)} · {t("happened")}{" "}
                    {pct(b.actual_pct, lang, 0, false)}
                  </span>
                </li>
              ))}
            </ul>
          )}
        </>
      )}
    </section>
  );
}
```

- [ ] **Step 3: Create `src/pages/CoverPage.tsx`**

```tsx
import type { AccuracyReport, MatchesResponse, SimResponse } from "../api";
import { useLang, useT } from "../i18n";
import { coverHeadline, num, pct, shortDate } from "../clubs";
import { AccuracyPanel } from "../components/AccuracyPanel";
import { Bar, ClubName, PageHead } from "../components/Print";

/** Page 1: the lead line, both races at a glance, this week's fixtures
 *  and the forecast report card. */
export function CoverPage({
  data,
  matches,
  accuracy,
  onOpenFixtures,
  onSelectClub,
}: {
  data: SimResponse;
  matches: MatchesResponse | null;
  accuracy: AccuracyReport | null;
  onOpenFixtures: () => void;
  onSelectClub: (team: string) => void;
}) {
  const t = useT();
  const { lang } = useLang();
  const favourite = data.teams[0];
  const contenders = data.teams.filter((x) => x.title_pct > 0).slice(0, 6);
  const titleMax = contenders[0]?.title_pct ?? 1;
  const strugglers = [...data.teams]
    .filter((x) => x.relegation_pct > 0)
    .sort((a, b) => b.relegation_pct - a.relegation_pct)
    .slice(0, 6);
  const relegationMax = strugglers[0]?.relegation_pct ?? 1;
  const round = matches?.rounds.find((r) => r.round === matches.current_round);
  const upcoming = round?.matches.filter((m) => !m.played && m.forecast) ?? [];

  return (
    <article className="page">
      <PageHead title={t("pageCover")} page={1} />
      <div className="spread">
        <section>
          {favourite && (
            <h2 className="cover-headline">
              {coverHeadline(favourite.team, favourite.title_pct, lang)}
            </h2>
          )}
          <p className="standfirst">
            {num(data.n_sims, lang)} {t("coverStandfirst")}
          </p>
          <h3 className="section-title">{t("titleRace")}</h3>
          <ol className="race-list">
            {contenders.map((x, i) => (
              <li key={x.team}>
                <button type="button" className="race-item" onClick={() => onSelectClub(x.team)}>
                  <span className="rank">{i + 1}</span>
                  <ClubName team={x.team} />
                  <Bar value={x.title_pct} max={titleMax} />
                  <span className="val">{pct(x.title_pct, lang)}</span>
                </button>
              </li>
            ))}
          </ol>
          <p className="drop-line">{t("dropLine")}</p>
          <ol className="race-list">
            {strugglers.map((x) => (
              <li key={x.team}>
                <button type="button" className="race-item" onClick={() => onSelectClub(x.team)}>
                  <span className="rank" aria-hidden="true" />
                  <ClubName team={x.team} />
                  <Bar value={x.relegation_pct} max={relegationMax} tone="red" />
                  <span className="val">{pct(x.relegation_pct, lang)}</span>
                </button>
              </li>
            ))}
          </ol>
        </section>
        <aside>
          <h3 className="section-title">{t("thisWeek")}</h3>
          {upcoming.length === 0 ? (
            <p className="note">{matches ? t("noFixturesLeft") : t("calendarLoading")}</p>
          ) : (
            <ul className="teasers">
              {upcoming.map((m) => (
                <li key={`${m.home}-${m.away}`} className="teaser">
                  <span className="teaser-when">
                    {shortDate(m.date, lang)}
                    {m.kickoff ? ` · ${m.kickoff}` : ""}
                  </span>
                  <span className="teaser-teams">
                    <ClubName team={m.home} />
                    <span className="vs">–</span>
                    <ClubName team={m.away} />
                  </span>
                  {m.forecast && (
                    <span className="teaser-chances">
                      {pct(m.forecast.home_win_pct, lang, 0)} · {pct(m.forecast.draw_pct, lang, 0)} ·{" "}
                      {pct(m.forecast.away_win_pct, lang, 0)}
                    </span>
                  )}
                </li>
              ))}
            </ul>
          )}
          <button type="button" className="btn" onClick={onOpenFixtures}>
            {t("allFixtures")} · {t("pageAbbr")} 2
          </button>
          {accuracy && <AccuracyPanel report={accuracy} />}
        </aside>
      </div>
    </article>
  );
}
```

- [ ] **Step 4: Append the cover styles to `src/styles.css`**

```css
/* ---- Kapak (cover) ---- */

.cover-headline {
  margin: 0 0 8px;
  font-family: var(--serif);
  font-weight: 600;
  font-size: clamp(30px, 5.6vw, 54px);
  line-height: 1.04;
  letter-spacing: -0.01em;
  text-wrap: balance;
}

.standfirst {
  margin: 0 0 24px;
  font-style: italic;
  font-size: 18px;
  color: var(--ink-2);
}

.race-list {
  margin: 0 0 4px;
  padding: 0;
  list-style: none;
}

.race-item,
.race-row {
  display: grid;
  grid-template-columns: 1.6em minmax(0, 10em) minmax(0, 1fr) 4.6em;
  align-items: center;
  gap: 10px;
  width: 100%;
  padding: 7px 0;
  border: 0;
  border-bottom: 1px solid var(--rule-soft);
  background: none;
  font-family: var(--narrow);
  font-size: 16px;
  font-weight: 600;
  text-align: left;
}

button.race-item {
  cursor: pointer;
}

button.race-item:hover .club-name {
  text-decoration: underline;
  text-decoration-color: var(--red);
  text-underline-offset: 3px;
}

.race-item .rank {
  font-family: var(--display);
  font-weight: 800;
  font-size: 20px;
  text-align: center;
  color: var(--ink-2);
}

.drop-line {
  display: flex;
  align-items: center;
  gap: 10px;
  margin: 20px 0 4px;
  font-family: var(--narrow);
  font-weight: 700;
  font-size: 12px;
  letter-spacing: 0.16em;
  text-transform: uppercase;
  color: var(--red-ink);
}

.drop-line::before,
.drop-line::after {
  content: "";
  flex: 1;
  border-top: 2px dashed var(--red);
}

.teasers {
  margin: 0 0 14px;
  padding: 0;
  list-style: none;
}

.teaser {
  display: grid;
  gap: 2px;
  padding: 9px 0;
  border-bottom: 1px solid var(--rule-soft);
  font-family: var(--narrow);
}

.teaser-when {
  font-size: 12px;
  font-weight: 700;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  color: var(--ink-3);
}

.teaser-teams {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 2px 8px;
  font-weight: 600;
  font-size: 16px;
}

.vs {
  color: var(--ink-3);
}

.teaser-chances {
  font-size: 14px;
  color: var(--ink-2);
  font-variant-numeric: tabular-nums;
}

.boxed {
  margin-top: 24px;
  padding: 14px 16px 16px;
  border: 1.5px solid var(--rule);
}

.calib {
  margin: 10px 0 0;
  padding: 0;
  list-style: none;
}

.calib li {
  display: grid;
  grid-template-columns: 5.2em minmax(0, 1fr);
  align-items: center;
  gap: 4px 10px;
  padding: 6px 0;
  border-bottom: 1px solid var(--rule-soft);
  font-family: var(--narrow);
  font-size: 13px;
}

.calib-bars {
  display: grid;
  gap: 3px;
}

.calib-val {
  grid-column: 2;
  color: var(--ink-2);
}
```

- [ ] **Step 5: Verify**

Run: `npx tsc --noEmit && npm run build`
Expected: no type errors; `✓ built`. (The old dashboard still mounts `AccuracyPanel` with the same props, so it keeps compiling.)

- [ ] **Step 6: Commit**

```bash
cd .. && git add frontend/src/components/Print.tsx frontend/src/components/AccuracyPanel.tsx frontend/src/pages/CoverPage.tsx frontend/src/styles.css
git commit -m "ui: print primitives, report card and the cover page"
```

---

### Task 3: "Bu Hafta" — ticket stubs, stamps, played column

**Files:**
- Create: `frontend/src/pages/FixturesPage.tsx`
- Modify: `frontend/src/components/Print.tsx` (add `ResultLine`), `frontend/src/styles.css` (append)

**Interfaces:**
- Consumes: `ClubName`, `PageHead` from Task 2; `clubColours`, `dateSpan`, `num`, `pct`, `shortDate`.
- Produces: `ResultLine({ home, away, homeScore, awayScore })` (Print.tsx; renders an `<li>`); `FixturesPage({ data, whatIf, onWhatIf, onReprint, busy })` with `data: MatchesResponse`, `whatIf: WhatIf[]`, `onWhatIf: (next: WhatIf[]) => void`, `onReprint: () => void`, `busy: boolean`. CSS: `.results-list`, `.result-line`, `.score` (reused by Task 6).

- [ ] **Step 1: Add `ResultLine` to `src/components/Print.tsx`** (append)

```tsx
/** One played match, home name right-aligned against the score. */
export function ResultLine({
  home,
  away,
  homeScore,
  awayScore,
}: {
  home: string;
  away: string;
  homeScore: number;
  awayScore: number;
}) {
  return (
    <li className="result-line">
      <ClubName team={home} flip />
      <span className="score">
        {homeScore}–{awayScore}
      </span>
      <ClubName team={away} />
    </li>
  );
}
```

- [ ] **Step 2: Create `src/pages/FixturesPage.tsx`**

```tsx
import { useEffect, useState, type CSSProperties } from "react";
import type { MatchCard, MatchForecast, MatchesResponse, WhatIf } from "../api";
import { useLang, useT } from "../i18n";
import { clubColours, dateSpan, num, pct, shortDate } from "../clubs";
import { ClubName, PageHead, ResultLine } from "../components/Print";

type Outcome = "home" | "draw" | "away";

/** Home, draw and away as one bar: each side in its own colours, the draw
 *  as a neutral halftone. */
function ChanceBar({ m, f }: { m: MatchCard; f: MatchForecast }) {
  const home = clubColours(m.home);
  const away = clubColours(m.away);
  return (
    <div className="probbar" aria-hidden="true">
      <span
        className="seg seg-team"
        style={{ width: `${f.home_win_pct}%`, "--c1": home.c1, "--c2": home.c2 } as CSSProperties}
      />
      <span className="seg seg-draw" style={{ width: `${f.draw_pct}%` }} />
      <span
        className="seg seg-team"
        style={{ width: `${f.away_win_pct}%`, "--c1": away.c1, "--c2": away.c2 } as CSSProperties}
      />
    </div>
  );
}

/** The tear-off end of a stub: stamp a result to assume it. */
function Stamps({
  m,
  pinned,
  onPin,
}: {
  m: MatchCard;
  pinned: Outcome | null;
  onPin: (o: Outcome | null) => void;
}) {
  const t = useT();
  const options: [Outcome, string, string][] = [
    ["home", t("stampHome"), t("pinHome")],
    ["draw", t("stampDraw"), t("pinDraw")],
    ["away", t("stampAway"), t("pinAway")],
  ];
  return (
    <div className="stub-tear" role="group" aria-label={`${t("pinsLabel")}: ${m.home} – ${m.away}`}>
      <span className="pins-label">{t("pinsLabel")}</span>
      {options.map(([value, glyph, label]) => (
        <button
          key={value}
          type="button"
          className="stamp"
          aria-pressed={pinned === value}
          aria-label={label}
          title={label}
          onClick={() => onPin(pinned === value ? null : value)}
        >
          {glyph}
        </button>
      ))}
    </div>
  );
}

function Stub({
  m,
  f,
  no,
  pinned,
  onPin,
}: {
  m: MatchCard;
  f: MatchForecast;
  no: number;
  pinned: Outcome | null;
  onPin: (o: Outcome | null) => void;
}) {
  const t = useT();
  const { lang } = useLang();
  return (
    <article className="stub">
      <div className="stub-main">
        <p className="stub-meta">
          <span className="stub-no">№ {no}</span>
          <span>
            {shortDate(m.date, lang)}
            {m.kickoff ? ` · ${m.kickoff}` : ""}
          </span>
        </p>
        <h3 className="stub-teams">
          <ClubName team={m.home} />
          <span className="stub-v">–</span>
          <ClubName team={m.away} />
        </h3>
        <ChanceBar m={m} f={f} />
        <dl className="outcomes">
          <div>
            <dt>{t("outHome")}</dt>
            <dd>{pct(f.home_win_pct, lang)}</dd>
          </div>
          <div>
            <dt>{t("outDraw")}</dt>
            <dd>{pct(f.draw_pct, lang)}</dd>
          </div>
          <div>
            <dt>{t("outAway")}</dt>
            <dd>{pct(f.away_win_pct, lang)}</dd>
          </div>
        </dl>
        <p className="small-print">
          xG {num(f.home_xg, lang, 2)} – {num(f.away_xg, lang, 2)} · {t("over25Short")}{" "}
          {pct(f.over25_pct, lang, 0)} · {t("bttsShort")} {pct(f.btts_pct, lang, 0)}
        </p>
        <p className="small-print">
          {t("likely")}:{" "}
          {f.likely_scores.map((s) => `${s.home}–${s.away} ${pct(s.pct, lang, 0)}`).join(" · ")}
        </p>
      </div>
      <Stamps m={m} pinned={pinned} onPin={onPin} />
    </article>
  );
}

/** Page 2: one matchday as an issue — ticket stubs for the fixtures still
 *  to play, a results column for those already played. */
export function FixturesPage({
  data,
  whatIf,
  onWhatIf,
  onReprint,
  busy,
}: {
  data: MatchesResponse;
  whatIf: WhatIf[];
  onWhatIf: (next: WhatIf[]) => void;
  onReprint: () => void;
  busy: boolean;
}) {
  const t = useT();
  const { lang } = useLang();
  const [round, setRound] = useState(data.current_round);
  // A live refresh can move the current matchday on; follow it.
  useEffect(() => setRound(data.current_round), [data.current_round]);

  const first = data.rounds[0]?.round ?? 1;
  const last = data.rounds[data.rounds.length - 1]?.round ?? first;
  const current = data.rounds.find((r) => r.round === round) ?? data.rounds[0];
  if (!current) return null;

  const toPlay = current.matches.filter((m) => !m.played && m.forecast);
  const played = current.matches.filter(
    (m) => m.played && m.home_score != null && m.away_score != null,
  );
  const pinOf = (m: MatchCard) =>
    (whatIf.find((w) => w.home === m.home && w.away === m.away)?.outcome ?? null) as Outcome | null;
  const setPin = (m: MatchCard, o: Outcome | null) => {
    const rest = whatIf.filter((w) => !(w.home === m.home && w.away === m.away));
    onWhatIf(o ? [...rest, { home: m.home, away: m.away, outcome: o }] : rest);
  };
  const fixtureNo = (m: MatchCard) =>
    (current.round - 1) * current.matches.length + current.matches.indexOf(m) + 1;
  const sign = (o: string) => (o === "home" ? ">" : o === "away" ? "<" : "=");

  return (
    <article className="page">
      <PageHead title={t("pageFixtures")} page={2} />
      <nav className="issue-nav" aria-label={t("matchday")}>
        <button
          type="button"
          className="btn"
          onClick={() => setRound(Math.max(first, current.round - 1))}
          disabled={current.round <= first}
          aria-label={t("prevIssue")}
        >
          ‹
        </button>
        <label>
          <span className="sr-only">{t("matchday")}</span>
          <select value={current.round} onChange={(e) => setRound(Number(e.target.value))}>
            {data.rounds.map((r) => (
              <option key={r.round} value={r.round}>
                {t("issue")} {r.round}
              </option>
            ))}
          </select>
        </label>
        <button
          type="button"
          className="btn"
          onClick={() => setRound(Math.min(last, current.round + 1))}
          disabled={current.round >= last}
          aria-label={t("nextIssue")}
        >
          ›
        </button>
        <span className="issue-dates">{dateSpan(current.matches.map((m) => m.date), lang)}</span>
      </nav>
      <p className="note">{t("fixturesNote")}</p>
      {whatIf.length > 0 && (
        <div className="assume-slip" role="status">
          <span className="assume-label">{t("assuming")}</span>
          <span className="assume-list" lang="tr">
            {whatIf.map((w) => `${w.home} ${sign(w.outcome)} ${w.away}`).join(" · ")}
          </span>
          <button type="button" className="btn btn-red" onClick={onReprint} disabled={busy}>
            {busy ? t("printing") : t("reprintWithPins")}
          </button>
          <button type="button" className="btn" onClick={() => onWhatIf([])}>
            {t("clearPins")}
          </button>
        </div>
      )}
      {toPlay.length > 0 && (
        <div className="stubs">
          {toPlay.map((m) => (
            <Stub
              key={`${m.home}-${m.away}`}
              m={m}
              f={m.forecast!}
              no={fixtureNo(m)}
              pinned={pinOf(m)}
              onPin={(o) => setPin(m, o)}
            />
          ))}
        </div>
      )}
      {played.length > 0 && (
        <section className="played-block">
          <h3 className="section-title">{t("playedThisWeek")}</h3>
          <ul className="results-list">
            {played.map((m) => (
              <ResultLine
                key={`${m.home}-${m.away}`}
                home={m.home}
                away={m.away}
                homeScore={m.home_score!}
                awayScore={m.away_score!}
              />
            ))}
          </ul>
        </section>
      )}
    </article>
  );
}
```

- [ ] **Step 3: Append the fixtures styles to `src/styles.css`**

```css
/* ---- Bu Hafta (this week) ---- */

.issue-nav {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px 10px;
  margin-bottom: 8px;
}

.issue-nav select {
  padding: 7px 10px;
  border: 1.5px solid var(--ink);
  border-radius: 0;
  background: var(--paper);
  font-family: var(--narrow);
  font-weight: 700;
  font-size: 14px;
  letter-spacing: 0.06em;
  text-transform: uppercase;
}

.issue-dates {
  font-family: var(--narrow);
  font-size: 14px;
  color: var(--ink-2);
}

.assume-slip {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px 14px;
  margin: 0 0 16px;
  padding: 10px 12px;
  border: 1.5px dashed var(--red);
  font-family: var(--narrow);
  font-size: 14px;
}

.assume-label {
  font-weight: 700;
  letter-spacing: 0.1em;
  text-transform: uppercase;
  color: var(--red-ink);
}

.assume-list {
  flex: 1 1 14em;
  min-width: 0;
}

.stubs {
  display: grid;
  gap: 16px;
}

@media (min-width: 760px) {
  .stubs {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
}

@media (min-width: 1140px) {
  .stubs {
    grid-template-columns: repeat(3, minmax(0, 1fr));
  }
}

.stub {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  overflow: hidden;
  border: 1.5px solid var(--rule);
  background: var(--paper);
}

.stub-main {
  min-width: 0;
  padding: 12px 14px;
}

.stub-meta {
  display: flex;
  flex-wrap: wrap;
  gap: 4px 10px;
  margin: 0 0 6px;
  font-family: var(--narrow);
  font-size: 12px;
  font-weight: 700;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  color: var(--ink-3);
}

.stub-no {
  color: var(--red-ink);
}

.stub-teams {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 2px 8px;
  margin: 0 0 10px;
  font-family: var(--display);
  font-weight: 800;
  font-size: 21px;
  line-height: 1.1;
  text-transform: uppercase;
}

.stub-v {
  color: var(--ink-3);
}

.probbar {
  display: flex;
  height: 16px;
  margin: 0 0 6px;
  border: 1.5px solid var(--rule);
}

.seg {
  display: block;
  min-width: 3px;
  height: 100%;
}

.seg-team {
  background: linear-gradient(var(--c1) 0 66%, var(--c2) 66% 100%);
}

.seg-draw {
  border-right: 1.5px solid var(--rule);
  border-left: 1.5px solid var(--rule);
  background-color: var(--paper-2);
  background-image: radial-gradient(circle at center, var(--ink-3) 0.9px, transparent 1.2px);
  background-size: 4px 4px;
}

.outcomes {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  margin: 0 0 8px;
  font-family: var(--narrow);
}

.outcomes > div:nth-child(2) {
  text-align: center;
}

.outcomes > div:nth-child(3) {
  text-align: right;
}

.outcomes dt {
  font-size: 11px;
  font-weight: 700;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: var(--ink-3);
}

.outcomes dd {
  margin: 0;
  font-weight: 700;
  font-size: 18px;
  font-variant-numeric: tabular-nums;
}

/* Perforated tear-off with half-moon notches top and bottom. */
.stub-tear {
  position: relative;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 6px;
  padding: 12px 10px;
  border-left: 2px dashed var(--rule-soft);
  background: var(--paper-2);
}

.stub-tear::before,
.stub-tear::after {
  content: "";
  position: absolute;
  left: -9px;
  width: 16px;
  height: 16px;
  border: 1.5px solid var(--rule);
  border-radius: 50%;
  background: var(--paper);
}

.stub-tear::before {
  top: -9px;
}

.stub-tear::after {
  bottom: -9px;
}

.pins-label {
  font-family: var(--narrow);
  font-size: 10px;
  font-weight: 700;
  letter-spacing: 0.12em;
  text-transform: uppercase;
  color: var(--ink-2);
}

.stamp {
  width: 40px;
  height: 34px;
  padding: 0;
  border: 1.5px solid var(--ink);
  background: transparent;
  font-family: var(--display);
  font-weight: 900;
  font-size: 19px;
  line-height: 1;
  cursor: pointer;
}

.stamp:hover {
  background: var(--paper);
}

.stamp[aria-pressed="true"] {
  border: 2px solid var(--red);
  background: color-mix(in srgb, var(--red) 12%, transparent);
  color: var(--red-ink);
  transform: rotate(-5deg);
}

.played-block {
  margin-top: 28px;
}

.results-list {
  margin: 0;
  padding: 0;
  list-style: none;
}

.result-line {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr);
  align-items: center;
  gap: 10px;
  padding: 6px 0;
  border-bottom: 1px solid var(--rule-soft);
  font-family: var(--narrow);
  font-size: 15px;
  font-weight: 600;
}

.result-line > .club:first-child {
  justify-self: end;
}

.score {
  min-width: 3.4em;
  padding: 1px 6px;
  border: 1px solid var(--rule);
  text-align: center;
  font-family: var(--display);
  font-weight: 800;
  font-size: 18px;
  font-variant-numeric: tabular-nums;
}
```

- [ ] **Step 4: Verify**

Run: `npx tsc --noEmit && npm run build`
Expected: no type errors; `✓ built`.

- [ ] **Step 5: Commit**

```bash
cd .. && git add frontend/src/pages/FixturesPage.tsx frontend/src/components/Print.tsx frontend/src/styles.css
git commit -m "ui: this week's fixtures as ticket stubs with what-if stamps"
```

---

### Task 4: "Puan Durumu" and the club page

**Files:**
- Create: `frontend/src/pages/TablePage.tsx`
- Replace: `frontend/src/components/ClubDetail.tsx`
- Modify: `frontend/src/styles.css` (append)

**Interfaces:**
- Consumes: `ClubName`, `Swatch`, `PageHead`; `num`, `pct`, `shortDate`, `signed`.
- Produces: `TablePage({ table, onSelectClub })` with `table: TableRow[]`; `ClubDetail({ team, position, matches, onClose })` — same props as today (`team: TeamRow`, `position: PositionRow | undefined`, `matches: MatchesResponse | null`, `onClose: () => void`).

- [ ] **Step 1: Create `src/pages/TablePage.tsx`**

```tsx
import type { TableRow } from "../api";
import { useLang, useT } from "../i18n";
import { signed } from "../clubs";
import { ClubName, PageHead } from "../components/Print";

/** Page 3: the projected final table, set like a printed league table —
 *  thin rules under 2nd and 4th, a red rule above the bottom three. */
export function TablePage({
  table,
  onSelectClub,
}: {
  table: TableRow[];
  onSelectClub: (team: string) => void;
}) {
  const t = useT();
  const { lang } = useLang();
  const n = table.length;
  const rowClass = (pos: number) =>
    [pos === 2 || pos === 4 ? "rule-after" : "", pos === n - 2 ? "rule-red" : ""]
      .filter(Boolean)
      .join(" ");

  return (
    <article className="page">
      <PageHead title={t("pageTable")} page={3} />
      <p className="note">
        {t("projectedNote")} {t("tapForClub")}
      </p>
      <div className="table-frame">
        <table className="print league">
          <caption className="sr-only">{t("projectedTable")}</caption>
          <thead>
            <tr>
              <th scope="col">{t("colPos")}</th>
              <th scope="col" className="l">
                {t("club")}
              </th>
              <th scope="col">{t("colP")}</th>
              <th scope="col">{t("colW")}</th>
              <th scope="col">{t("colD")}</th>
              <th scope="col">{t("colL")}</th>
              <th scope="col">{t("colF")}</th>
              <th scope="col">{t("colA")}</th>
              <th scope="col">{t("colGD")}</th>
              <th scope="col">{t("colPts")}</th>
            </tr>
          </thead>
          <tbody>
            {table.map((r) => (
              <tr key={r.team} className={rowClass(r.position)}>
                <td className="rank">{r.position}</td>
                <td className="l">
                  <button type="button" className="row-link" onClick={() => onSelectClub(r.team)}>
                    <ClubName team={r.team} />
                  </button>
                </td>
                <td>{r.played}</td>
                <td>{r.won}</td>
                <td>{r.drawn}</td>
                <td>{r.lost}</td>
                <td>{r.gf}</td>
                <td>{r.ga}</td>
                <td>{signed(r.gd, lang, 0)}</td>
                <td className="pts">{r.points}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <ul className="legend">
        <li>
          <span className="key" /> {t("topTwo")} · {t("topFour")}
        </li>
        <li>
          <span className="key key-red" /> {t("relegation")}
        </li>
      </ul>
    </article>
  );
}
```

- [ ] **Step 2: Replace `src/components/ClubDetail.tsx`**

```tsx
import { useEffect, useRef } from "react";
import type { MatchCard, MatchesResponse, PositionRow, TeamRow } from "../api";
import { useLang, useT } from "../i18n";
import { num, pct, shortDate } from "../clubs";
import { ClubName, Swatch } from "./Print";

/** The club's own view of a fixture: who it plays, where, its chances. */
type ClubFixture = {
  m: MatchCard;
  round: number;
  opponent: string;
  atHome: boolean;
  winPct: number | null;
  drawPct: number | null;
  result: "W" | "D" | "L" | null;
};

function clubFixtures(club: string, data: MatchesResponse): ClubFixture[] {
  const out: ClubFixture[] = [];
  for (const r of data.rounds) {
    for (const m of r.matches) {
      if (m.home !== club && m.away !== club) continue;
      const atHome = m.home === club;
      const f = m.forecast;
      let result: ClubFixture["result"] = null;
      if (m.played && m.home_score != null && m.away_score != null) {
        const gf = atHome ? m.home_score : m.away_score;
        const ga = atHome ? m.away_score : m.home_score;
        result = gf > ga ? "W" : gf === ga ? "D" : "L";
      }
      out.push({
        m,
        round: r.round,
        opponent: atHome ? m.away : m.home,
        atHome,
        winPct: f ? (atHome ? f.home_win_pct : f.away_win_pct) : null,
        drawPct: f ? f.draw_pct : null,
        result,
      });
    }
  }
  return out;
}

/** A club's own programme page, laid over the current one. */
export function ClubDetail({
  team,
  position,
  matches,
  onClose,
}: {
  team: TeamRow;
  position: PositionRow | undefined;
  matches: MatchesResponse | null;
  onClose: () => void;
}) {
  const t = useT();
  const { lang } = useLang();
  const closeRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    closeRef.current?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    const overflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("keydown", onKey);
      document.body.style.overflow = overflow;
    };
  }, [onClose]);

  const fixtures = matches ? clubFixtures(team.team, matches) : [];
  const remaining = fixtures.filter((f) => !f.m.played && f.winPct != null);
  const playedSoFar = fixtures.filter((f) => f.m.played);
  // Run-in difficulty is the model's own opinion, averaged: the mean win
  // probability across what is left to play.
  const runIn = remaining.length
    ? remaining.reduce((s, f) => s + (f.winPct ?? 0), 0) / remaining.length
    : null;
  const hardest = remaining.reduce<ClubFixture | null>(
    (worst, f) => (worst === null || (f.winPct ?? 100) < (worst.winPct ?? 100) ? f : worst),
    null,
  );
  const easiest = remaining.reduce<ClubFixture | null>(
    (best, f) => (best === null || (f.winPct ?? 0) > (best.winPct ?? 0) ? f : best),
    null,
  );
  const total = position?.position_pct.length ?? 18;
  const peak = position ? Math.max(...position.position_pct, 0.001) : 1;
  const letter = { W: t("resW"), D: t("resD"), L: t("resL") };
  const where = (f: ClubFixture) => `${f.atHome ? "" : "@ "}${f.opponent}`;

  return (
    <div className="sheet-overlay" onClick={onClose} role="presentation">
      <div
        className="club-sheet"
        role="dialog"
        aria-modal="true"
        aria-labelledby="club-sheet-title"
        onClick={(e) => e.stopPropagation()}
      >
        <header className="club-sheet-head">
          <h2 id="club-sheet-title" className="club-sheet-title" lang="tr">
            <Swatch team={team.team} />
            {team.team}
          </h2>
          <button ref={closeRef} type="button" className="btn" onClick={onClose}>
            {t("close")}
          </button>
        </header>

        <dl className="facts">
          <div>
            <dt>{t("title")}</dt>
            <dd>{pct(team.title_pct, lang)}</dd>
          </div>
          <div>
            <dt>{t("topFour")}</dt>
            <dd>{pct(team.top_four_pct, lang)}</dd>
          </div>
          <div>
            <dt>{t("relegation")}</dt>
            <dd>{pct(team.relegation_pct, lang)}</dd>
          </div>
          <div>
            <dt>{t("expectedPoints")}</dt>
            <dd>{num(team.exp_points, lang, 1)}</dd>
          </div>
          <div>
            <dt>{t("averageFinish")}</dt>
            <dd>{num(team.mean_position, lang, 1)}</dd>
          </div>
        </dl>

        {position && (
          <section>
            <h3 className="section-title">{t("whereItFinishes")}</h3>
            <div className="posbars">
              {position.position_pct.map((p, i) => (
                <span
                  key={i}
                  className={i + 1 > total - 3 ? "posbar posbar-rel" : "posbar"}
                  title={`${i + 1}. · ${pct(p, lang)}`}
                >
                  <span className="posbar-fill" style={{ height: `${(p / peak) * 100}%` }} />
                  <span className="posbar-num">{i + 1}</span>
                </span>
              ))}
            </div>
          </section>
        )}

        {runIn != null && (
          <p className="note">
            {t("runIn")}: {remaining.length} {t("toPlayAvg")} <strong>{pct(runIn, lang, 0)}</strong>.
            {hardest && ` ${t("hardest")}: ${where(hardest)} (${pct(hardest.winPct ?? 0, lang, 0)}).`}
            {easiest && ` ${t("easiest")}: ${where(easiest)} (${pct(easiest.winPct ?? 0, lang, 0)}).`}
          </p>
        )}

        {playedSoFar.length > 0 && (
          <section>
            <h3 className="section-title">{t("played")}</h3>
            <ul className="form-list">
              {playedSoFar.map((f) => (
                <li key={f.round} className={`form form-${f.result}`}>
                  <span className="form-res">{f.result ? letter[f.result] : ""}</span>
                  <span lang="tr">{where(f)}</span>
                </li>
              ))}
            </ul>
          </section>
        )}

        {remaining.length > 0 && (
          <section>
            <h3 className="section-title">{t("remainingFixtures")}</h3>
            <div className="table-frame">
              <table className="print">
                <thead>
                  <tr>
                    <th scope="col">{t("md")}</th>
                    <th scope="col" className="l">
                      {t("date")}
                    </th>
                    <th scope="col" className="l">
                      {t("opponent")}
                    </th>
                    <th scope="col">{t("winCol")}</th>
                    <th scope="col">{t("drawCol")}</th>
                  </tr>
                </thead>
                <tbody>
                  {remaining.map((f) => (
                    <tr key={f.round}>
                      <td>{f.round}</td>
                      <td className="l">{shortDate(f.m.date, lang)}</td>
                      <td className="l">
                        {f.atHome ? "" : "@ "}
                        <ClubName team={f.opponent} />
                      </td>
                      <td>{pct(f.winPct ?? 0, lang)}</td>
                      <td>{pct(f.drawPct ?? 0, lang)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </section>
        )}
      </div>
    </div>
  );
}
```

- [ ] **Step 3: Append the table and club-page styles to `src/styles.css`**

```css
/* ---- Puan Durumu (table) ---- */

.league td.pts {
  font-family: var(--display);
  font-weight: 800;
  font-size: 19px;
}

.league .rule-after td {
  border-bottom: 1px solid var(--rule);
}

.league .rule-red td {
  border-top: 2px dashed var(--red);
}

/* ---- Kulüp sayfası (club page) ---- */

.sheet-overlay {
  position: fixed;
  inset: 0;
  z-index: 20;
  display: flex;
  align-items: flex-start;
  justify-content: center;
  padding: 24px var(--gutter);
  overflow-y: auto;
  background: color-mix(in srgb, var(--ink) 55%, transparent);
}

.club-sheet {
  width: 100%;
  max-width: 780px;
  margin: auto 0;
  padding: 18px 20px 22px;
  border: 2px solid var(--rule);
  background-color: var(--paper);
  background-image: var(--grain);
}

.club-sheet-head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 14px;
  padding-bottom: 8px;
  border-bottom: 3px double var(--rule);
}

.club-sheet-title {
  display: flex;
  align-items: center;
  gap: 10px;
  margin: 0;
  font-family: var(--display);
  font-weight: 900;
  font-size: clamp(32px, 7vw, 52px);
  line-height: 0.95;
  text-transform: uppercase;
  overflow-wrap: anywhere;
}

.club-sheet-title .swatch {
  width: 22px;
  height: 30px;
}

.club-sheet section {
  margin-top: 18px;
}

.posbars {
  display: grid;
  grid-template-columns: repeat(18, minmax(0, 1fr));
  align-items: end;
  gap: 2px;
  height: 110px;
}

.posbar {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: flex-end;
  height: 100%;
}

.posbar-fill {
  display: block;
  width: 100%;
  min-height: 1px;
  border: 1px solid var(--rule-soft);
  background-color: color-mix(in srgb, var(--ink) 18%, transparent);
  background-image: radial-gradient(circle at center, var(--ink) 1.1px, transparent 1.4px);
  background-size: 4px 4px;
}

.posbar-rel .posbar-fill {
  background-color: color-mix(in srgb, var(--red) 18%, transparent);
  background-image: radial-gradient(circle at center, var(--red) 1.1px, transparent 1.4px);
}

.posbar-num {
  margin-top: 3px;
  font-family: var(--narrow);
  font-size: 11px;
  color: var(--ink-3);
}

.form-list {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin: 0;
  padding: 0;
  list-style: none;
  font-family: var(--narrow);
  font-size: 13px;
}

.form {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 2px 8px 2px 2px;
  border: 1px solid var(--rule-soft);
}

.form-res {
  display: inline-grid;
  place-items: center;
  width: 20px;
  height: 20px;
  background: var(--paper-2);
  font-family: var(--display);
  font-weight: 900;
  font-size: 14px;
}

.form-W .form-res {
  background: var(--ink);
  color: var(--paper);
}

.form-L .form-res {
  background: var(--red);
  color: var(--paper);
}
```

- [ ] **Step 4: Verify**

Run: `npx tsc --noEmit && npm run build`
Expected: no type errors; `✓ built`.

- [ ] **Step 5: Commit**

```bash
cd .. && git add frontend/src/pages/TablePage.tsx frontend/src/components/ClubDetail.tsx frontend/src/styles.css
git commit -m "ui: printed league table and the club page"
```

---

### Task 5: "Yarışlar" and the position grid

**Files:**
- Create: `frontend/src/pages/RacesPage.tsx`
- Replace: `frontend/src/components/PositionGrid.tsx`
- Modify: `frontend/src/styles.css` (append)

**Interfaces:**
- Consumes: `Bar`, `ClubName`, `PageHead`; `num`, `pct`, `signed`; `Key` from `../i18n`.
- Produces: `RacesPage({ data, onSelectClub })` with `data: SimResponse`; `PositionGrid({ positions })` (same props as today).

- [ ] **Step 1: Replace `src/components/PositionGrid.tsx`**

```tsx
import type { CSSProperties } from "react";
import type { PositionRow } from "../api";
import { useLang, useT } from "../i18n";
import { pct } from "../clubs";
import { ClubName } from "./Print";

/** Square-root ramp so small but real chances stay visible. */
const intensity = (p: number) => Math.min(1, Math.sqrt(p / 100));

/** The 18×18 finishing-position grid as a screen tint. The tint tops out
 *  at 46% ink, so the figures in every cell stay readable in both
 *  editions. */
export function PositionGrid({ positions }: { positions: PositionRow[] }) {
  const t = useT();
  const { lang } = useLang();
  const total = positions.length;
  const places = Array.from({ length: total }, (_, i) => i + 1);

  return (
    <section>
      <h3 className="section-title">{t("finishingPositions")}</h3>
      <p className="note">{t("finishingNote")}</p>
      <div className="table-frame">
        <table className="posgrid">
          <thead>
            <tr>
              <th scope="col">
                <span className="sr-only">{t("club")}</span>
              </th>
              {places.map((p) => (
                <th
                  scope="col"
                  key={p}
                  className={p <= 4 ? "top4" : p > total - 3 ? "rel" : undefined}
                >
                  {p}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {positions.map((row) => (
              <tr key={row.team}>
                <th scope="row">
                  <ClubName team={row.team} />
                </th>
                {row.position_pct.map((p, i) => (
                  <td
                    key={i}
                    style={{ "--i": intensity(p) } as CSSProperties}
                    title={`${row.team} · ${i + 1}. · ${pct(p, lang)}`}
                  >
                    {p >= 1 ? Math.round(p) : ""}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <ul className="legend">
        <li>
          <span className="key key-bold" /> {t("topFourPlaces")}
        </li>
        <li>
          <span className="key key-red" /> {t("relegationZone")}
        </li>
      </ul>
    </section>
  );
}
```

- [ ] **Step 2: Create `src/pages/RacesPage.tsx`**

```tsx
import type { RaceThreshold, RivalryPair, SimResponse, TeamRow } from "../api";
import { useLang, useT, type Key } from "../i18n";
import { num, pct, signed } from "../clubs";
import { Bar, ClubName, PageHead } from "../components/Print";
import { PositionGrid } from "../components/PositionGrid";

/** Screen tint for a probability cell: ink (red for relegation) at a
 *  strength that follows the value. */
const tint = (p: number, red = false) =>
  p > 0
    ? {
        background: `color-mix(in srgb, var(${red ? "--red" : "--ink"}) ${(4 + (p / 100) * 24).toFixed(1)}%, transparent)`,
      }
    : undefined;

type Race = { title: Key; note: Key; pick: (x: TeamRow) => number; tone: "ink" | "red" };

const RACES: Race[] = [
  { title: "title", note: "raceTitleNote", pick: (x) => x.title_pct, tone: "ink" },
  { title: "topTwo", note: "raceTopTwoNote", pick: (x) => x.top_two_pct, tone: "ink" },
  { title: "topFour", note: "raceTopFourNote", pick: (x) => x.top_four_pct, tone: "ink" },
  { title: "relegation", note: "raceRelNote", pick: (x) => x.relegation_pct, tone: "red" },
];

const PLACE: Record<string, Key> = {
  Champion: "champion",
  "Last top-two place": "lastTopTwoPlace",
  "Last top-four place": "lastTopFourPlace",
  "Last safe place": "lastSafePlace",
};

function OutcomeTable({
  teams,
  onSelectClub,
}: {
  teams: TeamRow[];
  onSelectClub: (team: string) => void;
}) {
  const t = useT();
  const { lang } = useLang();
  return (
    <div className="table-frame">
      <table className="print">
        <caption className="sr-only">{t("seasonOutcomes")}</caption>
        <thead>
          <tr>
            <th scope="col">#</th>
            <th scope="col" className="l">
              {t("club")}
            </th>
            <th scope="col" title={t("raceTitleNote")}>
              {t("title")}
            </th>
            <th scope="col" title={t("tipTopTwo")}>
              {t("topTwoShort")}
            </th>
            <th scope="col" title={t("tipThird")}>
              {t("thirdShort")}
            </th>
            <th scope="col" title={t("tipFourth")}>
              {t("fourthShort")}
            </th>
            <th scope="col" title={t("tipTopFour")}>
              {t("topFourShort")}
            </th>
            <th scope="col" title={t("tipRelegation")}>
              {t("relegationShort")}
            </th>
            <th scope="col" title={t("tipXpts")}>
              {t("xPts")}
            </th>
            <th scope="col" title={t("tipXgd")}>
              {t("xGD")}
            </th>
          </tr>
        </thead>
        <tbody>
          {teams.map((x, i) => (
            <tr key={x.team}>
              <td className="rank">{i + 1}</td>
              <td className="l">
                <button type="button" className="row-link" onClick={() => onSelectClub(x.team)}>
                  <ClubName team={x.team} />
                </button>
              </td>
              <td style={tint(x.title_pct)}>{pct(x.title_pct, lang)}</td>
              <td style={tint(x.top_two_pct)}>{pct(x.top_two_pct, lang)}</td>
              <td style={tint(x.third_pct)}>{pct(x.third_pct, lang)}</td>
              <td style={tint(x.fourth_pct)}>{pct(x.fourth_pct, lang)}</td>
              <td style={tint(x.top_four_pct)}>{pct(x.top_four_pct, lang)}</td>
              <td style={tint(x.relegation_pct, true)}>{pct(x.relegation_pct, lang)}</td>
              <td>{num(x.exp_points, lang, 1)}</td>
              <td>{signed(x.exp_gd, lang, 1)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

/** One race: the clubs still meaningfully involved, biggest share first. */
function RaceCard({ race, teams }: { race: Race; teams: TeamRow[] }) {
  const t = useT();
  const { lang } = useLang();
  const rows = teams
    .map((x) => ({ team: x.team, value: race.pick(x) }))
    .filter((r) => r.value >= 0.5)
    .sort((a, b) => b.value - a.value)
    .slice(0, 8);
  return (
    <section className="race-card">
      <h3 className="section-title">{t(race.title)}</h3>
      <p className="small-print">{t(race.note)}</p>
      {rows.length === 0 ? (
        <p className="note">{t("noClubAbove")}</p>
      ) : (
        <ol className="race-list">
          {rows.map((r) => (
            <li key={r.team} className="race-row">
              <ClubName team={r.team} />
              <Bar value={r.value} tone={race.tone} />
              <span className="val">{pct(r.value, lang)}</span>
            </li>
          ))}
        </ol>
      )}
    </section>
  );
}

function Thresholds({ rows }: { rows: RaceThreshold[] }) {
  const t = useT();
  return (
    <section>
      <h3 className="section-title">{t("whatItTakes")}</h3>
      <p className="note">{t("whatItTakesNote")}</p>
      <div className="table-frame">
        <table className="print">
          <thead>
            <tr>
              <th scope="col" className="l">
                {t("place")}
              </th>
              <th scope="col">{t("halfTheTime")}</th>
              <th scope="col">{t("threeInFour")}</th>
              <th scope="col">{t("nineInTen")}</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((r) => (
              <tr key={r.position}>
                <td className="l">{PLACE[r.label] ? t(PLACE[r.label]) : r.label}</td>
                <td>{r.p50}</td>
                <td>{r.p75}</td>
                <td>
                  <strong>{r.p90}</strong>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  );
}

function Rivals({ pairs }: { pairs: RivalryPair[] }) {
  const t = useT();
  const { lang } = useLang();
  return (
    <section className="races-block">
      <h3 className="section-title">{t("aboveWhom")}</h3>
      <ul className="rivals">
        {pairs.slice(0, 8).map((r) => (
          <li key={`${r.a}-${r.b}`} className="rival">
            <span className="rival-pair">
              <ClubName team={r.a} />
              <span className="rival-op">{t("above")}</span>
              <ClubName team={r.b} />
            </span>
            <span className="val">{pct(r.a_above_pct, lang)}</span>
          </li>
        ))}
      </ul>
    </section>
  );
}

/** Page 4: every season outcome, the four races, the points each place
 *  takes, and the 18×18 grid. */
export function RacesPage({
  data,
  onSelectClub,
}: {
  data: SimResponse;
  onSelectClub: (team: string) => void;
}) {
  const t = useT();
  return (
    <article className="page">
      <PageHead title={t("pageRaces")} page={4} />
      <section>
        <h3 className="section-title">{t("seasonOutcomes")}</h3>
        <p className="note">{t("outcomesNote")}</p>
        <OutcomeTable teams={data.teams} onSelectClub={onSelectClub} />
      </section>
      <div className="spread races-block">
        <div className="race-cards">
          {RACES.map((race) => (
            <RaceCard key={race.title} race={race} teams={data.teams} />
          ))}
        </div>
        <aside>
          <Thresholds rows={data.thresholds} />
          {data.rivalries.length > 0 && <Rivals pairs={data.rivalries} />}
        </aside>
      </div>
      <div className="races-block">
        <PositionGrid positions={data.positions} />
      </div>
    </article>
  );
}
```

- [ ] **Step 3: Append the races styles to `src/styles.css`**

```css
/* ---- Yarışlar (races) ---- */

.races-block {
  margin-top: 30px;
}

.race-cards {
  display: grid;
  gap: 22px;
}

@media (min-width: 640px) {
  .race-cards {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
}

.race-card .small-print {
  margin: -4px 0 6px;
}

.race-row {
  grid-template-columns: minmax(0, 10em) minmax(0, 1fr) 4.6em;
}

.rivals {
  margin: 0;
  padding: 0;
  list-style: none;
}

.rival {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  align-items: center;
  gap: 10px;
  padding: 7px 0;
  border-bottom: 1px solid var(--rule-soft);
  font-family: var(--narrow);
  font-size: 15px;
  font-weight: 600;
}

.rival-pair {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 2px 8px;
  min-width: 0;
}

.rival-op {
  font-family: var(--serif);
  font-style: italic;
  font-weight: 400;
  color: var(--ink-3);
}

.posgrid {
  border-collapse: collapse;
  font-family: var(--narrow);
  font-size: 12px;
  font-variant-numeric: tabular-nums;
}

.posgrid th,
.posgrid td {
  width: 30px;
  min-width: 30px;
  height: 28px;
  padding: 0;
  border: 1px solid var(--rule-soft);
  text-align: center;
}

.posgrid thead th {
  font-weight: 700;
  color: var(--ink-2);
}

.posgrid thead th.top4 {
  box-shadow: inset 0 -3px 0 var(--ink);
}

.posgrid thead th.rel {
  box-shadow: inset 0 -3px 0 var(--red);
}

.posgrid tbody th {
  position: sticky;
  left: 0;
  z-index: 1;
  width: auto;
  padding: 0 10px 0 0;
  background: var(--paper);
  text-align: left;
  white-space: nowrap;
  font-weight: 600;
  font-size: 14px;
}

.posgrid td {
  background: color-mix(in srgb, var(--ink) calc(var(--i) * 46%), transparent);
}
```

- [ ] **Step 4: Verify**

Run: `npx tsc --noEmit && npm run build`
Expected: no type errors; `✓ built`.

- [ ] **Step 5: Commit**

```bash
cd .. && git add frontend/src/pages/RacesPage.tsx frontend/src/components/PositionGrid.tsx frontend/src/styles.css
git commit -m "ui: races page and the position grid in screen tint"
```

---

### Task 6: "Sonuçlar" — the results page

**Files:**
- Create: `frontend/src/pages/ResultsPage.tsx`
- Modify: `frontend/src/styles.css` (append)

**Interfaces:**
- Consumes: `PageHead`, `ResultLine`; `matchdayLabel`, `num`, `stamp`.
- Produces: `ResultsPage({ liveData })` with `liveData: LiveData`.

- [ ] **Step 1: Create `src/pages/ResultsPage.tsx`**

```tsx
import type { LiveData } from "../api";
import { useLang, useT } from "../i18n";
import { matchdayLabel, num, stamp } from "../clubs";
import { PageHead, ResultLine } from "../components/Print";

const TOTAL_FIXTURES = 306;

/** Page 5: every played result, latest matchday first, like the results
 *  column on a programme's back page. */
export function ResultsPage({ liveData }: { liveData: LiveData }) {
  const t = useT();
  const { lang } = useLang();
  const played = liveData.played_matches;
  const goals = played.reduce((sum, m) => sum + m.home_score + m.away_score, 0);
  const rounds = [...new Set(played.map((m) => m.round))].sort((a, b) => b - a);
  const fetched = stamp(liveData.fetched_at, lang);

  return (
    <article className="page">
      <PageHead title={t("pageResults")} page={5} />
      <p className="stats-line">
        {num(played.length, lang)} {t("matchesPlayedLower")} · {num(TOTAL_FIXTURES - played.length, lang)}{" "}
        {t("remainingLower")} · {num(goals, lang)} {t("goalsLower")} ·{" "}
        {num(played.length ? goals / played.length : 0, lang, 2)} {t("goalsPerMatch")}
      </p>
      <div className="results-cols">
        {rounds.map((round) => (
          <section key={round} className="round-block">
            <h3 className="section-title">{matchdayLabel(round, lang)}</h3>
            <ul className="results-list">
              {played
                .filter((m) => m.round === round)
                .map((m) => (
                  <ResultLine
                    key={`${m.home}-${m.away}`}
                    home={m.home}
                    away={m.away}
                    homeScore={m.home_score}
                    awayScore={m.away_score}
                  />
                ))}
            </ul>
          </section>
        ))}
      </div>
      <p className="small-print">
        {t("liveSource")}
        {fetched ? ` · ${t("fetched")} ${fetched}` : ""}.
      </p>
    </article>
  );
}
```

- [ ] **Step 2: Append the results styles to `src/styles.css`**

```css
/* ---- Sonuçlar (results) ---- */

.stats-line {
  margin: 0 0 18px;
  font-family: var(--narrow);
  font-weight: 600;
  font-size: 15px;
  font-variant-numeric: tabular-nums;
}

.results-cols {
  columns: 1;
  column-gap: 36px;
  column-rule: 1px solid var(--rule-soft);
}

@media (min-width: 760px) {
  .results-cols {
    columns: 2;
  }
}

@media (min-width: 1140px) {
  .results-cols {
    columns: 3;
  }
}

.round-block {
  margin-bottom: 22px;
  break-inside: avoid;
}
```

- [ ] **Step 3: Verify**

Run: `npx tsc --noEmit && npm run build`
Expected: no type errors; `✓ built`.

- [ ] **Step 4: Commit**

```bash
cd .. && git add frontend/src/pages/ResultsPage.tsx frontend/src/styles.css
git commit -m "ui: results page as a programme back page"
```

---

### Task 7: Shell — masthead, contents, printer's note; remove the dashboard

**Files:**
- Create: `frontend/src/components/Masthead.tsx`, `frontend/src/components/Colophon.tsx`
- Replace: `frontend/src/App.tsx`
- Modify: `frontend/index.html`, `frontend/src/i18n.tsx` (prune), `frontend/src/styles.css` (append)
- Delete: `frontend/src/components/ForecastView.tsx`, `MatchesView.tsx`, `ResultsTable.tsx`, `RacesView.tsx`, `LeagueTable.tsx`, `LiveStats.tsx`

**Interfaces:**
- Consumes: all five pages, `ClubDetail`; `dateSpan`, `stamp`.
- Produces: `PAGES`, `type Page`, `pageFromHash(): Page`, `Masthead({ issue, dates, conditional, page, onGo })` from `Masthead.tsx`; `type Theme = "light" | "dark"`, `Colophon({ ... })` from `Colophon.tsx`.

- [ ] **Step 1: Create `src/components/Masthead.tsx`**

```tsx
import { useLang, useT, type Key } from "../i18n";
import { dateSpan } from "../clubs";

export const PAGES = ["cover", "fixtures", "table", "races", "results"] as const;
export type Page = (typeof PAGES)[number];

const PAGE_KEY: Record<Page, Key> = {
  cover: "pageCover",
  fixtures: "pageFixtures",
  table: "pageTable",
  races: "pageRaces",
  results: "pageResults",
};

/** The page named in the URL hash, so the back button and shared links
 *  land on the right page. */
export function pageFromHash(): Page {
  const id = window.location.hash.slice(1);
  return (PAGES as readonly string[]).includes(id) ? (id as Page) : "cover";
}

/** The programme's masthead, and the contents strip that stays in reach
 *  while scrolling. */
export function Masthead({
  issue,
  dates,
  conditional,
  page,
  onGo,
}: {
  issue: number | null;
  dates: string[];
  conditional: boolean;
  page: Page;
  onGo: (p: Page) => void;
}) {
  const t = useT();
  const { lang } = useLang();
  return (
    <>
      <header className="masthead">
        <p className="mast-kicker">{t("mastKicker")}</p>
        <h1 className="mast-title">{t("mastTitle")}</h1>
        <p className="mast-issue">
          {issue != null && `${t("issue")} ${issue}`}
          {dates.length > 0 && ` · ${dateSpan(dates, lang)}`}
          {conditional && (
            <>
              {" · "}
              <span className="mast-flag">{t("conditionalEdition")}</span>
            </>
          )}
        </p>
      </header>
      <nav className="contents" aria-label={t("contents")}>
        <span className="contents-label">{t("contents")}</span>
        {PAGES.map((p, i) => (
          <button
            key={p}
            type="button"
            aria-current={page === p ? "page" : undefined}
            onClick={() => onGo(p)}
          >
            <span className="contents-no">{i + 1}</span>
            {t(PAGE_KEY[p])}
          </button>
        ))}
      </nav>
    </>
  );
}
```

- [ ] **Step 2: Create `src/components/Colophon.tsx`**

```tsx
import { useLang, useT } from "../i18n";
import { stamp } from "../clubs";

export type Theme = "light" | "dark";

/** The printer's note: print run, reprint and refresh, the edition
 *  switches, and the not-advice line. */
export function Colophon({
  nSims,
  seed,
  onSims,
  onSeed,
  onReprint,
  onRefresh,
  loading,
  refreshing,
  fetchedAt,
  theme,
  onTheme,
}: {
  nSims: number;
  seed: number;
  onSims: (n: number) => void;
  onSeed: (n: number) => void;
  onReprint: () => void;
  onRefresh: () => void;
  loading: boolean;
  refreshing: boolean;
  fetchedAt: string | null;
  theme: Theme;
  onTheme: (t: Theme) => void;
}) {
  const t = useT();
  const { lang, setLang } = useLang();
  const updated = fetchedAt ? stamp(fetchedAt, lang) : null;

  return (
    <footer className="colophon">
      <form
        className="colophon-run"
        onSubmit={(e) => {
          e.preventDefault();
          onReprint();
        }}
      >
        <span className="colophon-label">{t("printRun")}:</span>
        <label>
          <input
            type="number"
            inputMode="numeric"
            min={100}
            max={200000}
            step={1000}
            value={nSims}
            aria-label={t("sims")}
            onChange={(e) => onSims(Number(e.target.value))}
          />{" "}
          {t("seasonsWord")}
        </label>
        <span aria-hidden="true">·</span>
        <label>
          {t("seed")}{" "}
          <input
            type="number"
            inputMode="numeric"
            value={seed}
            onChange={(e) => onSeed(Number(e.target.value))}
          />
        </label>
        {updated && (
          <span>
            · {t("lastUpdate")} {updated}
          </span>
        )}
        <span className="colophon-actions">
          <button type="submit" className="btn btn-red" disabled={loading}>
            {loading ? t("printing") : t("reprint")}
          </button>
          <button type="button" className="btn" onClick={onRefresh} disabled={refreshing}>
            {refreshing ? t("updating") : t("updateLive")}
          </button>
        </span>
      </form>
      <div className="colophon-toggles">
        <button
          type="button"
          className="btn"
          lang={lang === "tr" ? "en" : "tr"}
          onClick={() => setLang(lang === "tr" ? "en" : "tr")}
        >
          {lang === "tr" ? "English edition" : "Türkçe baskı"}
        </button>
        <button
          type="button"
          className="btn"
          aria-pressed={theme === "dark"}
          onClick={() => onTheme(theme === "dark" ? "light" : "dark")}
        >
          {t("nightEdition")}
        </button>
      </div>
      <p className="legal">
        {t("notAdvice")} {t("sourcesNote")}
      </p>
    </footer>
  );
}
```

- [ ] **Step 3: Replace `src/App.tsx`**

```tsx
import { useCallback, useEffect, useRef, useState } from "react";
import {
  getAccuracy,
  getLiveData,
  getMatches,
  refreshLiveData,
  runSimulation,
  type AccuracyReport,
  type LiveData,
  type MatchesResponse,
  type SimResponse,
  type WhatIf,
} from "./api";
import { useT } from "./i18n";
import { ClubDetail } from "./components/ClubDetail";
import { Colophon, type Theme } from "./components/Colophon";
import { Masthead, pageFromHash, type Page } from "./components/Masthead";
import { CoverPage } from "./pages/CoverPage";
import { FixturesPage } from "./pages/FixturesPage";
import { RacesPage } from "./pages/RacesPage";
import { ResultsPage } from "./pages/ResultsPage";
import { TablePage } from "./pages/TablePage";

// index.html makes the same choice before first paint; this only has to
// agree with it so React state matches the attribute already set.
function initialTheme(): Theme {
  const saved = localStorage.getItem("theme");
  if (saved === "light" || saved === "dark") return saved;
  return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

export default function App() {
  const t = useT();
  const [data, setData] = useState<SimResponse | null>(null);
  const [liveData, setLiveData] = useState<LiveData | null>(null);
  const [matchesData, setMatchesData] = useState<MatchesResponse | null>(null);
  const [accuracy, setAccuracy] = useState<AccuracyReport | null>(null);
  const [loading, setLoading] = useState(false);
  const [refreshing, setRefreshing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [nSims, setNSims] = useState(50000);
  const [seed, setSeed] = useState(12345);
  const [whatIf, setWhatIf] = useState<WhatIf[]>([]);
  const [theme, setTheme] = useState<Theme>(initialTheme);
  const [page, setPage] = useState<Page>(pageFromHash);
  const [club, setClub] = useState<string | null>(null);

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    localStorage.setItem("theme", theme);
    document
      .querySelector('meta[name="theme-color"]')
      ?.setAttribute("content", theme === "dark" ? "#0f1724" : "#f2ecdf");
  }, [theme]);

  useEffect(() => {
    const onHash = () => setPage(pageFromHash());
    window.addEventListener("hashchange", onHash);
    return () => window.removeEventListener("hashchange", onHash);
  }, []);

  const go = useCallback((p: Page) => {
    if (window.location.hash !== `#${p}`) window.location.hash = p;
    setPage(p);
    window.scrollTo({ top: 0 });
  }, []);

  const closeClub = useCallback(() => setClub(null), []);

  const handleSimulate = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      setData(await runSimulation({ n_sims: nSims, seed, what_if: whatIf }));
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [nSims, seed, whatIf]);

  // First load: hydrate cached live data (the backend refreshes it in the
  // background) and print a first edition without any clicks.
  const bootedRef = useRef(false);
  useEffect(() => {
    if (bootedRef.current) return;
    bootedRef.current = true;
    getLiveData()
      .then((live) => {
        if (live) setLiveData(live);
      })
      .catch(() => {
        /* cached live data is optional; refresh is still available */
      });
    getMatches()
      .then(setMatchesData)
      .catch(() => {
        /* the calendar is optional on every page but "Bu Hafta" */
      });
    getAccuracy()
      .then(setAccuracy)
      .catch(() => {
        /* the report card is optional */
      });
    void handleSimulate();
  }, [handleSimulate]);

  const handleRefresh = useCallback(async () => {
    setRefreshing(true);
    setError(null);
    try {
      setLiveData(await refreshLiveData());
      // The refresh moves the backend's played results and ratings on;
      // reprint every derived page so none keeps the old edition.
      const [result, matches, report] = await Promise.all([
        runSimulation({ n_sims: nSims, seed, what_if: whatIf }),
        getMatches().catch(() => null),
        getAccuracy().catch(() => null),
      ]);
      setData(result);
      if (matches) setMatchesData(matches);
      if (report) setAccuracy(report);
    } catch (e) {
      setError(String(e));
    } finally {
      setRefreshing(false);
    }
  }, [nSims, seed, whatIf]);

  const issue = matchesData?.current_round ?? null;
  const issueDates =
    matchesData?.rounds.find((r) => r.round === issue)?.matches.map((m) => m.date) ?? [];
  const selected = data?.teams.find((x) => x.team === club) ?? null;

  return (
    <div className="sheet">
      <Masthead
        issue={issue}
        dates={issueDates}
        conditional={(data?.what_if.length ?? 0) > 0}
        page={page}
        onGo={go}
      />
      {error && (
        <p className="erratum" role="alert">
          <strong>{t("pressError")}:</strong> {error}
        </p>
      )}
      <main>
        {!data ? (
          loading ? (
            <p className="in-press">
              {t("inPress")}
              <small>{t("inPressBody")}</small>
            </p>
          ) : (
            <div className="in-press">
              {t("noForecast")}
              <small>{t("noForecastBody")}</small>
              <p>
                <button type="button" className="btn btn-red" onClick={() => void handleSimulate()}>
                  {t("runSimulation")}
                </button>
              </p>
            </div>
          )
        ) : (
          <>
            {page === "cover" && (
              <CoverPage
                data={data}
                matches={matchesData}
                accuracy={accuracy}
                onOpenFixtures={() => go("fixtures")}
                onSelectClub={setClub}
              />
            )}
            {page === "fixtures" &&
              (matchesData ? (
                <FixturesPage
                  data={matchesData}
                  whatIf={whatIf}
                  onWhatIf={setWhatIf}
                  onReprint={() => void handleSimulate()}
                  busy={loading}
                />
              ) : (
                <p className="note">{t("calendarLoading")}</p>
              ))}
            {page === "table" && <TablePage table={data.table} onSelectClub={setClub} />}
            {page === "races" && <RacesPage data={data} onSelectClub={setClub} />}
            {page === "results" &&
              (liveData ? <ResultsPage liveData={liveData} /> : <p className="note">{t("noLiveData")}</p>)}
          </>
        )}
      </main>
      <Colophon
        nSims={nSims}
        seed={seed}
        onSims={setNSims}
        onSeed={setSeed}
        onReprint={() => void handleSimulate()}
        onRefresh={() => void handleRefresh()}
        loading={loading}
        refreshing={refreshing}
        fetchedAt={liveData?.fetched_at ?? null}
        theme={theme}
        onTheme={setTheme}
      />
      {selected && data && (
        <ClubDetail
          team={selected}
          position={data.positions.find((p) => p.team === selected.team)}
          matches={matchesData}
          onClose={closeClub}
        />
      )}
    </div>
  );
}
```

- [ ] **Step 4: Update `frontend/index.html`**

Set the theme-color meta to `<meta name="theme-color" content="#f2ecdf" />`, the title to `<title>Süper Lig 2026–27 · Maç Programı</title>`, and replace the inline script body with:

```html
    <script>
      // Apply the saved (or OS-preferred) edition before first paint, so a
      // night-edition reader never sees a flash of day paper. Mirrors
      // initialTheme() in App.tsx.
      (function () {
        try {
          var t = localStorage.getItem("theme");
          if (t !== "light" && t !== "dark")
            t = matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
          document.documentElement.dataset.theme = t;
          if (t === "dark")
            document
              .querySelector('meta[name="theme-color"]')
              .setAttribute("content", "#0f1724");
        } catch (e) {}
      })();
    </script>
```

- [ ] **Step 5: Append the shell styles to `src/styles.css`**

```css
/* ---- Masthead and contents ---- */

.masthead {
  display: grid;
  grid-template-columns: minmax(0, 1fr);
  gap: 2px;
  padding: 20px 0 12px;
  border-bottom: 1px solid var(--rule);
}

.mast-kicker {
  margin: 0;
  font-family: var(--narrow);
  font-weight: 700;
  font-size: 12px;
  letter-spacing: 0.16em;
  text-transform: uppercase;
  color: var(--red-ink);
}

.mast-title {
  margin: 0;
  font-family: var(--display);
  font-weight: 900;
  font-size: clamp(52px, 13vw, 118px);
  line-height: 0.82;
  letter-spacing: 0.005em;
  text-transform: uppercase;
}

.mast-issue {
  margin: 6px 0 0;
  font-family: var(--narrow);
  font-weight: 600;
  font-size: 14px;
  font-variant-numeric: tabular-nums;
}

.mast-flag {
  font-weight: 700;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: var(--red-ink);
}

@media (min-width: 760px) {
  .masthead {
    grid-template-columns: minmax(0, 1fr) auto;
    align-items: end;
    column-gap: 24px;
  }
  .mast-kicker {
    grid-column: 1 / -1;
  }
  .mast-issue {
    margin: 0;
    text-align: right;
  }
}

.contents {
  position: sticky;
  top: 0;
  z-index: 10;
  display: flex;
  align-items: baseline;
  gap: 0 18px;
  overflow-x: auto;
  scrollbar-width: none;
  padding: 8px 0 0;
  border-bottom: 3px double var(--rule);
  background-color: var(--paper);
  background-image: var(--grain);
}

.contents::-webkit-scrollbar {
  display: none;
}

.contents-label {
  font-style: italic;
  font-size: 14px;
  white-space: nowrap;
  color: var(--ink-3);
}

.contents button {
  flex: none;
  padding: 4px 0 8px;
  border: 0;
  border-bottom: 3px solid transparent;
  background: none;
  font-family: var(--narrow);
  font-weight: 700;
  font-size: 15px;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  white-space: nowrap;
  cursor: pointer;
}

.contents button[aria-current="page"] {
  border-bottom-color: var(--red);
}

.contents-no {
  margin-right: 5px;
  font-family: var(--display);
  font-weight: 900;
  color: var(--red-ink);
}

@media (max-width: 520px) {
  .contents-label {
    display: none;
  }
}

/* ---- Printer's note ---- */

.colophon {
  display: grid;
  gap: 12px;
  margin-top: 48px;
  padding-top: 14px;
  border-top: 3px double var(--rule);
  font-family: var(--narrow);
  font-size: 14px;
  color: var(--ink-2);
}

.colophon-run {
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  gap: 6px 10px;
}

.colophon-label {
  font-weight: 700;
  letter-spacing: 0.1em;
  text-transform: uppercase;
  color: var(--ink);
}

.colophon input {
  width: 6.5em;
  padding: 0 2px;
  border: 0;
  border-bottom: 1.5px solid var(--ink);
  border-radius: 0;
  background: transparent;
  font-variant-numeric: tabular-nums;
}

.colophon-actions,
.colophon-toggles {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}

.colophon-actions {
  width: 100%;
}

.legal {
  margin: 0;
  font-family: var(--serif);
  font-style: italic;
  font-size: 13.5px;
}
```

- [ ] **Step 6: Delete the dashboard components**

```bash
git rm src/components/ForecastView.tsx src/components/MatchesView.tsx src/components/ResultsTable.tsx src/components/RacesView.tsx src/components/LeagueTable.tsx src/components/LiveStats.tsx
```

- [ ] **Step 7: Prune the dashboard's i18n keys**

Delete the keys only the removed dashboard used (single- and multi-line entries alike). From `frontend/`:

```bash
python3 -I - <<'EOF'
import re
OBSOLETE = set("""allLiveResults brandSub btts clickAClub draw fairOdds fairOddsShort
finalScores fixtureToPlay fixturesToPlay forecastAccuracy fourthPlace goalsScored market
matchPredictions matchesNote matchesPlayed matchesRemaining mdShort mostLikelyChampion
mostLikelyScores nextMatchday nextMatchdayAria odds ofSeasons over25 prevMatchday
probability projectedChampion relegationRisk remaining run running scored seasonProgress
seasonsSeed simulating simulatingBody tabForecast tabLive tabPositions tabRaces tabTable
thirdPlace tipDecimalOdds titleFavorite toDark toLight under25 updated whatIf whatIfNote
win""".split())
path = "src/i18n.tsx"
out, skipping = [], False
for line in open(path, encoding="utf-8").read().split("\n"):
    if skipping:
        if line == "  ],":
            skipping = False
        continue
    m = re.match(r"^  (\w+): \[", line)
    if m and m.group(1) in OBSOLETE:
        skipping = not line.rstrip().endswith("],")
        continue
    out.append(line)
open(path, "w", encoding="utf-8").write("\n".join(out))
EOF
```

Then delete any section comment left with no entries under it (for example `// Forecast summary tiles`). Confirm nothing referenced was removed and nothing dead remains:

```bash
python3 -I -c '
import re, pathlib
src = pathlib.Path("src")
keys = re.findall(r"^  (\w+): \[", (src / "i18n.tsx").read_text(), re.M)
code = "\n".join(p.read_text() for p in src.rglob("*.ts*") if p.name != "i18n.tsx")
print(sorted(k for k in keys if f"\"{k}\"" not in code))
'
```

Expected: `[]`, and `npx tsc --noEmit` passes (a deleted key that is still used fails type checking, because `t()` only accepts `Key`).

- [ ] **Step 8: Verify**

Run: `npm test && npx tsc --noEmit && npm run build && npm audit --audit-level=high`
Expected: tests pass; no type errors; `✓ built`; `found 0 vulnerabilities`. Also confirm the bundle carries no odds copy: `grep -ciE "fair odds|adil oran|decimal odds|ondalık oran" dist/assets/*.js` prints `0`.

- [ ] **Step 9: Commit**

```bash
cd .. && git add -A frontend/src frontend/index.html
git commit -m "ui: programme shell — masthead, contents strip, printer's note; dashboard removed"
```

---

### Task 8: Visual verification and review

**Files:**
- Create (scratch, not committed): `/tmp/claude-1000/-home-cagatay-Desktop-superlig-2026-27-simulator/82b8f970-d2bf-4398-9198-8982ad37be82/scratchpad/pw/ui-check.mjs`
- Modify: only `frontend/src/styles.css` or page components, for defects the checks find

**Interfaces:**
- Consumes: the built app served by the Rust binary.

- [ ] **Step 1: Serve the build with real data**

From the repository root:

```bash
cargo build --release
PORT=3777 LIVE_REFRESH_MINUTES=0 target/release/superlig-sim &
curl -s -X POST http://localhost:3777/api/refresh | head -c 120
```

Expected: JSON starting with `{"played_matches":[` (live TFF results applied).

- [ ] **Step 2: Write the check script**

Create `/tmp/claude-1000/-home-cagatay-Desktop-superlig-2026-27-simulator/82b8f970-d2bf-4398-9198-8982ad37be82/scratchpad/pw/ui-check.mjs` (Playwright is already installed in `/tmp/claude-1000/-home-cagatay-Desktop-superlig-2026-27-simulator/82b8f970-d2bf-4398-9198-8982ad37be82/scratchpad/pw`):

```js
// node ui-check.mjs http://localhost:3777 <outdir>
import { chromium } from "playwright";
import fs from "node:fs";

const [base, out] = process.argv.slice(2);
const exe = ["chromium-1234", "chromium-1228"]
  .map((v) => `/home/cagatay/.cache/ms-playwright/${v}/chrome-linux/chrome`)
  .find((p) => fs.existsSync(p));
const browser = await chromium.launch({ executablePath: exe });
const PAGES = ["cover", "fixtures", "table", "races", "results"];
const failures = [];

for (const [lang, theme] of [["tr", "light"], ["en", "dark"], ["en", "light"], ["tr", "dark"]]) {
  for (const width of [360, 390, 1440]) {
    const ctx = await browser.newContext({ viewport: { width, height: 900 } });
    await ctx.addInitScript(([l, th]) => {
      localStorage.setItem("lang", l);
      localStorage.setItem("theme", th);
    }, [lang, theme]);
    const page = await ctx.newPage();
    for (const p of PAGES) {
      await page.goto(`${base}/#${p}`, { waitUntil: "networkidle" });
      await page.waitForSelector("article.page", { timeout: 60000 });
      const r = await page.evaluate(() => ({
        sw: document.documentElement.scrollWidth,
        iw: window.innerWidth,
        clubsWithoutTr: [...document.querySelectorAll(".club")].filter(
          (e) => e.getAttribute("lang") !== "tr",
        ).length,
      }));
      const tag = `${lang}/${theme}/${width}/${p}`;
      if (r.sw > r.iw) failures.push(`${tag}: page is ${r.sw}px wide in a ${r.iw}px viewport`);
      if (r.clubsWithoutTr) failures.push(`${tag}: ${r.clubsWithoutTr} club names without lang="tr"`);
      if (width !== 360) {
        await page.screenshot({ path: `${out}/${lang}-${theme}-${width}-${p}.png`, fullPage: true });
      }
    }
    if (width === 390) {
      const c = await page.evaluate(() => {
        const cs = getComputedStyle(document.documentElement);
        const hex = (n) => cs.getPropertyValue(n).trim();
        const lum = (h) => {
          const ch = [1, 3, 5]
            .map((i) => parseInt(h.slice(i, i + 2), 16) / 255)
            .map((v) => (v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4));
          return 0.2126 * ch[0] + 0.7152 * ch[1] + 0.0722 * ch[2];
        };
        const ratio = (a, b) => {
          const [x, y] = [lum(hex(a)), lum(hex(b))].sort((p, q) => q - p);
          return (x + 0.05) / (y + 0.05);
        };
        return {
          "ink-2/paper": ratio("--ink-2", "--paper"),
          "ink-3/paper": ratio("--ink-3", "--paper"),
          "ink-3/paper-2": ratio("--ink-3", "--paper-2"),
          "red-ink/paper": ratio("--red-ink", "--paper"),
          "red-ink/paper-2": ratio("--red-ink", "--paper-2"),
        };
      });
      for (const [pair, v] of Object.entries(c)) {
        if (v < 4.5) failures.push(`${theme}: contrast ${pair} is ${v.toFixed(2)} (< 4.5)`);
      }
    }
    await ctx.close();
  }
}

// Review focus: degraded APIs and matchday boundaries.
{
  const ctx = await browser.newContext({ viewport: { width: 390, height: 900 } });
  await ctx.route("**/api/accuracy", (r) => r.abort());
  await ctx.route("**/api/live", (r) => r.abort());
  const page = await ctx.newPage();
  await page.goto(`${base}/#cover`, { waitUntil: "networkidle" });
  if (!(await page.locator(".cover-headline").count())) failures.push("cover did not render with accuracy/live down");
  await page.goto(`${base}/#results`, { waitUntil: "networkidle" });
  if (!(await page.locator("main .note").count())) failures.push("results page shows no note with live data down");
  await ctx.close();
}
{
  const ctx = await browser.newContext({ viewport: { width: 390, height: 900 } });
  const page = await ctx.newPage();
  await page.goto(`${base}/#fixtures`, { waitUntil: "networkidle" });
  await page.waitForSelector(".issue-nav select");
  await page.selectOption(".issue-nav select", "1");
  if ((await page.locator(".stub").count()) !== 0) failures.push("matchday 1 (all played) still shows stubs");
  if (!(await page.locator(".played-block").count())) failures.push("matchday 1 shows no results column");
  if (!(await page.locator(".issue-nav button").first().isDisabled())) failures.push("‹ not disabled on matchday 1");
  await page.selectOption(".issue-nav select", "34");
  if (!(await page.locator(".issue-nav button").last().isDisabled())) failures.push("› not disabled on matchday 34");
  await ctx.close();
}

await browser.close();
console.log(failures.length ? failures.join("\n") : "ALL CHECKS PASSED");
process.exit(failures.length ? 1 : 0);
```

- [ ] **Step 3: Run the checks**

Run: `mkdir -p /tmp/claude-1000/-home-cagatay-Desktop-superlig-2026-27-simulator/82b8f970-d2bf-4398-9198-8982ad37be82/scratchpad/shots-after && node /tmp/claude-1000/-home-cagatay-Desktop-superlig-2026-27-simulator/82b8f970-d2bf-4398-9198-8982ad37be82/scratchpad/pw/ui-check.mjs http://localhost:3777 /tmp/claude-1000/-home-cagatay-Desktop-superlig-2026-27-simulator/82b8f970-d2bf-4398-9198-8982ad37be82/scratchpad/shots-after`
Expected: `ALL CHECKS PASSED`. For any failure, fix the cause in the owning page or `styles.css`, rebuild (`cd frontend && npm run build`; the server reads `frontend/dist` from disk per request), and re-run until it passes.

- [ ] **Step 4: Review the screenshots against the spec**

Open at least `tr-light-390-cover.png`, `tr-light-1440-fixtures.png`, `en-dark-390-table.png`, `en-light-1440-races.png`, `tr-dark-390-results.png`. Check, and fix anything off:
- Turkish glyphs render in all three faces (ğ ş ı İ ç ö ü), including "BAŞAKŞEHİR" in the English edition.
- Club swatches show both stripes; white stripes are visible on cream paper.
- No fair odds, no "1X2" market wording anywhere.
- Red appears only on page numbers, rules, the relegation line, stamps and kicker text.
- Ticket stubs show the perforation and notches; stamps rotate when pressed.

- [ ] **Step 5: Exercise the live interactions**

In a browser at `http://localhost:3777`: stamp two results on "Bu Hafta", press "Bunlarla yeniden bas", confirm the masthead shows "Varsayımlı baskı" and the cover's numbers change; clear the pins and reprint; open a club from the table, press Escape; switch to the English and night editions from the printer's note; use the browser back button between pages.

- [ ] **Step 6: Stop the server, commit any fixes, hand screenshots to the user**

```bash
pkill -x superlig-sim
git add frontend/src && git commit -m "ui: fixes from visual verification"   # only if Step 3–5 changed files
```

Show the user the key screenshots (phone and desktop, day and night). Nothing is pushed or deployed until they approve.
