import { useState } from "react";
import type { MatchesResponse, MatchCard, WhatIf } from "../api";
import { useT, useLocale } from "../i18n";

const fmtOdds = (o: number | null) => (o != null ? o.toFixed(2) : "–");

/** "Sat 22 Aug" in the reader's locale; the ISO date is date-only, so parse
 *  it as local midnight rather than letting UTC shift it a day. */
function fmtDate(iso: string, locale?: string): string {
  const [y, m, d] = iso.split("-").map(Number);
  if (!y || !m || !d) return iso;
  return new Date(y, m - 1, d).toLocaleDateString(locale, {
    weekday: "short",
    day: "numeric",
    month: "short",
  });
}

/** The date span a matchday covers — Süper Lig rounds run Fri to Mon. */
function roundSpan(matches: { date: string }[], locale?: string): string {
  const dates = [...new Set(matches.map((m) => m.date))].sort();
  if (dates.length === 0) return "";
  const first = fmtDate(dates[0], locale);
  const last = fmtDate(dates[dates.length - 1], locale);
  return first === last ? first : `${first} – ${last}`;
}

function PinButtons({
  m,
  pinned,
  onPin,
}: {
  m: MatchCard;
  pinned: string | null;
  onPin: (outcome: string | null) => void;
}) {
  const tr = useT();
  const opts: [string, string, string][] = [
    ["home", "1", tr("pinHome")],
    ["draw", "X", tr("pinDraw")],
    ["away", "2", tr("pinAway")],
  ];
  return (
    <div className="pin-row" role="group" aria-label={`${m.home} v ${m.away}`}>
      {opts.map(([value, glyph, label]) => (
        <button
          key={value}
          type="button"
          className={`pin-btn${pinned === value ? " pin-on" : ""}`}
          aria-pressed={pinned === value}
          title={label}
          onClick={() => onPin(pinned === value ? null : value)}
        >
          {glyph}
        </button>
      ))}
    </div>
  );
}

function ForecastRow({
  m,
  pinned,
  onPin,
}: {
  m: MatchCard;
  pinned: string | null;
  onPin: (outcome: string | null) => void;
}) {
  const tr = useT();
  const locale = useLocale();
  const f = m.forecast;
  if (!f) return null;
  const homeFavored = f.home_win_pct >= f.away_win_pct;
  const mk = m.market;
  return (
    <div className="match-card">
      <div className="fixture-teams">
        <span className={`fixture-team${homeFavored ? " favored" : ""}`}>{m.home}</span>
        <span className="fixture-vs">v</span>
        <span className={`fixture-team${homeFavored ? "" : " favored"}`}>{m.away}</span>
      </div>
      <div className="xg-line">
        <span className="match-when">
          {fmtDate(m.date, locale)}
          {m.kickoff ? ` · ${m.kickoff}` : ""}
        </span>
        xG {f.home_xg.toFixed(2)} – {f.away_xg.toFixed(2)}
      </div>
      <div className="split-bar">
        <div className="split-a" style={{ width: `${f.home_win_pct}%` }} />
        <div className="split-d" style={{ width: `${f.draw_pct}%` }} />
        <div className="split-b" style={{ width: `${f.away_win_pct}%` }} />
      </div>
      <table className="market-table">
        <thead>
          <tr>
            <th scope="col" className="mt-label">
              {tr("market")}
            </th>
            <th scope="col">{tr("probability")}</th>
            <th scope="col">{tr("fairOdds")}</th>
            {mk && <th scope="col">{tr("bookmaker")}</th>}
          </tr>
        </thead>
        <tbody>
          <tr>
            <td className="mt-label">
              <i className="split-key a" aria-hidden="true" />
              {m.home} {tr("win")} <span className="mt-code">1</span>
            </td>
            <td>{f.home_win_pct.toFixed(1)}%</td>
            <td>{fmtOdds(f.home_odds)}</td>
            {mk && <td className="mt-book">{mk.home_odds.toFixed(2)}</td>}
          </tr>
          <tr>
            <td className="mt-label">
              <i className="split-key d" aria-hidden="true" />
              {tr("draw")} <span className="mt-code">X</span>
            </td>
            <td>{f.draw_pct.toFixed(1)}%</td>
            <td>{fmtOdds(f.draw_odds)}</td>
            {mk && <td className="mt-book">{mk.draw_odds.toFixed(2)}</td>}
          </tr>
          <tr className="mt-group-end">
            <td className="mt-label">
              <i className="split-key b" aria-hidden="true" />
              {m.away} {tr("win")} <span className="mt-code">2</span>
            </td>
            <td>{f.away_win_pct.toFixed(1)}%</td>
            <td>{fmtOdds(f.away_odds)}</td>
            {mk && <td className="mt-book">{mk.away_odds.toFixed(2)}</td>}
          </tr>
          <tr>
            <td className="mt-label">{tr("over25")}</td>
            <td>{f.over25_pct.toFixed(1)}%</td>
            <td>{fmtOdds(f.over25_odds)}</td>
            {mk && <td />}
          </tr>
          <tr>
            <td className="mt-label">{tr("under25")}</td>
            <td>{(100 - f.over25_pct).toFixed(1)}%</td>
            <td>{fmtOdds(f.under25_odds)}</td>
            {mk && <td />}
          </tr>
          <tr>
            <td className="mt-label">{tr("btts")}</td>
            <td>{f.btts_pct.toFixed(1)}%</td>
            <td>{fmtOdds(f.btts_odds)}</td>
            {mk && <td />}
          </tr>
        </tbody>
      </table>
      {mk && (
        <div className="edge-line">
          <span className="edge-label">{tr("vsBookmaker")}</span>
          <span className={`edge-val${mk.edge_pct >= 0 ? " edge-up" : " edge-down"}`}>
            {mk.edge_pct >= 0 ? "+" : ""}
            {mk.edge_pct.toFixed(1)} {tr("ptsOn")} {mk.edge_outcome}
          </span>
          <span className="edge-margin">{tr("bookMargin")} {((mk.overround - 1) * 100).toFixed(1)}%</span>
        </div>
      )}
      <PinButtons m={m} pinned={pinned} onPin={onPin} />
      <div className="score-chips">
        <span className="score-chips-label">{tr("mostLikelyScores")}</span>
        {f.likely_scores.map((sc) => (
          <span key={`${sc.home}-${sc.away}`} className="score-chip">
            {sc.home}–{sc.away} <em>{sc.pct.toFixed(1)}%</em>
          </span>
        ))}
      </div>
    </div>
  );
}

/** Played fixtures collapse into one compact block — a result is a fact,
 *  not a forecast, and should not compete with the priced cards. */
function PlayedBlock({ matches }: { matches: MatchCard[] }) {
  const tr = useT();
  const locale = useLocale();
  if (matches.length === 0) return null;
  return (
    <div className="ft-block">
      <span className="ft-block-label">{tr("finalScores")}</span>
      <div className="ft-rows">
        {matches.map((m) => (
          <span key={`${m.home}-${m.away}`} className="ft-row">
            <span className="ft-date">{fmtDate(m.date, locale)}</span> {m.home}{" "}
            <span className="result-score">
              {m.home_score}–{m.away_score}
            </span>{" "}
            {m.away}
          </span>
        ))}
      </div>
    </div>
  );
}

export function MatchesView({
  data,
  whatIf,
  onWhatIf,
}: {
  data: MatchesResponse;
  whatIf: WhatIf[];
  onWhatIf: (next: WhatIf[]) => void;
}) {
  const tr = useT();
  const locale = useLocale();
  const [round, setRound] = useState(data.current_round);
  const total = data.rounds.length;
  const current = data.rounds.find((r) => r.round === round) ?? data.rounds[0];

  return (
    <section className="panel" aria-label="Match predictions">
      <header className="panel-head">
        <h2>{tr("matchPredictions")}</h2>
        <span className="eyebrow">{roundSpan(current.matches, locale)}</span>
        <div className="round-nav" role="group" aria-label={tr("matchday")}>
          <button
            type="button"
            className="btn"
            onClick={() => setRound(Math.max(1, round - 1))}
            disabled={round <= 1}
            aria-label={tr("prevMatchday")}
          >
            ‹
          </button>
          <label className="round-pick">
            <select value={round} onChange={(e) => setRound(Number(e.target.value))}>
              {data.rounds.map((r) => (
                <option key={r.round} value={r.round}>
                  {tr("matchday")} {r.round}
                </option>
              ))}
            </select>
          </label>
          <button
            type="button"
            className="btn"
            onClick={() => setRound(Math.min(total, round + 1))}
            disabled={round >= total}
            aria-label={tr("nextMatchdayAria")}
          >
            ›
          </button>
        </div>
      </header>
      <div className="panel-body">
        <p className="panel-note">
          {tr("matchesNote")} {tr("whatIfNote")}
        </p>
        {whatIf.length > 0 && (
          <div className="pin-summary">
            <span className="score-chips-label">{tr("assuming")}</span>
            {whatIf.map((w) => (
              <span key={`${w.home}-${w.away}`} className="score-chip">
                {w.home} {w.outcome === "home" ? ">" : w.outcome === "away" ? "<" : "="} {w.away}
              </span>
            ))}
            <button type="button" className="btn" onClick={() => onWhatIf([])}>
              {tr("clearPins")}
            </button>
          </div>
        )}
        <PlayedBlock matches={current.matches.filter((m) => m.played)} />
        <div className="match-list">
          {current.matches
            .filter((m) => !m.played)
            .map((m) => (
              <ForecastRow
                key={`${m.home}-${m.away}`}
                m={m}
                pinned={
                  whatIf.find((w) => w.home === m.home && w.away === m.away)?.outcome ?? null
                }
                onPin={(outcome) => {
                  const rest = whatIf.filter((w) => !(w.home === m.home && w.away === m.away));
                  onWhatIf(outcome ? [...rest, { home: m.home, away: m.away, outcome }] : rest);
                }}
              />
            ))}
        </div>
      </div>
    </section>
  );
}
