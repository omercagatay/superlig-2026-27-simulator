import { useEffect, useRef } from "react";
import type { MatchCard, MatchesResponse, PositionRow, TeamRow } from "../api";
import { useLang, useT } from "../i18n";
import { dateSpan, num, pct, shortDate } from "../clubs";
import { ClubName, Swatch } from "./Common";
import { Sparkline, useTrendWords } from "./Sparkline";
import {
  clubSeries,
  drawable,
  ownScale,
  reconstruction,
  reconstructionNote,
  sparkLabel,
  startCaption,
  type Measure,
  type TrendPoint,
  type TrendView,
} from "../trend";

const SPARK_W = 120;
const SPARK_H = 28;
/** Below this a chance prints as "<0.1%" at most: nothing to draw. */
const TRACE = 0.05;

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

/** A club's page, shown as a dialog over the current one. */
export function ClubDetail({
  team,
  position,
  matches,
  trend,
  onClose,
}: {
  team: TeamRow;
  position: PositionRow | undefined;
  matches: MatchesResponse | null;
  /** The season so far; null shows no trend. */
  trend: TrendView | null;
  onClose: () => void;
}) {
  const t = useT();
  const { lang } = useLang();
  const closeRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    const opener = document.activeElement as HTMLElement | null;
    closeRef.current?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
      if (e.key !== "Tab") return;
      // A modal keeps Tab inside itself: wrap from the last control to the
      // first and back, and pull stray focus back in.
      const sheet = closeRef.current?.closest(".modal");
      const items = sheet?.querySelectorAll<HTMLElement>(
        'a[href], button:not([disabled]), input, select, [tabindex]:not([tabindex="-1"])',
      );
      if (!sheet || !items?.length) return;
      const first = items[0];
      const last = items[items.length - 1];
      const at = document.activeElement;
      if (!sheet.contains(at) || (e.shiftKey && at === first) || (!e.shiftKey && at === last)) {
        e.preventDefault();
        (e.shiftKey ? last : first).focus();
      }
    };
    const overflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("keydown", onKey);
      document.body.style.overflow = overflow;
      opener?.focus();
    };
  }, [onClose]);

  const words = useTrendWords();
  const series: TrendPoint[] = trend ? clubSeries(trend.snapshots, team.team, trend.now) : [];
  // Under a figure: its line over the season and where it started. A
  // measure at zero throughout has nothing to show.
  const factTrend = (m: Measure, tone: "accent" | "red") =>
    trend && drawable(series, m, TRACE) ? (
      <dd className="fact-trend">
        <Sparkline
          series={series}
          measure={m}
          scale={ownScale(series, m)}
          width={SPARK_W}
          height={SPARK_H}
          tone={tone}
          label={sparkLabel(series, m, lang, words.point, trend.current)}
        />
        <span className="facts-note" aria-hidden="true">
          {startCaption(series, m, lang, words.point)}
        </span>
      </dd>
    ) : null;
  const pointDate = (p: TrendPoint) =>
    p.now ? t("nowRow") : p.played === 0 || p.latest === null ? t("beforeSeason") : dateSpan([p.latest], lang);
  const historyNote = trend
    ? reconstructionNote(reconstruction(trend.snapshots, trend.now !== null), lang, words.note)
    : "";

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
    <div className="modal-backdrop" onClick={onClose} role="presentation">
      <div
        className="modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="club-title"
        onClick={(e) => e.stopPropagation()}
      >
        <header className="modal-head">
          <h2 id="club-title" className="modal-title" lang="tr">
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
            {factTrend("title_pct", "accent")}
          </div>
          <div>
            <dt>{t("topFour")}</dt>
            <dd>{pct(team.top_four_pct, lang)}</dd>
            {factTrend("top_four_pct", "accent")}
          </div>
          <div>
            <dt>{t("relegation")}</dt>
            <dd>{pct(team.relegation_pct, lang)}</dd>
            {factTrend("relegation_pct", "red")}
          </div>
          <div>
            <dt>{t("expectedPoints")}</dt>
            <dd>{num(team.exp_points, lang, 1)}</dd>
            {factTrend("exp_points", "accent")}
          </div>
          <div>
            <dt>{t("averageFinish")}</dt>
            <dd>{num(team.mean_position, lang, 1)}</dd>
          </div>
        </dl>

        {series.length >= 2 && (
          <details className="hist">
            <summary>{t("weekByWeek")}</summary>
            <div className="table-wrap">
              <table className="data trend-table">
                <thead>
                  <tr>
                    <th scope="col" className="l">
                      {t("date")}
                    </th>
                    <th scope="col">{t("colP")}</th>
                    <th scope="col">{t("titleShort")}</th>
                    <th scope="col">{t("topFourShort")}</th>
                    <th scope="col">{t("relegationShort")}</th>
                    <th scope="col">{t("xPts")}</th>
                  </tr>
                </thead>
                <tbody>
                  {series.map((p) => (
                    <tr key={`${p.played}-${p.now}`}>
                      <th scope="row" className={p.played === 0 && !p.now ? "l trend-wrap" : "l"}>
                        {pointDate(p)}
                      </th>
                      <td>{num(p.played, lang)}</td>
                      <td>{pct(p.title_pct, lang)}</td>
                      <td>{pct(p.top_four_pct, lang)}</td>
                      <td>{pct(p.relegation_pct, lang)}</td>
                      <td>{num(p.exp_points, lang, 1)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            {historyNote && <p className="trend-note">{historyNote}</p>}
          </details>
        )}

        {position && (
          <section>
            <h3 className="sub-title">{t("whereItFinishes")}</h3>
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
          <p className="muted">
            {t("runIn")}: {remaining.length} {t("toPlayAvg")} <strong>{pct(runIn, lang, 0)}</strong>.
            {hardest && ` ${t("hardest")}: ${where(hardest)} (${pct(hardest.winPct ?? 0, lang, 0)}).`}
            {easiest && ` ${t("easiest")}: ${where(easiest)} (${pct(easiest.winPct ?? 0, lang, 0)}).`}
          </p>
        )}

        {playedSoFar.length > 0 && (
          <section>
            <h3 className="sub-title">{t("played")}</h3>
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
            <h3 className="sub-title">{t("remainingFixtures")}</h3>
            <div className="table-wrap">
              <table className="data">
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
