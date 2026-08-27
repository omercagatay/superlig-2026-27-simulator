import { useEffect, useRef } from "react";
import type { TeamRow, PositionRow, MatchesResponse, MatchCard } from "../api";
import { useT, useLocale } from "../i18n";

const ZONE = (pos: number, total: number) =>
  pos <= 2 ? "zone-ucl" : pos === 3 ? "zone-uel" : pos === 4 ? "zone-uecl" : pos > total - 3 ? "zone-rel" : "";

function fmtDate(iso: string, locale?: string): string {
  const [y, m, d] = iso.split("-").map(Number);
  if (!y || !m || !d) return iso;
  return new Date(y, m - 1, d).toLocaleDateString(locale, { day: "numeric", month: "short" });
}

/** The club's own view of a fixture: who it plays, where, and its chances. */
type ClubFixture = {
  m: MatchCard;
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
      let result: "W" | "D" | "L" | null = null;
      if (m.played && m.home_score != null && m.away_score != null) {
        const gf = atHome ? m.home_score : m.away_score;
        const ga = atHome ? m.away_score : m.home_score;
        result = gf > ga ? "W" : gf === ga ? "D" : "L";
      }
      out.push({
        m,
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
  const tr = useT();
  const locale = useLocale();
  const closeRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    closeRef.current?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const fixtures = matches ? clubFixtures(team.team, matches) : [];
  const remaining = fixtures.filter((f) => !f.m.played && f.winPct != null);
  const playedSoFar = fixtures.filter((f) => f.m.played);
  // Run-in difficulty is just the model's own opinion, averaged: the mean win
  // probability across what is left to play.
  const runIn = remaining.length
    ? remaining.reduce((s, f) => s + (f.winPct ?? 0), 0) / remaining.length
    : null;
  const hardest = remaining.reduce<ClubFixture | null>(
    (worst, f) => (worst === null || (f.winPct ?? 100) < (worst.winPct ?? 100) ? f : worst),
    null
  );
  const easiest = remaining.reduce<ClubFixture | null>(
    (best, f) => (best === null || (f.winPct ?? 0) > (best.winPct ?? 0) ? f : best),
    null
  );
  const total = position?.position_pct.length ?? 18;
  const peak = position ? Math.max(...position.position_pct) : 1;

  return (
    <div className="modal-backdrop" onClick={onClose} role="presentation">
      <div
        className="modal"
        role="dialog"
        aria-modal="true"
        aria-label={`${team.team} detail`}
        onClick={(e) => e.stopPropagation()}
      >
        <header className="panel-head">
          <h2>{team.team}</h2>
          <button ref={closeRef} type="button" className="btn" onClick={onClose}>
            {tr("close")}
          </button>
        </header>

        <div className="panel-body">
          <div className="club-stats">
            <div className="club-stat">
              <span className="tile-label">{tr("title")}</span>
              <span className="club-stat-val">{team.title_pct.toFixed(1)}%</span>
            </div>
            <div className="club-stat">
              <span className="tile-label">{tr("topFour")}</span>
              <span className="club-stat-val">{team.top_four_pct.toFixed(1)}%</span>
            </div>
            <div className="club-stat">
              <span className="tile-label">{tr("relegation")}</span>
              <span className="club-stat-val">{team.relegation_pct.toFixed(1)}%</span>
            </div>
            <div className="club-stat">
              <span className="tile-label">{tr("expectedPoints")}</span>
              <span className="club-stat-val">{team.exp_points.toFixed(1)}</span>
            </div>
            <div className="club-stat">
              <span className="tile-label">{tr("averageFinish")}</span>
              <span className="club-stat-val">{team.mean_position.toFixed(1)}</span>
            </div>
          </div>

          {position && (
            <>
              <span className="score-chips-label">{tr("whereItFinishes")}</span>
              <div className="posbars">
                {position.position_pct.map((pct, i) => (
                  <span
                    key={i}
                    className={`posbar ${ZONE(i + 1, total)}`}
                    title={`${i + 1}${["st", "nd", "rd"][i] ?? "th"}: ${pct.toFixed(1)}%`}
                  >
                    <span className="posbar-fill" style={{ height: `${(pct / peak) * 100}%` }} />
                    <span className="posbar-num">{i + 1}</span>
                  </span>
                ))}
              </div>
            </>
          )}

          {runIn != null && (
            <p className="panel-note">
              {tr("runIn")}: {remaining.length} {tr("toPlayAvg")}{" "}
              <strong>{runIn.toFixed(0)}%</strong>.
              {hardest &&
                ` ${tr("hardest")}: ${hardest.atHome ? "" : "@ "}${hardest.opponent} (${hardest.winPct?.toFixed(0)}%).`}
              {easiest &&
                ` ${tr("easiest")}: ${easiest.atHome ? "" : "@ "}${easiest.opponent} (${easiest.winPct?.toFixed(0)}%).`}
            </p>
          )}

          {playedSoFar.length > 0 && (
            <>
              <span className="score-chips-label">{tr("played")}</span>
              <div className="club-form">
                {playedSoFar.map((f, i) => (
                  <span key={i} className={`form-chip form-${f.result}`}>
                    {f.result} <em>{f.atHome ? "v" : "@"} {f.opponent}</em>
                  </span>
                ))}
              </div>
            </>
          )}

          <span className="score-chips-label">{tr("remainingFixtures")}</span>
          <div className="table-scroll">
            <table className="data-table">
              <thead>
                <tr>
                  <th scope="col">{tr("md")}</th>
                  <th scope="col">{tr("date")}</th>
                  <th scope="col" className="col-team">{tr("opponent")}</th>
                  <th scope="col">{tr("winCol")}</th>
                  <th scope="col">{tr("drawCol")}</th>
                </tr>
              </thead>
              <tbody>
                {remaining.map((f, i) => (
                  <tr key={i}>
                    <td className="cell-rank">{matchRound(f, matches)}</td>
                    <td>{fmtDate(f.m.date, locale)}</td>
                    <td className="cell-team">
                      {f.atHome ? "" : "@ "}
                      {f.opponent}
                    </td>
                    <td>{f.winPct?.toFixed(1)}%</td>
                    <td>{f.drawPct?.toFixed(1)}%</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      </div>
    </div>
  );
}

function matchRound(f: ClubFixture, data: MatchesResponse | null): number | string {
  if (!data) return "";
  const r = data.rounds.find((r) => r.matches.some((m) => m === f.m));
  return r ? r.round : "";
}
