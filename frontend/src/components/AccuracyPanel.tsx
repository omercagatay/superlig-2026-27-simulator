import { useEffect, useRef, useState, type KeyboardEvent, type PointerEvent } from "react";
import type { AccuracyReport, ScoredMatch } from "../api";
import { useLang, useT } from "../i18n";
import { dateSpan, matchdayLabel, num, pct, shortDate, signed } from "../clubs";
import {
  byMatchday,
  fill,
  roundTicks,
  runningSeries,
  tickDigits,
  valueTicks,
  type MatchdayHistory,
  type RunningPoint,
} from "../history";
import { Bar, ClubName } from "./Common";

/** How the model did against predictions frozen before kick-off. An empty
 *  log is the honest state early in a season. */
export function AccuracyPanel({ report }: { report: AccuracyReport }) {
  const t = useT();
  const { lang } = useLang();
  const band = (from: number, to: number) =>
    lang === "tr" ? `%${from}–${to}` : `${from}–${to}%`;

  return (
    <section>
      <h2 className="section-title">{t("accuracyTitle")}</h2>
      {report.scored === 0 ? (
        <p className="muted">{t("accuracyEmpty")}</p>
      ) : (
        <>
          <dl className="facts">
            <div>
              <dt>{t("scoredMatches")}</dt>
              <dd>{num(report.scored, lang)}</dd>
            </div>
            <div>
              <dt>{t("calledCorrectly")}</dt>
              <dd>{pct(report.hit_rate_pct, lang, 0, false)}</dd>
            </div>
            <div>
              <dt>{t("logLoss")}</dt>
              <dd>{num(report.log_loss, lang, 3)}</dd>
            </div>
            <div>
              <dt>{t("vsBaseRates")}</dt>
              <dd>{signed(report.log_loss - report.baseline_log_loss, lang, 3)}</dd>
            </div>
          </dl>
          {report.calibration.length > 0 && (
            <ul className="calib" aria-label={t("calibration")}>
              {report.calibration.map((b) => (
                <li key={b.band_from_pct}>
                  <span>{band(b.band_from_pct, b.band_to_pct)}</span>
                  <span className="calib-bars">
                    <Bar value={b.mean_predicted_pct} />
                    <Bar value={b.actual_pct} tone="muted" />
                  </span>
                  <span className="calib-val">
                    {t("predicted")} {pct(b.mean_predicted_pct, lang, 0, false)}, {t("happened")}{" "}
                    {pct(b.actual_pct, lang, 0, false)}
                  </span>
                </li>
              ))}
            </ul>
          )}
          <ForecastHistory matches={report.matches} />
        </>
      )}
    </section>
  );
}

/** Every scored forecast, matchday by matchday, behind a disclosure so the
 *  overview stays short. */
function ForecastHistory({ matches }: { matches: ScoredMatch[] }) {
  const t = useT();
  const days = byMatchday(matches);
  const points = runningSeries(matches);
  return (
    <details className="hist">
      <summary>{t("forecastHistory")}</summary>
      {points.length >= 2 && <RunningChart points={points} />}
      {days.map((day) => (
        <MatchdayBlock key={day.round} day={day} />
      ))}
    </details>
  );
}

function MatchdayBlock({ day }: { day: MatchdayHistory }) {
  const t = useT();
  const { lang } = useLang();
  return (
    <section>
      <h3 className="sub-title">{matchdayLabel(day.round, lang)}</h3>
      <p className="hist-sum">
        {fill(t("matchdayCalled"), { hits: num(day.hits, lang), n: num(day.n, lang) })} ·{" "}
        {fill(t("matchdayLogLoss"), {
          model: num(day.logLoss, lang, 2),
          base: num(day.baselineLogLoss, lang, 2),
        })}
      </p>
      <ul className="hist-list">
        <li className="week-head hist-row" aria-hidden="true">
          <span />
          <span className="week-pcts">
            <span>{t("colHome")}</span>
            <span>{t("colDraw")}</span>
            <span>{t("colAway")}</span>
          </span>
          <span />
          <span />
        </li>
        {day.matches.map((m) => (
          <HistoryRow key={`${m.home}-${m.away}`} m={m} />
        ))}
      </ul>
    </section>
  );
}

/** One frozen forecast against what happened. The outcome that happened is
 *  boxed and bold, the call marked with a tick or a cross, each with text
 *  for screen readers, so nothing rests on colour. */
function HistoryRow({ m }: { m: ScoredMatch }) {
  const t = useT();
  const { lang } = useLang();
  const outcomes = [
    ["1", m.home_pct, t("outHome")],
    ["X", m.draw_pct, t("outDraw")],
    ["2", m.away_pct, t("outAway")],
  ] as const;
  return (
    <li className="week-row hist-row">
      <span className="week-match">
        <span className="week-when">
          {shortDate(m.date, lang)} · {fill(t("forecastMade"), { date: dateSpan([m.logged_at], lang) })}
        </span>
        <span className="week-teams">
          <ClubName team={m.home} />
          <span className="vs">–</span>
          <ClubName team={m.away} />
        </span>
      </span>
      <span className="week-pcts">
        {outcomes.map(([key, value, label]) => (
          <span key={key} className={m.outcome === key ? "hist-pct hist-happened" : "hist-pct"}>
            <span className="sr-only">{label}: </span>
            {pct(value, lang, 0)}
            {m.outcome === key && <span className="sr-only"> ({t("happened")})</span>}
          </span>
        ))}
      </span>
      <span className="score">
        {m.home_score}–{m.away_score}
      </span>
      <span className={m.hit ? "hist-mark" : "hist-mark hist-miss"}>
        <span aria-hidden="true">{m.hit ? "✓" : "✗"}</span>
        <span className="sr-only">{t(m.hit ? "calledIt" : "missedIt")}</span>
      </span>
    </li>
  );
}

/** The width of an element, kept current as it resizes (and 0 while it is
 *  hidden inside a closed <details>). */
function useWidth<T extends HTMLElement>() {
  const ref = useRef<T>(null);
  const [width, setWidth] = useState(0);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    setWidth(el.clientWidth);
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(([entry]) => setWidth(entry.contentRect.width));
    observer.observe(el);
    return () => observer.disconnect();
  }, []);
  return [ref, width] as const;
}

const PLOT_H = 140;
const M = { top: 10, right: 48, bottom: 42, left: 40 };
/** End labels closer than this would collide, so the legend and the facts
 *  above carry the values instead. */
const LABEL_GAP = 14;

/** Season-to-date log-loss after each matchday: the model in the accent, the
 *  base rates muted. Pointer and arrow keys move a crosshair; a hidden table
 *  carries every value for screen readers. */
function RunningChart({ points }: { points: RunningPoint[] }) {
  const t = useT();
  const { lang } = useLang();
  const [ref, width] = useWidth<HTMLDivElement>();
  const [active, setActive] = useState<number | null>(null);

  const height = M.top + PLOT_H + M.bottom;
  const plotW = Math.max(0, width - M.left - M.right);
  const first = points[0].round;
  const end = points[points.length - 1];
  const ticks = valueTicks(points.flatMap((p) => [p.logLoss, p.baselineLogLoss]));
  const [lo, hi] = [ticks[0], ticks[ticks.length - 1]];
  const digits = tickDigits(ticks);
  const x = (round: number) => M.left + ((round - first) / (end.round - first)) * plotW;
  const y = (v: number) => M.top + (1 - (v - lo) / (hi - lo)) * PLOT_H;
  const crisp = (v: number) => Math.round(v) + 0.5;
  const path = (pick: (p: RunningPoint) => number) =>
    points.map((p, i) => `${i ? "L" : "M"}${x(p.round).toFixed(1)},${y(pick(p)).toFixed(1)}`).join("");
  const value = (v: number) => num(v, lang, 3);
  const summary = fill(t("chartSummary"), {
    round: num(end.round, lang),
    model: value(end.logLoss),
    base: value(end.baselineLogLoss),
  });

  const focus = active === null ? null : points[active];
  const marked = focus ?? end;
  const endLabels = Math.abs(y(end.logLoss) - y(end.baselineLogLoss)) >= LABEL_GAP;

  const onPointer = (e: PointerEvent<HTMLDivElement>) => {
    if (plotW <= 0) return;
    const px = e.clientX - e.currentTarget.getBoundingClientRect().left;
    let best = 0;
    points.forEach((p, i) => {
      if (Math.abs(x(p.round) - px) < Math.abs(x(points[best].round) - px)) best = i;
    });
    setActive(best);
  };
  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    const last = points.length - 1;
    const at = active ?? last;
    const next =
      e.key === "ArrowLeft"
        ? Math.max(0, at - 1)
        : e.key === "ArrowRight"
          ? Math.min(last, at + 1)
          : e.key === "Home"
            ? 0
            : e.key === "End"
              ? last
              : null;
    if (next === null) return;
    e.preventDefault();
    setActive(next);
  };

  return (
    <figure className="hist-chart">
      <figcaption>{t("runningLogLoss")}</figcaption>
      <ul className="legend hist-legend" aria-hidden="true">
        <li>
          <span className="hist-key" />
          {t("model")}
        </li>
        <li>
          <span className="hist-key hist-key-base" />
          {t("baseRates")}
        </li>
      </ul>
      <div
        ref={ref}
        className="hist-plot"
        style={{ height }}
        role="img"
        aria-label={summary}
        tabIndex={0}
        onPointerDown={onPointer}
        onPointerMove={onPointer}
        onPointerLeave={(e) => e.pointerType === "mouse" && setActive(null)}
        onFocus={() => setActive((a) => a ?? points.length - 1)}
        onBlur={() => setActive(null)}
        onKeyDown={onKeyDown}
      >
        {plotW > 0 && (
          <svg width={width} height={height} aria-hidden="true" focusable="false">
            {ticks.map((v, i) => (
              <g key={v}>
                <line
                  className={i === 0 ? "hist-axis" : "hist-grid"}
                  x1={M.left}
                  x2={M.left + plotW}
                  y1={crisp(y(v))}
                  y2={crisp(y(v))}
                />
                <text className="hist-tick" x={M.left - 6} y={y(v)} dy="0.32em" textAnchor="end">
                  {num(v, lang, digits)}
                </text>
              </g>
            ))}
            {roundTicks(first, end.round, plotW).map((r) => (
              <text key={r} className="hist-tick" x={x(r)} y={M.top + PLOT_H + 17} textAnchor="middle">
                {num(r, lang)}
              </text>
            ))}
            <text className="hist-tick" x={M.left + plotW / 2} y={M.top + PLOT_H + 36} textAnchor="middle">
              {t("matchday")}
            </text>
            {focus && (
              <line
                className="hist-cross"
                x1={crisp(x(focus.round))}
                x2={crisp(x(focus.round))}
                y1={M.top}
                y2={M.top + PLOT_H}
              />
            )}
            <path className="hist-line hist-line-base" d={path((p) => p.baselineLogLoss)} />
            <path className="hist-line" d={path((p) => p.logLoss)} />
            <circle className="hist-dot hist-dot-base" cx={x(marked.round)} cy={y(marked.baselineLogLoss)} r={4} />
            <circle className="hist-dot" cx={x(marked.round)} cy={y(marked.logLoss)} r={4} />
            {endLabels && (
              <>
                <text className="hist-end" x={x(end.round) + 9} y={y(end.logLoss)} dy="0.32em">
                  {value(end.logLoss)}
                </text>
                <text className="hist-end" x={x(end.round) + 9} y={y(end.baselineLogLoss)} dy="0.32em">
                  {value(end.baselineLogLoss)}
                </text>
              </>
            )}
          </svg>
        )}
        {focus && plotW > 0 && (
          <div
            className={x(focus.round) > width * 0.6 ? "hist-tip hist-tip-flip" : "hist-tip"}
            style={{ left: x(focus.round) + (x(focus.round) > width * 0.6 ? -12 : 12) }}
            aria-hidden="true"
          >
            <div className="hist-tip-title">{matchdayLabel(focus.round, lang)}</div>
            <div className="hist-tip-row">
              <span className="hist-key" />
              <strong>{value(focus.logLoss)}</strong>
              <span className="muted">{t("model")}</span>
            </div>
            <div className="hist-tip-row">
              <span className="hist-key hist-key-base" />
              <strong>{value(focus.baselineLogLoss)}</strong>
              <span className="muted">{t("baseRates")}</span>
            </div>
          </div>
        )}
      </div>
      <table className="sr-only">
        <caption>{t("runningLogLossTable")}</caption>
        <thead>
          <tr>
            <th scope="col">{t("matchday")}</th>
            <th scope="col">{t("matchesCol")}</th>
            <th scope="col">{t("model")}</th>
            <th scope="col">{t("baseRates")}</th>
          </tr>
        </thead>
        <tbody>
          {points.map((p) => (
            <tr key={p.round}>
              <th scope="row">{num(p.round, lang)}</th>
              <td>{num(p.n, lang)}</td>
              <td>{value(p.logLoss)}</td>
              <td>{value(p.baselineLogLoss)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </figure>
  );
}
