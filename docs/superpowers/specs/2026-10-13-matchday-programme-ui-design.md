# Matchday programme UI — design

Date: 2026-10-13. Status: superseded the same day by a plain redesign (branch `plain-ui`); the programme look read as AI-generated. Kept for history.

## Intent

Replace the generic dashboard look with a design that feels like it genuinely comes from Turkish football: the printed **matchday programme** (maç programı) sold at the ground. Audience: Turkish fans first, with an English edition. Success: it reads as a programme, not an AI dashboard; every existing feature still works; it works properly on a 390 px phone with no sideways page scroll.

Out of scope: backend and API changes, new features, betting of any kind.

## Visual system

**Inks.** Two-ink print on warm uncoated stock. No rounded cards, shadows or gradients.

| Token | Day edition | Night-match edition |
|---|---|---|
| paper | `#F2ECDF` + faint grain | `#0F1724` |
| ink | `#1E1B17` | `#EDE6D6` |
| spot red | `#C1272D` | `#E2514F` |

The night edition is a designed variant (evening kickoff programme), follows `prefers-color-scheme`, and keeps the existing toggle and pre-paint script. Spot red is reserved for page numbers, rules and the relegation line.

**Type** (self-hosted via `@fontsource`, all with Turkish glyphs):
- Big Shoulders Display 800/900 — page titles, display numerals, club names in headlines.
- Newsreader — sentence headlines, captions, notes.
- Archivo Narrow with tabular figures — tables and fixture data.

**Typesetting.** Turkish edition: `%59,9`, decimal comma, Turkish dates. Uppercase is CSS on elements marked `lang="tr"` (every club name is), so İ/ı are correct in both editions. English edition: `59.9%`.

**Print motifs.** Double and single rules; page numbers (`s. 2`); fixtures numbered `№`; the matchday shown as an issue (`Sayı 8 · 16–19 Ekim`); fixtures as ticket stubs with a perforated edge; probability bars with a halftone-dot fill; a two-stripe scarf swatch per club.

**Club colours** (colours and names only, never crests):

| Club | Colours | Club | Colours |
|---|---|---|---|
| Galatasaray | `#FDB912` / `#A90432` | Gençlerbirliği | `#E30613` / `#111111` |
| Fenerbahçe | `#FFED00` / `#002D72` | Rizespor | `#00843D` / `#0055A5` |
| Beşiktaş | `#111111` / `#FFFFFF` | Alanyaspor | `#F47920` / `#008D36` |
| Trabzonspor | `#6E1E2E` / `#4FA3DB` | Çorum | `#D71920` / `#111111` |
| Başakşehir | `#F26722` / `#1B2A4A` | Kocaelispor | `#00833E` / `#111111` |
| Amedspor | `#1E8F3E` / `#D21F26` | Eyüpspor | `#5B2C83` / `#FFD100` |
| Göztepe | `#FFD200` / `#E30613` | Konyaspor | `#00843D` / `#FFFFFF` |
| Samsunspor | `#E30613` / `#FFFFFF` | Kasımpaşa | `#1B2A4A` / `#FFFFFF` |
| Erzurumspor | `#0055A5` / `#FFFFFF` | Gaziantep | `#E30613` / `#111111` |

Unknown clubs fall back to ink/paper. Swatches carry a hairline border so white stripes survive on cream paper.

## Pages

A contents strip (`1 Kapak · 2 Bu Hafta · 3 Puan Durumu · 4 Yarışlar · 5 Sonuçlar`) sits under the masthead (`SÜPER LİG 2026–27 · MAÇ PROGRAMI · SAYI n · dates`); compact and sticky on phones.

1. **Kapak** — data-written headline (title favourite and chance); title race (top six) and relegation race (bottom six) as halftone bars with swatches, split by a red dashed `küme hattı`; this week's fixtures as one-line teasers; the accuracy panel as a small "Tahmin karnesi" box.
2. **Bu Hafta** — issue navigation (‹ Sayı 8 ›); one ticket stub per unplayed fixture: № number, date and kick-off, both clubs with swatches, the 1/X/2 bar in the two clubs' colours with the draw in neutral, the three percentages, xG, three most likely scores, over 2.5 and both-teams-to-score as small print, and the existing what-if pins as stamp buttons. Played fixtures collapse into a results column.
3. **Puan Durumu** — projected final table (position, swatch, club, expected W/D/L, GD, points) with thin rules under 2nd and 4th and a red rule above the bottom three. Selecting a club opens its page (today's `ClubDetail`: where it finishes, run-in hardest/easiest, expected points).
4. **Yarışlar** — season-outcome table (title, top two, 3rd, 4th, top four, relegation, xPts, xGD; no odds column); race cards with "what it takes"; who finishes above whom; the 18×18 position grid in halftone intensity.
5. **Sonuçlar** — played results by matchday and the live-data stats (today's `LiveStats`).

**Printer's note** (footer on every page): `Baskı: 50.000 sezon · tohum 12345 · son güncelleme …`, the sims/seed inputs, *Yeniden bas* (re-run), *Canlı veriyi güncelle* (refresh), TR/EN and night-edition toggles, and "Bağımsız model tahmini, bahis tavsiyesi değildir".

**Removed:** every fair-odds figure, the stat tiles ("relegation risk", "season progress"), the Barlow fonts.

## Code shape

- `styles.css` rewritten from scratch around the two token sets.
- `main.tsx` imports the three new font packages; Barlow packages removed from `package.json`.
- New `src/clubs.ts`: club colour map and locale-aware formatting helpers (percent, number, uppercase, date). All components format through it.
- `App.tsx` becomes a shell (masthead, contents, current page, printer's note); simulation, refresh, what-if and accuracy state stay where they are.
- One component per page — `CoverPage`, `FixturesPage`, `TablePage`, `RacesPage`, `ResultsPage` — reusing existing data logic; `ClubDetail`, `PositionGrid`, race cards and `LiveStats` are restyled and moved in, not rewritten.
- `i18n.tsx` gains the programme vocabulary in both languages.
- `api.ts` and the backend are unchanged.

## Responsive rules

Mobile first at 390 px. The document never scrolls horizontally; wide tables scroll inside their own frame; fixtures stack in one column; the contents strip stays reachable. Desktop uses a two-column programme spread where content allows.

## Verification

- `npx tsc --noEmit` and `npm run build` pass.
- Playwright screenshots at 1440 px and 390 px, day and night editions, Turkish and English, every page.
- Automated check: `document.documentElement.scrollWidth <= innerWidth` on every page at 390 px.
- Text contrast checked for ink on paper in both editions and for numbers on the halftone bars.
- Live check against a local server with real data: all five pages render, what-if pins and re-run work, club page opens.
- Work is committed locally as it lands; the user reviews screenshots before anything is pushed or deployed.
