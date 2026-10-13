import type { CSSProperties } from "react";
import { clubColours } from "../clubs";

/** A small two-colour chip in a club's colours. Decorative: the name next
 *  to it carries the meaning. */
export function Swatch({ team }: { team: string }) {
  const { c1, c2 } = clubColours(team);
  return (
    <span
      className="swatch"
      aria-hidden="true"
      style={{ "--c1": c1, "--c2": c2 } as CSSProperties}
    />
  );
}

/** A club name with its colours. `flip` puts the chip after the name (home
 *  sides in a result line). Marked Turkish so screen readers say it right. */
export function ClubName({ team, flip = false }: { team: string; flip?: boolean }) {
  return (
    <span className={flip ? "club club-flip" : "club"} lang="tr">
      <Swatch team={team} />
      <span className="club-name">{team}</span>
    </span>
  );
}

/** Horizontal probability bar; `max` is the value that fills it. */
export function Bar({
  value,
  max = 100,
  tone = "accent",
}: {
  value: number;
  max?: number;
  tone?: "accent" | "red" | "muted";
}) {
  const width = max > 0 ? Math.max(0, Math.min(100, (value / max) * 100)) : 0;
  return (
    <span className={`bar bar-${tone}`} aria-hidden="true">
      <span className="bar-fill" style={{ width: `${width}%` }} />
    </span>
  );
}

/** One played match, home name right-aligned against the score. */
export function ResultLine({
  home,
  away,
  homeScore,
  awayScore,
}: {
  home: string;
  away: string;
  homeScore: number;
  awayScore: number;
}) {
  return (
    <li className="result-line">
      <ClubName team={home} flip />
      <span className="score">
        {homeScore}–{awayScore}
      </span>
      <ClubName team={away} />
    </li>
  );
}
