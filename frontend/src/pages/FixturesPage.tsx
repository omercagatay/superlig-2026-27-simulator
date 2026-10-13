import { useEffect, useState, type CSSProperties } from "react";
import type { MatchCard, MatchForecast, MatchesResponse, WhatIf } from "../api";
import { useLang, useT, type Key } from "../i18n";
import { clubColours, dateSpan, kickoffOrder, matchdayLabel, num, pct, shortDate } from "../clubs";
import { ClubName, ResultLine } from "../components/Common";

type Outcome = "home" | "draw" | "away";

const OUTCOME_KEY: Record<Outcome, Key> = { home: "pickHome", draw: "pickDraw", away: "pickAway" };

/** Home, draw and away as one bar, each side in its club colour. */
function ChanceBar({ m, f }: { m: MatchCard; f: MatchForecast }) {
  return (
    <div className="probbar" aria-hidden="true">
      <span
        className="seg seg-team"
        style={{ width: `${f.home_win_pct}%`, "--c": clubColours(m.home).c1 } as CSSProperties}
      />
      <span className="seg seg-draw" style={{ width: `${f.draw_pct}%` }} />
      <span
        className="seg seg-team"
        style={{ width: `${f.away_win_pct}%`, "--c": clubColours(m.away).c1 } as CSSProperties}
      />
    </div>
  );
}

/** Pick a result to assume it in the next calculation. */
function Pick({
  m,
  pinned,
  onPin,
}: {
  m: MatchCard;
  pinned: Outcome | null;
  onPin: (o: Outcome | null) => void;
}) {
  const t = useT();
  return (
    <div className="pick">
      <span aria-hidden="true">{t("pickLabel")}</span>
      <div className="segmented" role="group" aria-label={`${t("pickLabel")}: ${m.home}–${m.away}`}>
        {(Object.keys(OUTCOME_KEY) as Outcome[]).map((o) => (
          <button
            key={o}
            type="button"
            aria-pressed={pinned === o}
            onClick={() => onPin(pinned === o ? null : o)}
          >
            {t(OUTCOME_KEY[o])}
          </button>
        ))}
      </div>
    </div>
  );
}

function Fixture({
  m,
  f,
  pinned,
  onPin,
}: {
  m: MatchCard;
  f: MatchForecast;
  pinned: Outcome | null;
  onPin: (o: Outcome | null) => void;
}) {
  const t = useT();
  const { lang } = useLang();
  return (
    <li className="fixture">
      <p className="fixture-when">
        {shortDate(m.date, lang)}
        {m.kickoff ? `, ${m.kickoff}` : ""}
      </p>
      <h3 className="fixture-teams">
        <ClubName team={m.home} />
        <span className="vs">–</span>
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
      <p className="fixture-detail">
        xG {num(f.home_xg, lang, 2)}–{num(f.away_xg, lang, 2)}, {t("threePlusGoals")}{" "}
        {pct(f.over25_pct, lang, 0)}, {t("bothScore")} {pct(f.btts_pct, lang, 0)}
      </p>
      <p className="fixture-detail">
        {t("likely")}: {f.likely_scores.map((s) => `${s.home}–${s.away} (${pct(s.pct, lang, 0)})`).join(", ")}
      </p>
      <Pick m={m} pinned={pinned} onPin={onPin} />
    </li>
  );
}

/** One matchday: forecasts for fixtures still to play, results for the rest. */
export function FixturesPage({
  data,
  whatIf,
  onWhatIf,
  onRecalculate,
  busy,
}: {
  data: MatchesResponse;
  whatIf: WhatIf[];
  onWhatIf: (next: WhatIf[]) => void;
  onRecalculate: () => void;
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

  const toPlay = current.matches.filter((m) => !m.played && m.forecast).sort(kickoffOrder);
  const played = current.matches.filter(
    (m) => m.played && m.home_score != null && m.away_score != null,
  );
  const pinOf = (m: MatchCard) =>
    (whatIf.find((w) => w.home === m.home && w.away === m.away)?.outcome ?? null) as Outcome | null;
  const setPin = (m: MatchCard, o: Outcome | null) => {
    const rest = whatIf.filter((w) => !(w.home === m.home && w.away === m.away));
    onWhatIf(o ? [...rest, { home: m.home, away: m.away, outcome: o }] : rest);
  };

  return (
    <>
      <h1 className="page-title">{t("pageFixtures")}</h1>
      <nav className="md-nav" aria-label={t("matchday")}>
        <button
          type="button"
          className="btn"
          onClick={() => setRound(Math.max(first, current.round - 1))}
          disabled={current.round <= first}
          aria-label={t("prevMatchday")}
        >
          ‹
        </button>
        <label>
          <span className="sr-only">{t("matchday")}</span>
          <select value={current.round} onChange={(e) => setRound(Number(e.target.value))}>
            {data.rounds.map((r) => (
              <option key={r.round} value={r.round}>
                {matchdayLabel(r.round, lang)}
              </option>
            ))}
          </select>
        </label>
        <button
          type="button"
          className="btn"
          onClick={() => setRound(Math.min(last, current.round + 1))}
          disabled={current.round >= last}
          aria-label={t("nextMatchday")}
        >
          ›
        </button>
        <span className="muted">{dateSpan(current.matches.map((m) => m.date), lang)}</span>
      </nav>
      <p className="muted">{t("fixturesNote")}</p>
      {whatIf.length > 0 && (
        <div className="picks-bar" role="status">
          <span>
            <strong>{t("assumedResults")}:</strong>{" "}
            <span lang="tr">
              {whatIf.map((w) => `${w.home}–${w.away} (${t(OUTCOME_KEY[w.outcome as Outcome])})`).join(", ")}
            </span>
          </span>
          <span className="picks-actions">
            <button type="button" className="btn btn-primary" onClick={onRecalculate} disabled={busy}>
              {busy ? t("recalculating") : t("recalculate")}
            </button>
            <button type="button" className="btn" onClick={() => onWhatIf([])}>
              {t("clearPicks")}
            </button>
          </span>
        </div>
      )}
      {toPlay.length > 0 && (
        <ul className="fixture-list">
          {toPlay.map((m) => (
            <Fixture
              key={`${m.home}-${m.away}`}
              m={m}
              f={m.forecast!}
              pinned={pinOf(m)}
              onPin={(o) => setPin(m, o)}
            />
          ))}
        </ul>
      )}
      {played.length > 0 && (
        <section>
          <h2 className="section-title">{t("playedThisWeek")}</h2>
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
    </>
  );
}
