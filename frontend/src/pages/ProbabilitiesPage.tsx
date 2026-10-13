import type { RaceThreshold, RivalryPair, SimResponse, TeamRow } from "../api";
import { useLang, useT, type Key } from "../i18n";
import { num, pct, signed } from "../clubs";
import { Bar, ClubName } from "../components/Common";
import { PositionGrid } from "../components/PositionGrid";

/** Cell tint for a probability: accent (red for relegation), stronger with
 *  the value. */
const tint = (p: number, red = false) =>
  p > 0
    ? {
        background: `color-mix(in srgb, var(${red ? "--red" : "--accent"}) ${(4 + (p / 100) * 22).toFixed(1)}%, transparent)`,
      }
    : undefined;

type Race = { title: Key; pick: (x: TeamRow) => number; tone: "accent" | "red" };

const RACES: Race[] = [
  { title: "title", pick: (x) => x.title_pct, tone: "accent" },
  { title: "topTwo", pick: (x) => x.top_two_pct, tone: "accent" },
  { title: "topFour", pick: (x) => x.top_four_pct, tone: "accent" },
  { title: "relegation", pick: (x) => x.relegation_pct, tone: "red" },
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
    <div className="table-wrap">
      <table className="data">
        <thead>
          <tr>
            <th scope="col">#</th>
            <th scope="col" className="l">
              {t("club")}
            </th>
            <th scope="col" title={t("tipTitle")}>
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
function RaceList({ race, teams }: { race: Race; teams: TeamRow[] }) {
  const t = useT();
  const { lang } = useLang();
  const rows = teams
    .map((x) => ({ team: x.team, value: race.pick(x) }))
    .filter((r) => r.value >= 0.5)
    .sort((a, b) => b.value - a.value)
    .slice(0, 8);
  return (
    <section>
      <h3 className="sub-title">{t(race.title)}</h3>
      {rows.length === 0 ? (
        <p className="muted">{t("noClubAbove")}</p>
      ) : (
        <ol className="rank-list">
          {rows.map((r) => (
            <li key={r.team} className="rank-row rank-row-plain">
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

function PointsNeeded({ rows }: { rows: RaceThreshold[] }) {
  const t = useT();
  return (
    <section>
      <h2 className="section-title">{t("pointsNeeded")}</h2>
      <div className="table-wrap">
        <table className="data">
          <thead>
            <tr>
              <th scope="col" className="l">
                {t("place")}
              </th>
              <th scope="col">{t("chance50")}</th>
              <th scope="col">{t("chance75")}</th>
              <th scope="col">{t("chance90")}</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((r) => (
              <tr key={r.position}>
                <td className="l">{PLACE[r.label] ? t(PLACE[r.label]) : r.label}</td>
                <td>{r.p50}</td>
                <td>{r.p75}</td>
                <td className="pts">{r.p90}</td>
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
    <section>
      <h2 className="section-title">{t("aboveWhom")}</h2>
      <ul className="rank-list">
        {pairs.slice(0, 8).map((r) => (
          <li key={`${r.a}-${r.b}`} className="rival-row">
            <span className="rival-pair">
              <ClubName team={r.a} />
              <span className="muted">{t("above")}</span>
              <ClubName team={r.b} />
            </span>
            <span className="val">{pct(r.a_above_pct, lang)}</span>
          </li>
        ))}
      </ul>
    </section>
  );
}

/** Every season outcome, the four races, the points each place takes, and
 *  the 18×18 grid. */
export function ProbabilitiesPage({
  data,
  onSelectClub,
}: {
  data: SimResponse;
  onSelectClub: (team: string) => void;
}) {
  const t = useT();
  return (
    <>
      <h1 className="page-title">{t("pageProbabilities")}</h1>
      <h2 className="section-title">{t("seasonOutcomes")}</h2>
      <OutcomeTable teams={data.teams} onSelectClub={onSelectClub} />
      <div className="race-grid">
        {RACES.map((race) => (
          <RaceList key={race.title} race={race} teams={data.teams} />
        ))}
      </div>
      <div className="columns">
        <PointsNeeded rows={data.thresholds} />
        {data.rivalries.length > 0 && <Rivals pairs={data.rivalries} />}
      </div>
      <PositionGrid positions={data.positions} />
    </>
  );
}
