import type { LiveData } from "../api";
import { useT, useLocale } from "../i18n";

const TOTAL_FIXTURES = 306;

/** The backend sends RFC 3339; show it in the reader's own locale. */
function formatFetchedAt(raw: string, locale?: string): string | null {
  const t = Date.parse(raw);
  return Number.isFinite(t)
    ? new Date(t).toLocaleString(locale, {
        month: "short",
        day: "numeric",
        hour: "2-digit",
        minute: "2-digit",
      })
    : null;
}

export function LiveStats({ liveData }: { liveData: LiveData }) {
  const tr = useT();
  const locale = useLocale();
  const played = liveData.played_matches;
  const goals = played.reduce((sum, m) => sum + m.home_score + m.away_score, 0);
  const rounds = new Set(played.map((m) => m.round));
  const currentRound = rounds.size > 0 ? Math.max(...rounds) : 0;

  // Most recent matchday first — that is what someone checking in wants.
  const byRound = [...rounds].sort((a, b) => b - a);
  const fetchedAt = formatFetchedAt(liveData.fetched_at, locale);

  return (
    <div>
      <div className="tiles">
        <div className="tile">
          <span className="tile-label">{tr("matchesPlayed")}</span>
          <span className="tile-value">{played.length}</span>
        </div>
        <div className="tile">
          <span className="tile-label">{tr("remaining")}</span>
          <span className="tile-value">{TOTAL_FIXTURES - played.length}</span>
        </div>
        <div className="tile">
          <span className="tile-label">{tr("goalsScored")}</span>
          <span className="tile-value">{goals}</span>
        </div>
        <div className="tile">
          <span className="tile-label">{tr("matchday")}</span>
          <span className="tile-value">{currentRound || "—"}</span>
        </div>
      </div>

      <div className="live-grid">
        {byRound.map((round) => {
          const matches = played.filter((m) => m.round === round);
          return (
            <section className="panel" key={round} aria-label={`${tr("matchday")} ${round}`}>
              <header className="panel-head">
                <h3>
                  {tr("matchday")} {round} · {matches.length}
                </h3>
              </header>
              <div className="roster">
                {matches.map((m, i) => (
                  <div key={i} className="result-row">
                    <span>
                      {m.home}{" "}
                      <span className="result-score">
                        {m.home_score}–{m.away_score}
                      </span>{" "}
                      {m.away}
                    </span>
                  </div>
                ))}
              </div>
            </section>
          );
        })}
      </div>

      <p className="source-note">
        {tr("liveSource")}
        {fetchedAt ? ` · ${tr("fetched")} ${fetchedAt}` : ""}.
      </p>
    </div>
  );
}
