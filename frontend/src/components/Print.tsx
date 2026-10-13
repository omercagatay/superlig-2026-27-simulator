import type { CSSProperties } from "react";
import { clubColours } from "../clubs";
import { useT } from "../i18n";

/** Two-stripe scarf swatch in a club's colours. Decorative: the name next
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

/** A club name with its swatch. Marked Turkish so uppercase styling gives
 *  "İ" and "I" correctly in the English edition too. `flip` puts the
 *  swatch after the name (home sides in a result line). */
export function ClubName({ team, flip = false }: { team: string; flip?: boolean }) {
  return (
    <span className={flip ? "club club-flip" : "club"} lang="tr">
      <Swatch team={team} />
      <span className="club-name">{team}</span>
    </span>
  );
}

/** Halftone probability bar; `max` is the value that fills it. */
export function Bar({
  value,
  max = 100,
  tone = "ink",
}: {
  value: number;
  max?: number;
  tone?: "ink" | "red";
}) {
  const width = max > 0 ? Math.max(0, Math.min(100, (value / max) * 100)) : 0;
  return (
    <span className={`bar bar-${tone}`} aria-hidden="true">
      <span className="bar-fill" style={{ width: `${width}%` }} />
    </span>
  );
}

/** A programme page heading: the title, and the page number in red. */
export function PageHead({ title, page }: { title: string; page: number }) {
  const t = useT();
  return (
    <header className="page-head">
      <h2 className="page-title">{title}</h2>
      <span className="page-num">
        {t("pageAbbr")} {page}
      </span>
    </header>
  );
}
