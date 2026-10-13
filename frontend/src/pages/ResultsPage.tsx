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
