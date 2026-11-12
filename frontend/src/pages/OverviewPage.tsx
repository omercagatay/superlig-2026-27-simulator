import type { AccuracyReport, MatchesResponse, SimResponse, TeamRow } from "../api";
import { useLang, useT } from "../i18n";
import { kickoffOrder, num, pct, shortDate } from "../clubs";
import { AccuracyPanel } from "../components/AccuracyPanel";
import { Bar, ClubName } from "../components/Common";
import { Sparkline, useTrendWords } from "../components/Sparkline";
import {
  clubSeries,
  drawable,
  reconstruction,
  reconstructionNote,
  sharedScale,
  sparkLabel,
  type Measure,
  type TrendPoint,
  type TrendView,
} from "../trend";

const SPARK_W = 72;
const SPARK_H = 24;

/** Each row's series for one measure, or null when no row has one to draw. */
function raceSeries(rows: TeamRow[], trend: TrendView | null, m: Measure): (TrendPoint[] | null)[] | null {
  if (!trend) return null;
  const all = rows.map((x) => clubSeries(trend.snapshots, x.team, trend.now));
  const drawn = all.map((s) => (drawable(s, m) ? s : null));
  return drawn.some((s) => s !== null) ? drawn : null;
}

function RaceList({
  rows,
  pick,
  tone,
  ranked,
  measure,
  series,
  current,
  onSelectClub,
}: {
  rows: TeamRow[];
  pick: (x: TeamRow) => number;
  tone: "accent" | "red";
  ranked: boolean;
  measure: Measure;
  /** Each row's trend, from `raceSeries`; null draws none. */
  series: (TrendPoint[] | null)[] | null;
  current: number | null;
  onSelectClub: (team: string) => void;
}) {
  const { lang } = useLang();
  const words = useTrendWords();
  const max = rows.length ? pick(rows[0]) : 1;
  // One scale for the list, so rows compare.
  const scale = series
    ? sharedScale(
        series.filter((s): s is TrendPoint[] => s !== null),
        measure,
      )
    : null;
  return (
    <ol className={series ? "rank-list rank-list-trend" : "rank-list"}>
      {rows.map((x, i) => (
        <li key={x.team}>
          <button type="button" className="rank-row" onClick={() => onSelectClub(x.team)}>
            <span className="rank">{ranked ? i + 1 : ""}</span>
            <ClubName team={x.team} />
            {series && scale && (
              <span className="rank-spark">
                {series[i] && (
                  <Sparkline
                    series={series[i]}
                    measure={measure}
                    scale={scale}
                    width={SPARK_W}
                    height={SPARK_H}
                    tone={tone}
                    label={sparkLabel(series[i], measure, lang, words.point, current)}
                  />
                )}
              </span>
            )}
            <Bar value={pick(x)} max={max} tone={tone} />
            <span className="val">{pct(pick(x), lang)}</span>
          </button>
        </li>
      ))}
    </ol>
  );
}

/** Title and relegation races, this week's fixtures, forecast accuracy. */
export function OverviewPage({
  data,
  matches,
  calendarNote,
  accuracy,
  trend,
  onOpenFixtures,
  onSelectClub,
}: {
  data: SimResponse;
  matches: MatchesResponse | null;
  /** What to say while there is no calendar: loading, or failed. */
  calendarNote: string;
  accuracy: AccuracyReport | null;
  /** The season so far; null shows no trend. */
  trend: TrendView | null;
  onOpenFixtures: () => void;
  onSelectClub: (team: string) => void;
}) {
  const t = useT();
  const { lang } = useLang();
  const contenders = data.teams.filter((x) => x.title_pct > 0).slice(0, 6);
  const strugglers = [...data.teams]
    .filter((x) => x.relegation_pct > 0)
    .sort((a, b) => b.relegation_pct - a.relegation_pct)
    .slice(0, 6);
  const words = useTrendWords();
  const titleSeries = raceSeries(contenders, trend, "title_pct");
  const relegationSeries = raceSeries(strugglers, trend, "relegation_pct");
  const current = trend?.current ?? null;
  const trendNote =
    trend && (titleSeries || relegationSeries)
      ? [
          t("trendNote"),
          reconstructionNote(reconstruction(trend.snapshots, trend.now !== null), lang, words.note),
        ]
          .filter(Boolean)
          .join(" ")
      : null;
  const round = matches?.rounds.find((r) => r.round === matches.current_round);
  const upcoming = round?.matches.filter((m) => !m.played && m.forecast).sort(kickoffOrder) ?? [];

  return (
    <>
      <h1 className="page-title">{t("pageOverview")}</h1>
      <p className="lead">
        {num(data.n_sims, lang)} {t("simulatedLead")}
      </p>
      <div className="columns">
        <div>
          <h2 className="section-title">{t("title")}</h2>
          <RaceList
            rows={contenders}
            pick={(x) => x.title_pct}
            tone="accent"
            ranked
            measure="title_pct"
            series={titleSeries}
            current={current}
            onSelectClub={onSelectClub}
          />
          <h2 className="section-title">{t("relegation")}</h2>
          <RaceList
            rows={strugglers}
            pick={(x) => x.relegation_pct}
            tone="red"
            ranked={false}
            measure="relegation_pct"
            series={relegationSeries}
            current={current}
            onSelectClub={onSelectClub}
          />
          {trendNote && <p className="trend-note">{trendNote}</p>}
        </div>
        <div>
          <h2 className="section-title">{t("thisWeek")}</h2>
          {upcoming.length === 0 ? (
            <p className="muted">{matches ? t("noFixturesLeft") : calendarNote}</p>
          ) : (
            <ul className="week-list">
              <li className="week-head" aria-hidden="true">
                <span />
                <span className="week-pcts">
                  <span>{t("colHome")}</span>
                  <span>{t("colDraw")}</span>
                  <span>{t("colAway")}</span>
                </span>
              </li>
              {upcoming.map((m) => (
                <li key={`${m.home}-${m.away}`} className="week-row">
                  <span className="week-match">
                    <span className="week-when">
                      {shortDate(m.date, lang)}
                      {m.kickoff ? `, ${m.kickoff}` : ""}
                    </span>
                    <span className="week-teams">
                      <ClubName team={m.home} />
                      <span className="vs">–</span>
                      <ClubName team={m.away} />
                    </span>
                  </span>
                  {m.forecast && (
                    <span className="week-pcts">
                      <span>{pct(m.forecast.home_win_pct, lang, 0)}</span>
                      <span>{pct(m.forecast.draw_pct, lang, 0)}</span>
                      <span>{pct(m.forecast.away_win_pct, lang, 0)}</span>
                    </span>
                  )}
                </li>
              ))}
            </ul>
          )}
          <p>
            <button type="button" className="btn" onClick={onOpenFixtures}>
              {t("allFixtures")}
            </button>
          </p>
          {accuracy && <AccuracyPanel report={accuracy} />}
        </div>
      </div>
    </>
  );
}
