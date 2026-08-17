import type { TeamRow, RaceThreshold } from "../api";

type Race = {
  title: string;
  note: string;
  pick: (t: TeamRow) => number;
  tone: string;
};

const RACES: Race[] = [
  {
    title: "Title",
    note: "Finishes 1st",
    pick: (t) => t.title_pct,
    tone: "tone-turf",
  },
  {
    title: "Champions League",
    note: "Finishes 1st or 2nd",
    pick: (t) => t.ucl_pct,
    tone: "tone-turf",
  },
  {
    title: "Any European place",
    note: "Finishes in the top 4",
    pick: (t) => t.europe_pct,
    tone: "tone-sky",
  },
  {
    title: "Relegation",
    note: "Finishes in the bottom 3",
    pick: (t) => t.relegation_pct,
    tone: "tone-danger",
  },
];

/** One race: the clubs still meaningfully involved, biggest share first.
 *  Clubs with no realistic stake are dropped rather than listed at 0.0%. */
function RaceColumn({ race, teams }: { race: Race; teams: TeamRow[] }) {
  const rows = teams
    .map((t) => ({ team: t.team, pct: race.pick(t) }))
    .filter((r) => r.pct >= 0.5)
    .sort((a, b) => b.pct - a.pct)
    .slice(0, 8);

  return (
    <section className="panel" aria-label={`${race.title} race`}>
      <header className="panel-head">
        <h3>{race.title}</h3>
        <span className="eyebrow">{race.note}</span>
      </header>
      {rows.length === 0 ? (
        <p className="panel-note">No club is above 0.5%.</p>
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
  return (
    <div className="races">
      <section className="panel" aria-label="Points thresholds">
        <header className="panel-head">
          <h2>What it takes</h2>
        </header>
        <p className="panel-note">
          Points held by the club that actually finished in each place, across
          every simulated season. Read a row as: reach the 90% column and you
          would have taken that place in nine seasons out of ten.
        </p>
        <div className="table-scroll">
          <table className="data-table">
            <thead>
              <tr>
                <th scope="col" className="col-team">Place</th>
                <th scope="col">Half the time</th>
                <th scope="col">3 in 4</th>
                <th scope="col">9 in 10</th>
              </tr>
            </thead>
            <tbody>
              {thresholds.map((t) => (
                <tr key={t.position}>
                  <td className="cell-team">{t.label}</td>
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
