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
  calendarNote,
  accuracy,
  onOpenFixtures,
  onSelectClub,
}: {
  data: SimResponse;
  matches: MatchesResponse | null;
  /** What to say while there is no calendar: loading, or failed. */
  calendarNote: string;
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
            <p className="note">{matches ? t("noFixturesLeft") : calendarNote}</p>
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
