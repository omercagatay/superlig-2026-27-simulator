/* Pure helpers over the /api/matches calendar, kept apart from React so
   calendar.test.mjs tests them directly. */

import type { MatchesResponse, PlayedMatch, WhatIf } from "./api";
// With the extension: node's test runner loads this file as it is.
import { kickoffOrder } from "./clubs.ts";

/** Every played result in the calendar, matchday by matchday and in
 *  kick-off order within one. */
export function playedResults(matches: MatchesResponse): PlayedMatch[] {
  return matches.rounds.flatMap((r) =>
    r.matches
      .filter((m) => m.played && m.home_score != null && m.away_score != null)
      .sort(kickoffOrder)
      .map((m) => ({
        round: r.round,
        home: m.home,
        away: m.away,
        home_score: m.home_score!,
        away_score: m.away_score!,
      })),
  );
}

/** The most picks one simulation takes: the backend's MAX_WHAT_IF
 *  (src/validation.rs) refuses more. */
export const MAX_WHAT_IF = 20;

/** The picks with `home`–`away` assumed to end in `outcome`, or with its
 *  pick cleared when `outcome` is null. A new pick past MAX_WHAT_IF is
 *  not added; changing or clearing one always works. */
export function setPick(whatIf: WhatIf[], home: string, away: string, outcome: string | null): WhatIf[] {
  const rest = whatIf.filter((w) => !(w.home === home && w.away === away));
  if (!outcome) return rest;
  if (rest.length === whatIf.length && whatIf.length >= MAX_WHAT_IF) return whatIf;
  return [...rest, { home, away, outcome }];
}

/** The what-if picks whose fixture is still to play. The backend rejects a
 *  pick on a played match, so a refresh that brings in its result has to
 *  drop the pick before the next simulation. */
export function prunePlayedPicks(whatIf: WhatIf[], matches: MatchesResponse): WhatIf[] {
  const played = new Set(
    matches.rounds.flatMap((r) => r.matches.filter((m) => m.played).map((m) => `${m.home}\n${m.away}`)),
  );
  return whatIf.filter((w) => !played.has(`${w.home}\n${w.away}`));
}
