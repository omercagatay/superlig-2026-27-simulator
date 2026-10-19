#!/usr/bin/env python3
"""Pre-match bookmaker odds for the offline market benchmark.

Offline evaluation only.  These odds never reach the app, its API or its UI
(CLAUDE.md, "No licensed betting sources"), and they are not committed:
data/odds/ is in .gitignore.  examples/market.rs reads them to score the
model against the market.

Source: xgabora's Club Football Match Data archive (MIT), the same file
scripts/fetch_history.py reads (ARCHIVE_URL), built from Football-Data.co.uk.
Its README documents the odds as pre-match prices; it has no closing prices:
  OddHome/OddDraw/OddAway    Bet365 home/draw/away
  MaxHome/MaxDraw/MaxAway    best price over ~17 European bookmakers
  Over25/Under25             Bet365 over/under 2.5 goals
  MaxOver25/MaxUnder25       best over/under 2.5 price

Every Süper Lig row (Division T1) of seasons 2016-17..2025-26 is reconciled
against data/superlig_results.csv: same ordered pair (names mapped by
fetch_history's ARCHIVE_ALIASES) and score, and a date at most
MAX_DATE_SKEW_DAYS away; the history's date is written.  2025-26's history
comes from TFF's pages, so this also checks the archive against TFF.  The
2022-23 earthquake walkovers are dropped as src/history.rs drops them.  A
history match the archive lacks is allowed only inside the archive's known
2024-25 gap (ARCHIVE_GAP, filled from openfootball in the history).

2026-27 rows are reconciled against the played results in
data/fixtures_2026_27.json and written to a file of their own, for the live
comparison with data/forecast_log.json.

Provenance is pinned as in fetch_history.py: ODDS_SLICE_SHA256 is the SHA-256
of the archive rows of 2016-17..2025-26 (after the aliases, odds normalised
by `odds_value`, serialised by `odds_digest`).  A changed digest stops the
run before anything is written: re-verify the upstream change before
updating the constant.  The live season is not pinned: it grows as upstream
adds rounds.

Usage:
  python3 scripts/fetch_odds.py                     # download the archive
  python3 scripts/fetch_odds.py --archive PATH      # a downloaded Matches.csv

It prints, per season, the history's matches and how many have complete
Bet365 and best-price 1X2 and over/under odds.  Standard library only; the
helpers are tested offline by scripts/test_fetch_odds.py.
"""

from __future__ import annotations

import argparse
import csv
import io
import json
import sys
from collections import Counter, defaultdict
from datetime import date
from pathlib import Path
from typing import NamedTuple, Sequence

from fetch_history import (
    ARCHIVE_ALIASES,
    ARCHIVE_URL,
    MAX_DATE_SKEW_DAYS,
    MIN_ARCHIVE_BYTES,
    ROOT,
    check_digest,
    fetch,
    season_label,
    season_start,
    sha256,
)

HISTORY = ROOT / "data" / "superlig_results.csv"
FIXTURES = ROOT / "data" / "fixtures_2026_27.json"
OUT_DIR = ROOT / "data" / "odds"
OUT = OUT_DIR / "superlig_odds.csv"
LIVE_OUT = OUT_DIR / "superlig_odds_2026_27.csv"

FIRST_SEASON, LAST_SEASON = 2016, 2025
LIVE_SEASON = 2026
# SHA-256 of the archive rows of FIRST_SEASON..LAST_SEASON (see the module
# docstring), as read on 2026-10-19.
ODDS_SLICE_SHA256 = (
    "a6fffa9d20829c5848289598ca348826688d38f28c5f504f224c8f4a5b2e8663"
)

# Archive column -> written column, in output order.
ODDS_COLUMNS = {
    "OddHome": "b365_h",
    "OddDraw": "b365_d",
    "OddAway": "b365_a",
    "MaxHome": "max_h",
    "MaxDraw": "max_d",
    "MaxAway": "max_a",
    "Over25": "b365_over25",
    "Under25": "b365_under25",
    "MaxOver25": "max_over25",
    "MaxUnder25": "max_under25",
}
HEADER = ["season", "date", "home", "away", "hs", "as", *ODDS_COLUMNS.values()]
# The complete sets the coverage line counts, as written columns.
MARKETS = {
    "b365 1x2": ("b365_h", "b365_d", "b365_a"),
    "max 1x2": ("max_h", "max_d", "max_a"),
    "b365 o/u": ("b365_over25", "b365_under25"),
    "max o/u": ("max_over25", "max_under25"),
}

# The 2026-27 archive spells the promoted Çorum without its cedilla; the
# history never had the club, so ARCHIVE_ALIASES does not list it.
LIVE_ALIASES = {**ARCHIVE_ALIASES, "Corum": "Çorum"}

# The archive's 2024-25 gap (matchdays 10-12), which the history fills from
# openfootball: the only history matches the archive may lack.
ARCHIVE_GAP = (date(2024, 10, 25), date(2024, 11, 10))

# A Bet365 book (sum of 1/odds) outside its range, above the low end and up
# to the high one, is a parsing or upstream error, not a price.  On
# 2026-10-19 the 1X2 books ran 1.049 to 1.137 and the over/under books 1.046
# to 1.299: two over/under rows (Fenerbahçe v Gaziantepspor 2016-09-25,
# Galatasaray v Erzurumspor 2018-09-28) carry near-equal over and under
# prices upstream and are kept as they are.
BOOK_RANGES = {"b365 1x2": (1.0, 1.25), "b365 o/u": (1.0, 1.35)}


class OddsRow(NamedTuple):
    season: int
    date: str
    home: str
    away: str
    hs: int
    as_: int
    # Written column -> normalised odds text, "" when the archive has none.
    odds: dict[str, str]

    def cells(self) -> list[object]:
        return [
            season_label(self.season),
            self.date,
            self.home,
            self.away,
            self.hs,
            self.as_,
            *(self.odds[c] for c in ODDS_COLUMNS.values()),
        ]

    def complete(self, columns: Sequence[str]) -> bool:
        return all(self.odds[c] for c in columns)


def is_quake_walkover(day: date, home: str, away: str) -> bool:
    """src/history.rs's is_unplayed_award: after the 6 February 2023
    earthquake every remaining 2022-23 fixture of Hatayspor and Gaziantep was
    awarded 3-0 unplayed."""
    return date(2023, 2, 6) <= day <= date(2023, 7, 31) and bool(
        {home, away} & {"Hatayspor", "Gaziantep"}
    )


def odds_value(raw: str | None, where: str) -> str:
    """Decimal odds as `repr(float)`, so "2" and "2.00" read alike; "" when
    blank.  Anything that is not a price above 1 fails."""
    text = (raw or "").strip()
    if not text:
        return ""
    try:
        value = float(text)
    except ValueError:
        raise RuntimeError(f"{where}: odds {text!r} are not a number") from None
    if not 1.0 < value < float("inf"):
        raise RuntimeError(f"{where}: odds {text!r} are not a price above 1")
    return repr(value)


def parse_archive_odds(
    text: str,
    seasons: range,
    aliases: dict[str, str] = ARCHIVE_ALIASES,
) -> list[OddsRow]:
    """Süper Lig rows of `seasons` (start years) with their odds, names
    aliased.  Archive dates are kept; reconciliation replaces them."""
    reader = csv.DictReader(io.StringIO(text))
    required = {"Division", "MatchDate", "HomeTeam", "AwayTeam", "FTHome", "FTAway", *ODDS_COLUMNS}
    if not reader.fieldnames or not required.issubset(reader.fieldnames):
        missing = sorted(required - set(reader.fieldnames or []))
        raise RuntimeError(f"archive columns changed: missing {missing}")
    rows: list[OddsRow] = []
    for source in reader:
        if source["Division"] != "T1":
            continue
        match_date = date.fromisoformat(source["MatchDate"])
        season = season_start(match_date)
        if season not in seasons:
            continue
        where = f"archive {source['MatchDate']} {source['HomeTeam']} v {source['AwayTeam']}"
        try:
            hs, as_ = float(source["FTHome"]), float(source["FTAway"])
        except (TypeError, ValueError):
            raise RuntimeError(f"{where}: invalid score") from None
        if not (hs.is_integer() and as_.is_integer()) or hs < 0 or as_ < 0:
            raise RuntimeError(f"{where}: non-integral score")
        odds = {out: odds_value(source[col], where) for col, out in ODDS_COLUMNS.items()}
        row = OddsRow(
            season,
            match_date.isoformat(),
            aliases.get(source["HomeTeam"], source["HomeTeam"]),
            aliases.get(source["AwayTeam"], source["AwayTeam"]),
            int(hs),
            int(as_),
            odds,
        )
        for market, (low, high) in BOOK_RANGES.items():
            if row.complete(MARKETS[market]):
                book = sum(1.0 / float(row.odds[c]) for c in MARKETS[market])
                if not low < book <= high:
                    raise RuntimeError(f"{where}: {market} book {book:.4f} is not a price")
        rows.append(row)
    return rows


def odds_digest(rows: Sequence[OddsRow]) -> str:
    """SHA-256 of every row's season, date, pair, score and odds, in a
    canonical order and serialisation."""
    keyed = sorted(tuple(str(cell) for cell in row.cells()) for row in rows)
    out = io.StringIO()
    csv.writer(out, lineterminator="\n").writerows(keyed)
    return sha256(out.getvalue().encode("utf-8"))


class Played(NamedTuple):
    date: str
    hs: int
    as_: int


def history_matches(path: Path, seasons: range) -> dict[int, dict[tuple[str, str], Played]]:
    """Played history matches of `seasons` by season and ordered pair, the
    earthquake walkovers left out."""
    out: dict[int, dict[tuple[str, str], Played]] = defaultdict(dict)
    with path.open(encoding="utf-8", newline="") as handle:
        for record in csv.DictReader(handle):
            day = date.fromisoformat(record["date"])
            season = season_start(day)
            home, away = record["home_team"], record["away_team"]
            if season not in seasons or is_quake_walkover(day, home, away):
                continue
            if (home, away) in out[season]:
                raise RuntimeError(f"history: {home} v {away} twice in {season_label(season)}")
            out[season][(home, away)] = Played(
                day.isoformat(), int(record["home_score"]), int(record["away_score"])
            )
    return dict(out)


def fixture_results(path: Path) -> dict[tuple[str, str], Played | None]:
    """Every 2026-27 fixture by ordered pair: its date and result, or None
    while it is unplayed."""
    out: dict[tuple[str, str], Played | None] = {}
    for f in json.loads(path.read_text(encoding="utf-8")):
        played = f["home_score"] is not None and f["away_score"] is not None
        out[(f["home"], f["away"])] = (
            Played(f["date"], f["home_score"], f["away_score"]) if played else None
        )
    return out


def skew(a: str, b: str) -> int:
    return abs((date.fromisoformat(a) - date.fromisoformat(b)).days)


class Reconciled(NamedTuple):
    rows: list[OddsRow]
    # History matches per season, and the ones the archive lacks.
    history: Counter[int]
    missing: list[tuple[int, str, str, str]]
    walkovers: int
    date_differences: list[tuple[OddsRow, str]]


def reconcile_history(
    archive: Sequence[OddsRow],
    history: dict[int, dict[tuple[str, str], Played]],
    gap: tuple[date, date] = ARCHIVE_GAP,
) -> Reconciled:
    """Archive rows matched to history matches: same season, ordered pair and
    score, dates at most MAX_DATE_SKEW_DAYS apart.  The written row takes the
    history's date.  Every archive row must match (walkovers aside), and
    every history match must have an archive row unless it lies in `gap`."""
    problems: list[str] = []
    rows: list[OddsRow] = []
    seen: set[tuple[int, str, str]] = set()
    walkovers = 0
    date_differences: list[tuple[OddsRow, str]] = []
    for row in archive:
        if is_quake_walkover(date.fromisoformat(row.date), row.home, row.away):
            walkovers += 1
            continue
        key = (row.season, row.home, row.away)
        label = f"{row.date} {row.home} v {row.away}"
        if key in seen:
            problems.append(f"archive has {label} twice in {season_label(row.season)}")
            continue
        seen.add(key)
        played = history.get(row.season, {}).get((row.home, row.away))
        if played is None:
            problems.append(f"archive {label} is not in the history (an unmapped name?)")
            continue
        if (played.hs, played.as_) != (row.hs, row.as_):
            problems.append(
                f"{label}: archive {row.hs}-{row.as_}, history {played.hs}-{played.as_}"
            )
            continue
        if skew(played.date, row.date) > MAX_DATE_SKEW_DAYS:
            problems.append(f"{row.home} v {row.away}: archive {row.date}, history {played.date}")
            continue
        if played.date != row.date:
            date_differences.append((row, played.date))
        rows.append(row._replace(date=played.date))

    missing: list[tuple[int, str, str, str]] = []
    for season, matches in sorted(history.items()):
        for (home, away), played in sorted(matches.items()):
            if (season, home, away) in seen:
                continue
            missing.append((season, played.date, home, away))
            if not gap[0] <= date.fromisoformat(played.date) <= gap[1]:
                problems.append(
                    f"history {played.date} {home} v {away} has no archive row "
                    f"outside the known gap {gap[0]}..{gap[1]}"
                )
    if problems:
        raise RuntimeError("archive odds and history disagree:\n  " + "\n  ".join(problems))
    rows.sort(key=lambda r: (r.date, r.home, r.away))
    counts = Counter({season: len(matches) for season, matches in history.items()})
    return Reconciled(rows, counts, missing, walkovers, date_differences)


def reconcile_live(
    archive: Sequence[OddsRow], fixtures: dict[tuple[str, str], Played | None]
) -> tuple[list[OddsRow], list[tuple[OddsRow, str]]]:
    """2026-27 archive rows matched to played fixtures: same ordered pair and
    score, dates at most MAX_DATE_SKEW_DAYS apart; the fixture's date is
    written.  Any row that does not match fails."""
    problems: list[str] = []
    rows: list[OddsRow] = []
    seen: set[tuple[str, str]] = set()
    date_differences: list[tuple[OddsRow, str]] = []
    for row in archive:
        pair = (row.home, row.away)
        label = f"{row.date} {row.home} v {row.away}"
        if pair in seen:
            problems.append(f"archive has {label} twice")
            continue
        seen.add(pair)
        if pair not in fixtures:
            problems.append(f"archive {label} is not a 2026-27 fixture (an unmapped name?)")
            continue
        played = fixtures[pair]
        if played is None:
            problems.append(f"archive {label} has a score, the fixture list none")
        elif (played.hs, played.as_) != (row.hs, row.as_):
            problems.append(
                f"{label}: archive {row.hs}-{row.as_}, fixture list {played.hs}-{played.as_}"
            )
        elif skew(played.date, row.date) > MAX_DATE_SKEW_DAYS:
            problems.append(f"{row.home} v {row.away}: archive {row.date}, fixture list {played.date}")
        else:
            if played.date != row.date:
                date_differences.append((row, played.date))
            rows.append(row._replace(date=played.date))
    if problems:
        raise RuntimeError("2026-27 archive odds and fixture list disagree:\n  " + "\n  ".join(problems))
    rows.sort(key=lambda r: (r.date, r.home, r.away))
    return rows, date_differences


def coverage_lines(rows: Sequence[OddsRow], history: Counter[int]) -> list[str]:
    """Per season: history matches, then complete odds of each market as
    `n/history (percent)`."""
    lines = []
    for season in sorted(history):
        total = history[season]
        season_rows = [r for r in rows if r.season == season]
        cells = []
        for name, columns in MARKETS.items():
            n = sum(r.complete(columns) for r in season_rows)
            cells.append(f"{name} {n}/{total} ({n / total:.1%})")
        lines.append(f"  {season_label(season)}: {total} matches; " + ", ".join(cells))
    return lines


def write_csv(rows: Sequence[OddsRow], path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", newline="", encoding="utf-8") as handle:
        writer = csv.writer(handle, lineterminator="\n")
        writer.writerow(HEADER)
        writer.writerows(row.cells() for row in rows)


def read_archive(path: Path | None) -> str:
    if path is None:
        body = fetch(ARCHIVE_URL)
        print(f"archive: {len(body):,} bytes from {ARCHIVE_URL}", file=sys.stderr)
    else:
        body = path.read_bytes()
        print(f"archive: {len(body):,} bytes from {path}", file=sys.stderr)
    if len(body) < MIN_ARCHIVE_BYTES:
        raise RuntimeError(
            f"archive is only {len(body):,} bytes (expected at least "
            f"{MIN_ARCHIVE_BYTES:,}); refusing a partial or replaced file"
        )
    return body.decode("utf-8-sig")


def main(argv: Sequence[str] | None = None) -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument(
        "--archive",
        type=Path,
        help="read this copy of the archive's Matches.csv instead of downloading it",
    )
    args = parser.parse_args(argv)

    text = read_archive(args.archive)
    seasons = range(FIRST_SEASON, LAST_SEASON + 1)
    archive = parse_archive_odds(text, seasons)
    digest = odds_digest(archive)
    print(
        f"archive odds slice {season_label(FIRST_SEASON)}..{season_label(LAST_SEASON)}: "
        f"{len(archive)} rows, sha256 {digest}",
        file=sys.stderr,
    )
    check_digest(digest, ODDS_SLICE_SHA256, "archive odds slice", "scripts/fetch_odds.py")
    result = reconcile_history(archive, history_matches(HISTORY, seasons))
    live, live_moved = reconcile_live(
        parse_archive_odds(text, range(LIVE_SEASON, LIVE_SEASON + 1), LIVE_ALIASES),
        fixture_results(FIXTURES),
    )

    for row, other in result.date_differences + live_moved:
        print(
            f"  date differs for {row.home} v {row.away}: archive {row.date}, "
            f"written {other}",
            file=sys.stderr,
        )
    print(
        f"reconciled with {HISTORY.relative_to(ROOT)}: {len(result.rows)} matches, "
        f"{result.walkovers} earthquake walkovers dropped, {len(result.missing)} "
        f"history matches without an archive row (all in the 2024-25 gap)",
        file=sys.stderr,
    )
    print("coverage (complete odds / history matches):", file=sys.stderr)
    for line in coverage_lines(result.rows, result.history):
        print(line, file=sys.stderr)
    n_live = sum(r.complete(MARKETS["b365 1x2"]) for r in live)
    print(
        f"  {season_label(LIVE_SEASON)} (live, not pinned): {len(live)} played matches, "
        f"{n_live} with Bet365 1X2",
        file=sys.stderr,
    )

    for rows, path in ((result.rows, OUT), (live, LIVE_OUT)):
        write_csv(rows, path)
        print(
            f"wrote {len(rows)} rows to {path.relative_to(ROOT)} "
            f"(sha256 {sha256(path.read_bytes())}; not committed: data/odds/ is ignored)",
            file=sys.stderr,
        )


if __name__ == "__main__":
    main()
