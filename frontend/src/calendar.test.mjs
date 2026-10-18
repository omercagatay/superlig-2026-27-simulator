// npm test  (node --experimental-strip-types --test src/calendar.test.mjs)
import { test } from "node:test";
import assert from "node:assert/strict";
import { MAX_WHAT_IF, playedResults, prunePlayedPicks, setPick } from "./calendar.ts";

const card = (home, away, date, kickoff, score) => ({
  home,
  away,
  date,
  kickoff,
  played: score !== undefined,
  home_score: score?.[0] ?? null,
  away_score: score?.[1] ?? null,
  forecast: null,
});

// The backend lists a matchday's fixtures by home club, not by kick-off.
const calendar = {
  current_round: 2,
  rounds: [
    {
      round: 1,
      matches: [
        card("Alanyaspor", "Göztepe", "2026-08-09", "19:00", [1, 1]),
        card("Beşiktaş", "Konyaspor", "2026-08-08", "21:30", [3, 0]),
        card("Çorum", "Rizespor", "2026-08-10", null),
        card("Galatasaray", "Gaziantep", "2026-08-08", "19:00", [2, 1]),
      ],
    },
    {
      round: 2,
      matches: [
        card("Göztepe", "Beşiktaş", "2026-08-16", "19:00"),
        card("Konyaspor", "Galatasaray", "2026-08-15", "21:30", [0, 2]),
      ],
    },
  ],
};

test("played results come out matchday by matchday, in kick-off order", () => {
  assert.deepEqual(playedResults(calendar), [
    { round: 1, home: "Galatasaray", away: "Gaziantep", home_score: 2, away_score: 1 },
    { round: 1, home: "Beşiktaş", away: "Konyaspor", home_score: 3, away_score: 0 },
    { round: 1, home: "Alanyaspor", away: "Göztepe", home_score: 1, away_score: 1 },
    { round: 2, home: "Konyaspor", away: "Galatasaray", home_score: 0, away_score: 2 },
  ]);
  // The calendar itself is left in the backend's order.
  assert.equal(calendar.rounds[0].matches[0].home, "Alanyaspor");
  assert.deepEqual(playedResults({ current_round: 1, rounds: [] }), []);
});

test("picks on fixtures that have been played are dropped, the rest kept", () => {
  const whatIf = [
    { home: "Çorum", away: "Rizespor", outcome: "home" },
    { home: "Konyaspor", away: "Galatasaray", outcome: "draw" },
    // The reverse fixture is a different match, still to play.
    { home: "Galatasaray", away: "Konyaspor", outcome: "away" },
    { home: "Göztepe", away: "Beşiktaş", outcome: "away" },
  ];
  const kept = prunePlayedPicks(whatIf, calendar);
  assert.deepEqual(
    kept.map((w) => `${w.home}-${w.away}`),
    ["Çorum-Rizespor", "Galatasaray-Konyaspor", "Göztepe-Beşiktaş"],
  );
  assert.equal(whatIf.length, 4);
  assert.deepEqual(prunePlayedPicks([], calendar), []);
});

test("a pick is set, changed and cleared, and never past the backend's cap", () => {
  const one = setPick([], "Göztepe", "Beşiktaş", "home");
  assert.deepEqual(one, [{ home: "Göztepe", away: "Beşiktaş", outcome: "home" }]);
  assert.deepEqual(setPick(one, "Göztepe", "Beşiktaş", "draw"), [
    { home: "Göztepe", away: "Beşiktaş", outcome: "draw" },
  ]);
  assert.deepEqual(setPick(one, "Göztepe", "Beşiktaş", null), []);
  // The reverse fixture is a pick of its own.
  assert.equal(setPick(one, "Beşiktaş", "Göztepe", "away").length, 2);

  const full = Array.from({ length: MAX_WHAT_IF }, (_, i) => ({ home: `H${i}`, away: `A${i}`, outcome: "home" }));
  assert.equal(MAX_WHAT_IF, 20);
  assert.equal(setPick(full, "Göztepe", "Beşiktaş", "home"), full, "a 21st pick is refused");
  const changed = setPick(full, "H3", "A3", "away");
  assert.equal(changed.length, MAX_WHAT_IF);
  assert.equal(changed.find((w) => w.home === "H3").outcome, "away");
  const cleared = setPick(full, "H3", "A3", null);
  assert.equal(cleared.length, MAX_WHAT_IF - 1);
  assert.equal(setPick(cleared, "Göztepe", "Beşiktaş", "draw").length, MAX_WHAT_IF, "room again");
});
