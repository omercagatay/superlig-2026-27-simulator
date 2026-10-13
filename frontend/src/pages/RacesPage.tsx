import type { RaceThreshold, RivalryPair, SimResponse, TeamRow } from "../api";
import { useLang, useT, type Key } from "../i18n";
import { num, pct, signed } from "../clubs";
import { Bar, ClubName, PageHead } from "../components/Print";
import { PositionGrid } from "../components/PositionGrid";

/** Screen tint for a probability cell: ink (red for relegation) at a
 *  strength that follows the value. */
const tint = (p: number, red = false) =>
  p > 0
    ? {
        background: `color-mix(in srgb, var(${red ? "--red" : "--ink"}) ${(4 + (p / 100) * 24).toFixed(1)}%, transparent)`,
      }
    : undefined;

type Race = { title: Key; note: Key; pick: (x: TeamRow) => number; tone: "ink" | "red" };

const RACES: Race[] = [
  { title: "title", note: "raceTitleNote", pick: (x) => x.title_pct, tone: "ink" },
  { title: "topTwo", note: "raceTopTwoNote", pick: (x) => x.top_two_pct, tone: "ink" },
  { title: "topFour", note: "raceTopFourNote", pick: (x) => x.top_four_pct, tone: "ink" },
  { title: "relegation", note: "raceRelNote", pick: (x) => x.relegation_pct, tone: "red" },
];

const PLACE: Record<string, Key> = {
  Champion: "champion",
  "Last top-two place": "lastTopTwoPlace",
  "Last top-four place": "lastTopFourPlace",
  "Last safe place": "lastSafePlace",
};

function OutcomeTable({
  teams,
  onSelectClub,
}: {
  teams: TeamRow[];
  onSelectClub: (team: string) => void;
}) {
  const t = useT();
  const { lang } = useLang();
  return (
    <div className="table-frame">
      <table className="print">
        <caption className="sr-only">{t("seasonOutcomes")}</caption>
        <thead>
          <tr>
            <th scope="col">#</th>
            <th scope="col" className="l">
              {t("club")}
            </th>
            <th scope="col" title={t("raceTitleNote")}>
              {t("title")}
            </th>
            <th scope="col" title={t("tipTopTwo")}>
              {t("topTwoShort")}
            </th>
            <th scope="col" title={t("tipThird")}>
              {t("thirdShort")}
            </th>
            <th scope="col" title={t("tipFourth")}>
              {t("fourthShort")}
            </th>
            <th scope="col" title={t("tipTopFour")}>
              {t("topFourShort")}
            </th>
            <th scope="col" title={t("tipRelegation")}>
              {t("relegationShort")}
            </th>
            <th scope="col" title={t("tipXpts")}>
              {t("xPts")}
            </th>
            <th scope="col" title={t("tipXgd")}>
              {t("xGD")}
            </th>
          </tr>
        </thead>
        <tbody>
          {teams.map((x, i) => (
            <tr key={x.team}>
              <td className="rank">{i + 1}</td>
              <td className="l">
                <button type="button" className="row-link" onClick={() => onSelectClub(x.team)}>
                  <ClubName team={x.team} />
                </button>
              </td>
              <td style={tint(x.title_pct)}>{pct(x.title_pct, lang)}</td>
              <td style={tint(x.top_two_pct)}>{pct(x.top_two_pct, lang)}</td>
              <td style={tint(x.third_pct)}>{pct(x.third_pct, lang)}</td>
              <td style={tint(x.fourth_pct)}>{pct(x.fourth_pct, lang)}</td>
              <td style={tint(x.top_four_pct)}>{pct(x.top_four_pct, lang)}</td>
              <td style={tint(x.relegation_pct, true)}>{pct(x.relegation_pct, lang)}</td>
              <td>{num(x.exp_points, lang, 1)}</td>
              <td>{signed(x.exp_gd, lang, 1)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

/** One race: the clubs still meaningfully involved, biggest share first. */
function RaceCard({ race, teams }: { race: Race; teams: TeamRow[] }) {
  const t = useT();
  const { lang } = useLang();
  const rows = teams
    .map((x) => ({ team: x.team, value: race.pick(x) }))
    .filter((r) => r.value >= 0.5)
    .sort((a, b) => b.value - a.value)
    .slice(0, 8);
  return (
    <section className="race-card">
      <h3 className="section-title">{t(race.title)}</h3>
      <p className="small-print">{t(race.note)}</p>
      {rows.length === 0 ? (
        <p className="note">{t("noClubAbove")}</p>
      ) : (
        <ol className="race-list">
          {rows.map((r) => (
            <li key={r.team} className="race-row">
              <ClubName team={r.team} />
              <Bar value={r.value} tone={race.tone} />
              <span className="val">{pct(r.value, lang)}</span>
            </li>
          ))}
        </ol>
      )}
    </section>
  );
}

function Thresholds({ rows }: { rows: RaceThreshold[] }) {
  const t = useT();
  return (
    <section>
      <h3 className="section-title">{t("whatItTakes")}</h3>
      <p className="note">{t("whatItTakesNote")}</p>
      <div className="table-frame">
        <table className="print">
          <thead>
            <tr>
              <th scope="col" className="l">
                {t("place")}
              </th>
              <th scope="col">{t("halfTheTime")}</th>
              <th scope="col">{t("threeInFour")}</th>
              <th scope="col">{t("nineInTen")}</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((r) => (
              <tr key={r.position}>
                <td className="l">{PLACE[r.label] ? t(PLACE[r.label]) : r.label}</td>
                <td>{r.p50}</td>
                <td>{r.p75}</td>
                <td>
                  <strong>{r.p90}</strong>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  );
}

function Rivals({ pairs }: { pairs: RivalryPair[] }) {
  const t = useT();
  const { lang } = useLang();
  return (
    <section className="races-block">
      <h3 className="section-title">{t("aboveWhom")}</h3>
      <ul className="rivals">
        {pairs.slice(0, 8).map((r) => (
          <li key={`${r.a}-${r.b}`} className="rival">
            <span className="rival-pair">
              <ClubName team={r.a} />
              <span className="rival-op">{t("above")}</span>
              <ClubName team={r.b} />
            </span>
            <span className="val">{pct(r.a_above_pct, lang)}</span>
          </li>
        ))}
      </ul>
    </section>
  );
}

/** Page 4: every season outcome, the four races, the points each place
 *  takes, and the 18×18 grid. */
export function RacesPage({
  data,
  onSelectClub,
}: {
  data: SimResponse;
  onSelectClub: (team: string) => void;
}) {
  const t = useT();
  return (
    <article className="page">
      <PageHead title={t("pageRaces")} page={4} />
      <section>
        <h3 className="section-title">{t("seasonOutcomes")}</h3>
        <p className="note">{t("outcomesNote")}</p>
        <OutcomeTable teams={data.teams} onSelectClub={onSelectClub} />
      </section>
      <div className="spread races-block">
        <div className="race-cards">
          {RACES.map((race) => (
            <RaceCard key={race.title} race={race} teams={data.teams} />
          ))}
        </div>
        <aside>
          <Thresholds rows={data.thresholds} />
          {data.rivalries.length > 0 && <Rivals pairs={data.rivalries} />}
        </aside>
      </div>
      <div className="races-block">
        <PositionGrid positions={data.positions} />
      </div>
    </article>
  );
}
