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

/** The what-if picks whose fixture is still to play. The backend rejects a
 *  pick on a played match, so a refresh that brings in its result has to
 *  drop the pick before the next simulation. */
export function prunePlayedPicks(whatIf: WhatIf[], matches: MatchesResponse): WhatIf[] {
  const played = new Set(
    matches.rounds.flatMap((r) => r.matches.filter((m) => m.played).map((m) => `${m.home}\n${m.away}`)),
  );
  return whatIf.filter((w) => !played.has(`${w.home}\n${w.away}`));
}
