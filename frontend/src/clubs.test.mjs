// npm test  (node --experimental-strip-types --test src/clubs.test.mjs)
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  CLUB_COLOURS,
  clubColours,
  dateSpan,
  kickoffOrder,
  matchdayLabel,
  num,
  pct,
  shortDate,
  signed,
  stamp,
} from "./clubs.ts";

test("every club has two hex colours; an unknown club falls back to the text colours", () => {
  assert.equal(Object.keys(CLUB_COLOURS).length, 18);
  for (const [club, c] of Object.entries(CLUB_COLOURS)) {
    assert.match(c.c1, /^#[0-9A-F]{6}$/i, club);
    assert.match(c.c2, /^#[0-9A-F]{6}$/i, club);
  }
  assert.deepEqual(clubColours("Real Madrid"), { c1: "var(--text)", c2: "var(--bg)" });
});

test("percentages follow each language, with dashes and traces", () => {
  assert.equal(pct(59.94, "tr"), "%59,9");
  assert.equal(pct(59.94, "en"), "59.9%");
  assert.equal(pct(51.3, "tr", 0), "%51");
  assert.equal(pct(0, "tr"), "–");
  assert.equal(pct(Number.NaN, "en"), "–");
  assert.equal(pct(0, "tr", 0, false), "%0");
  assert.equal(pct(0.02, "tr"), "<%0,1");
  assert.equal(pct(0.02, "en"), "<0.1%");
});

test("numbers use each language's separators and a true minus", () => {
  assert.equal(num(50000, "tr"), "50.000");
  assert.equal(num(50000, "en"), "50,000");
  assert.equal(num(74.31, "tr", 1), "74,3");
  assert.equal(signed(43, "tr"), "+43,0");
  assert.equal(signed(-4.8, "en"), "−4.8");
  assert.equal(signed(-0.01, "en"), "0.0");
  assert.equal(signed(0, "tr", 0), "0");
});

test("dates and matchday labels read naturally in both languages", () => {
  assert.equal(dateSpan(["2026-10-19", "2026-10-16", "2026-10-17"], "tr"), "16–19 Ekim");
  assert.equal(dateSpan(["2026-10-16", "2026-10-19"], "en"), "16–19 October");
  assert.equal(dateSpan(["2026-09-30", "2026-10-02"], "tr"), "30 Eylül – 2 Ekim");
  assert.equal(dateSpan(["2026-10-17"], "en"), "17 October");
  assert.equal(dateSpan([], "tr"), "");
  assert.match(shortDate("2026-10-17", "tr"), /17 Eki/);
  assert.equal(shortDate("not-a-date", "tr"), "not-a-date");
  assert.equal(matchdayLabel(8, "tr"), "8. Hafta");
  assert.equal(matchdayLabel(8, "en"), "Matchday 8");
  assert.equal(stamp("garbage", "tr"), null);
  assert.match(stamp("2026-10-12T23:25:28Z", "tr"), /Eki/);
});

test("fixtures sort by date, then kick-off, unknown kick-offs last", () => {
  const ms = [
    { date: "2026-10-18", kickoff: "13:30" },
    { date: "2026-10-17", kickoff: "19:00" },
    { date: "2026-10-17", kickoff: null },
    { date: "2026-10-17", kickoff: "16:00" },
  ];
  assert.deepEqual(
    [...ms].sort(kickoffOrder).map((m) => `${m.date} ${m.kickoff}`),
    ["2026-10-17 16:00", "2026-10-17 19:00", "2026-10-17 null", "2026-10-18 13:30"],
  );
});
