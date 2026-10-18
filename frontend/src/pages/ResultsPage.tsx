import type { PlayedMatch } from "../api";
import { useLang, useT } from "../i18n";
import { matchdayLabel, num } from "../clubs";
import { ResultLine } from "../components/Common";

const TOTAL_FIXTURES = 306;

/** Every played result, latest matchday first. */
export function ResultsPage({ played }: { played: PlayedMatch[] }) {
  const t = useT();
  const { lang } = useLang();
  const goals = played.reduce((sum, m) => sum + m.home_score + m.away_score, 0);
  const rounds = [...new Set(played.map((m) => m.round))].sort((a, b) => b - a);

  return (
    <>
      <h1 className="page-title">{t("pageResults")}</h1>
      <p className="lead">
        {num(played.length, lang)} {t("matchesPlayedLower")}, {num(TOTAL_FIXTURES - played.length, lang)}{" "}
        {t("remainingLower")}, {num(goals, lang)} {t("goalsLower")},{" "}
        {num(played.length ? goals / played.length : 0, lang, 2)} {t("goalsPerMatch")}
      </p>
      <div className="results-grid">
        {rounds.map((round) => (
          <section key={round}>
            <h2 className="section-title">{matchdayLabel(round, lang)}</h2>
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
    </>
  );
}
