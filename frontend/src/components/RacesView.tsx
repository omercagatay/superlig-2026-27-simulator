import type { TeamRow, RaceThreshold } from "../api";
import { useT, type Key } from "../i18n";

type Race = {
  title: Key;
  note: Key;
  pick: (t: TeamRow) => number;
  tone: string;
};

const RACES: Race[] = [
  {
    title: "title",
    note: "raceTitleNote",
    pick: (t) => t.title_pct,
    tone: "tone-turf",
  },
  {
    title: "topTwo",
    note: "raceTopTwoNote",
    pick: (t) => t.top_two_pct,
    tone: "tone-turf",
  },
  {
    title: "topFour",
    note: "raceTopFourNote",
    pick: (t) => t.top_four_pct,
    tone: "tone-sky",
  },
  {
    title: "relegation",
    note: "raceRelNote",
    pick: (t) => t.relegation_pct,
    tone: "tone-danger",
  },
];

/** One race: the clubs still meaningfully involved, biggest share first.
 *  Clubs with no realistic stake are dropped rather than listed at 0.0%. */
function RaceColumn({ race, teams }: { race: Race; teams: TeamRow[] }) {
  const tr = useT();
  const rows = teams
    .map((t) => ({ team: t.team, pct: race.pick(t) }))
    .filter((r) => r.pct >= 0.5)
    .sort((a, b) => b.pct - a.pct)
    .slice(0, 8);

  return (
    <section className="panel" aria-label={tr(race.title)}>
      <header className="panel-head">
        <h3>{tr(race.title)}</h3>
        <span className="eyebrow">{tr(race.note)}</span>
      </header>
      {rows.length === 0 ? (
        <p className="panel-note">{tr("noClubAbove")}</p>
      ) : (
        rows.map((r) => (
          <div key={r.team} className="finals-row">
            <span className="finals-pair">{r.team}</span>
            <span className="race-mini">
              <span className={`race-mini-fill ${race.tone}`} style={{ width: `${r.pct}%` }} />
            </span>
            <span className="finals-pct">{r.pct.toFixed(1)}%</span>
          </div>
        ))
      )}
    </section>
  );
}

export function RacesView({
  teams,
  thresholds,
}: {
  teams: TeamRow[];
  thresholds: RaceThreshold[];
}) {
  const tr = useT();
  const LABELS: Record<string, Key> = {
    Champion: "champion",
    "Last top-two place": "lastTopTwoPlace",
    "Last top-four place": "lastTopFourPlace",
    "Last safe place": "lastSafePlace",
  };
  return (
    <div className="races">
      <section className="panel" aria-label="Points thresholds">
        <header className="panel-head">
          <h2>{tr("whatItTakes")}</h2>
        </header>
        <p className="panel-note">
          {tr("whatItTakesNote")}
        </p>
        <div className="table-scroll">
          <table className="data-table">
            <thead>
              <tr>
                <th scope="col" className="col-team">{tr("place")}</th>
                <th scope="col">{tr("halfTheTime")}</th>
                <th scope="col">{tr("threeInFour")}</th>
                <th scope="col">{tr("nineInTen")}</th>
              </tr>
            </thead>
            <tbody>
              {thresholds.map((t) => (
                <tr key={t.position}>
                  <td className="cell-team">{LABELS[t.label] ? tr(LABELS[t.label]) : t.label}</td>
                  <td>{t.p50}</td>
                  <td>{t.p75}</td>
                  <td>
                    <strong>{t.p90}</strong>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </section>

      <div className="race-grid">
        {RACES.map((race) => (
          <RaceColumn key={race.title} race={race} teams={teams} />
        ))}
      </div>
    </div>
  );
}
