import { useT } from "../i18n";
import {
  sparkPath,
  sparkPoints,
  type Measure,
  type ReconstructionWords,
  type TrendPoint,
  type TrendWords,
} from "../trend";

/** Room on every side for the end dot and its ring. */
const PAD = 5;

/** A small line of one measure over the season, with a dot on its last
 *  point. Not interactive: `label` says what it shows, and the club
 *  dialog's week-by-week table gives every value. */
export function Sparkline({
  series,
  measure,
  scale,
  width,
  height,
  tone,
  label,
}: {
  series: TrendPoint[];
  measure: Measure;
  scale: [number, number];
  width: number;
  height: number;
  tone: "accent" | "red";
  label: string;
}) {
  const points = sparkPoints(series, measure, scale, width, height, PAD);
  const end = points[points.length - 1];
  if (!end) return null;
  return (
    <svg
      className={`spark spark-${tone}`}
      width={width}
      height={height}
      viewBox={`0 0 ${width} ${height}`}
      role="img"
      aria-label={label}
      focusable="false"
    >
      <path className="spark-line" d={sparkPath(points)} />
      <circle className="spark-dot" cx={end.x} cy={end.y} r={4} />
    </svg>
  );
}

/** The translated words trend.ts builds its text from. */
export function useTrendWords(): { point: TrendWords; note: ReconstructionWords } {
  const t = useT();
  return {
    point: { before: t("trendBefore"), after: t("trendAfter"), now: t("trendNow"), between: t("trendBetween") },
    note: { until: t("trendUntil"), untilPreseason: t("trendUntilPreseason"), all: t("trendAll") },
  };
}
