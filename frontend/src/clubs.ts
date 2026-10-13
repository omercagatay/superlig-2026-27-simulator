/* Programme typesetting: club colours, and every number, percentage and
   date the pages print. Pure functions with no React, so they are tested
   directly (clubs.test.mjs). Colours and names only — crests are
   trademarks and are never drawn. */

export type Lang = "en" | "tr";

export interface ClubColours {
  /** First scarf stripe. */
  c1: string;
  /** Second scarf stripe. */
  c2: string;
}

export const CLUB_COLOURS: Record<string, ClubColours> = {
  Galatasaray: { c1: "#FDB912", c2: "#A90432" },
  Fenerbahçe: { c1: "#FFED00", c2: "#002D72" },
  Beşiktaş: { c1: "#111111", c2: "#FFFFFF" },
  Trabzonspor: { c1: "#6E1E2E", c2: "#4FA3DB" },
  Başakşehir: { c1: "#F26722", c2: "#1B2A4A" },
  Amedspor: { c1: "#1E8F3E", c2: "#D21F26" },
  Göztepe: { c1: "#FFD200", c2: "#E30613" },
  Samsunspor: { c1: "#E30613", c2: "#FFFFFF" },
  Erzurumspor: { c1: "#0055A5", c2: "#FFFFFF" },
  Gençlerbirliği: { c1: "#E30613", c2: "#111111" },
  Rizespor: { c1: "#00843D", c2: "#0055A5" },
  Alanyaspor: { c1: "#F47920", c2: "#008D36" },
  Çorum: { c1: "#D71920", c2: "#111111" },
  Kocaelispor: { c1: "#00833E", c2: "#111111" },
  Eyüpspor: { c1: "#5B2C83", c2: "#FFD100" },
  Konyaspor: { c1: "#00843D", c2: "#FFFFFF" },
  Kasımpaşa: { c1: "#1B2A4A", c2: "#FFFFFF" },
  Gaziantep: { c1: "#E30613", c2: "#111111" },
};

const FALLBACK: ClubColours = { c1: "var(--ink)", c2: "var(--paper)" };

export function clubColours(team: string): ClubColours {
  return CLUB_COLOURS[team] ?? FALLBACK;
}

function locale(lang: Lang): string {
  return lang === "tr" ? "tr-TR" : "en-GB";
}

/** A plain number with the edition's separators: "50.000" / "50,000". */
export function num(value: number, lang: Lang, digits = 0): string {
  return new Intl.NumberFormat(locale(lang), {
    minimumFractionDigits: digits,
    maximumFractionDigits: digits,
  }).format(value);
}

/** "%59,9" in Turkish, "59.9%" in English. Zero or NaN prints "–" unless
 *  `dashZero` is false; a trace below half the last digit prints "<%0,1",
 *  so a real but tiny chance never reads as zero. */
export function pct(value: number, lang: Lang, digits = 1, dashZero = true): string {
  const format = (v: number) =>
    new Intl.NumberFormat(locale(lang), {
      style: "percent",
      minimumFractionDigits: digits,
      maximumFractionDigits: digits,
    }).format(v / 100);
  if (!Number.isFinite(value)) return "–";
  if (value <= 0) return dashZero ? "–" : format(0);
  const step = 10 ** -digits;
  if (value < step / 2) return `<${format(step)}`;
  return format(value);
}

/** Signed, with a true minus sign: "+43,0", "−4,8". Values that round to
 *  zero print unsigned. */
export function signed(value: number, lang: Lang, digits = 1): string {
  const magnitude = num(Math.abs(value), lang, digits);
  if (Number(Math.abs(value).toFixed(digits)) === 0) return magnitude;
  return value > 0 ? `+${magnitude}` : `−${magnitude}`;
}

/** ISO dates are calendar days: parse as local midnight so no time zone
 *  can shift them. */
function parseIso(iso: string): Date | null {
  const [y, m, d] = iso.split("-").map(Number);
  return y && m && d ? new Date(y, m - 1, d) : null;
}

/** A fixture date: "17 Eki Cmt" / "Sat 17 Oct". */
export function shortDate(iso: string, lang: Lang): string {
  const d = parseIso(iso);
  if (!d) return iso;
  return new Intl.DateTimeFormat(locale(lang), {
    weekday: "short",
    day: "numeric",
    month: "short",
  }).format(d);
}

/** A matchday's span for the issue line: "16–19 Ekim", or across months
 *  "30 Eylül – 2 Ekim". */
export function dateSpan(isos: string[], lang: Lang): string {
  const days = [...new Set(isos)]
    .sort()
    .map(parseIso)
    .filter((d): d is Date => d !== null);
  if (days.length === 0) return "";
  const first = days[0];
  const last = days[days.length - 1];
  const dayMonth = (d: Date) =>
    new Intl.DateTimeFormat(locale(lang), { day: "numeric", month: "long" }).format(d);
  if (first.getTime() === last.getTime()) return dayMonth(first);
  if (first.getMonth() === last.getMonth() && first.getFullYear() === last.getFullYear()) {
    const month = new Intl.DateTimeFormat(locale(lang), { month: "long" }).format(last);
    return `${first.getDate()}–${last.getDate()} ${month}`;
  }
  return `${dayMonth(first)} – ${dayMonth(last)}`;
}

/** "8. Hafta" / "Matchday 8". */
export function matchdayLabel(round: number, lang: Lang): string {
  return lang === "tr" ? `${round}. Hafta` : `Matchday ${round}`;
}

/** When the live data was fetched: "13 Eki 02:25"; null if unparseable. */
export function stamp(rfc3339: string, lang: Lang): string | null {
  const ms = Date.parse(rfc3339);
  if (!Number.isFinite(ms)) return null;
  return new Intl.DateTimeFormat(locale(lang), {
    day: "numeric",
    month: "short",
    hour: "2-digit",
    minute: "2-digit",
  }).format(new Date(ms));
}

/** The cover's lead line, written from the title favourite's chance. */
export function coverHeadline(team: string, titlePct: number, lang: Lang): string {
  const chance = pct(titlePct, lang);
  if (titlePct >= 50) {
    return lang === "tr"
      ? `Zirvede ${team}: şampiyonluk ihtimali ${chance}`
      : `${team} on top: ${chance} title chance`;
  }
  return lang === "tr"
    ? `Yarış açık: ${team} ${chance} ile önde`
    : `An open race: ${team} lead on ${chance}`;
}
