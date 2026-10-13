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
