#!/usr/bin/env python3
"""Build the chronological Süper Lig history used by the strength models.

Real match dates matter: Dixon-Coles applies recency weights and Pi ratings
are sequential, so assigning one nominal date to every match in a season
produces order-dependent, misleading ratings.

Sources:
  2012-13 .. 2024-25  xgabora's Club Football Match Data archive (MIT)
                      https://github.com/xgabora/Club-Football-Match-Data-2000-2025
  2024-25 gap only    openfootball football.json, 2024-25/tr.1.json (CC0 1.0)
                      https://github.com/openfootball/football.json
  2025-26             TFF's official week-by-week archive
                      https://www.tff.org/Default.aspx?pageId=1768

The archive (built from Football-Data.co.uk) is not complete for 2024-25: it
lacks matchdays 10-12 (25 October - 10 November 2024, 27 played matches) and
the awarded Galatasaray v Adana Demirspor.  openfootball lists all 342
fixtures of that season (four final-day entries carry no score; the archive
has those).  Every archive row of 2024-25 must agree with it (same ordered
pair and score, date within a day); only the pairs the archive lacks are
added, and administrative awards listed in EXCLUDED_AWARDS are not: no
football was played, so they must not move a strength rating.

Provenance is pinned.  The archive URL tracks a moving branch and the file
grows as seasons are added, so the script pins a SHA-256 of the slice it
actually uses (ARCHIVE_SLICE_SHA256), not of the whole file.  The openfootball
file is pinned whole (OPENFOOTBALL_SHA256).  A changed digest stops the run:
re-verify the upstream change before updating the constant, never just copy
the new value in.  Both digests are printed in the run log.

Usage:
  python3 scripts/fetch_history.py              # archive + openfootball + TFF
  python3 scripts/fetch_history.py --keep-tff   # reuse the committed 2025-26
                                                # rows instead of fetching TFF

--keep-tff regenerates everything except 2025-26 from the network and takes
that season's rows unchanged from the committed data/superlig_results.csv
(validated as on a fetch).  Use it when tff.org is unreachable.  The pure
helpers are tested offline by scripts/test_fetch_history.py.
"""

from __future__ import annotations

import argparse
import csv
import hashlib
import html
import io
import json
import re
import ssl
import sys
import time
import unicodedata
import urllib.request
from collections import Counter, defaultdict
from datetime import date
from pathlib import Path
from typing import Iterable, NamedTuple, Sequence


ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "data" / "superlig_results.csv"
INTERMEDIATE = ROOT / "data" / "tff_intermediate.pem"
HEADER = [
    "date",
    "home_team",
    "away_team",
    "home_score",
    "away_score",
    "tournament",
    "city",
    "country",
    "neutral",
]

ARCHIVE_URL = (
    "https://raw.githubusercontent.com/xgabora/"
    "Club-Football-Match-Data-2000-2025/main/data/Matches.csv"
)
# The upstream file only grows: on 2026-10-19 it was 45,370,018 bytes and
# already carried 2025-26 and part of 2026-27.  Anything much smaller is a
# truncated or replaced download.  The slice digest is the real guard.
MIN_ARCHIVE_BYTES = 40_000_000
# SHA-256 of the archive rows this history uses (seasons 2012-13..2024-25,
# after ARCHIVE_ALIASES, serialised by rows_digest).  Upstream may add
# seasons; it may not change these rows without someone re-verifying them.
ARCHIVE_SLICE_SHA256 = (
    "1cd10a3728ee3aa8701f4f4826bb4f8492564bd9b65fc2f9af59196975ed3eaf"
)

OPENFOOTBALL_URL = (
    "https://raw.githubusercontent.com/openfootball/"
    "football.json/master/2024-25/tr.1.json"
)
# The whole 2024-25 file as reconciled against the archive on 2026-10-19.
OPENFOOTBALL_SHA256 = (
    "6bfa0c16f87ea59ab79c60873ad3d3c0288e3198e4d6d0f56a2ebcb117e708c4"
)
SUPPLEMENT_SEASON = 2024
# The archive's 2024-25 gap: all of matchdays 10-12, nothing else.
EXPECTED_SUPPLEMENT_ROUNDS = frozenset({10, 11, 12})
EXPECTED_SUPPLEMENT_COUNT = 27
# Archive and openfootball dates may differ by a day (a late kick-off
# recorded on either side of midnight).  The archive's date wins: its rows
# are the pinned slice, and openfootball only fills the gap.  On 2026-10-19
# no 2024-25 date differed at all.  A wider gap is a different match.
MAX_DATE_SKEW_DAYS = 1

TFF_URL = "https://www.tff.org/Default.aspx?pageId=1768&hafta={}"
TFF_SEASON = 2025
UA = "superlig-sim/0.8 (educational project; data acquisition)"
FIRST_SEASON, LAST_ARCHIVE_SEASON = 2012, 2024

# Played-match totals of the archive itself.  For 2024-25 it has 314 of the
# 342 fixtures: matchdays 10-12 (27 played matches) are missing upstream, and
# so is the awarded Galatasaray v Adana Demirspor.  openfootball supplies the
# 27; the award stays out.  Every other season is complete.
EXPECTED_ARCHIVE_COUNTS = {
    **{year: 306 for year in range(2012, 2020)},
    2020: 420,
    2021: 380,
    2022: 342,
    2023: 380,
    2024: 314,
}

# Totals of the written history.  Every season is a full double round-robin
# except the awarded pairs in EXCLUDED_AWARDS.  The 2022-23 earthquake
# walkovers (Hatayspor, Gaziantep) are archive rows and stay in the file;
# src/history.rs drops them at load (is_unplayed_award).
EXPECTED_SEASON_COUNTS = {
    **EXPECTED_ARCHIVE_COUNTS,
    SUPPLEMENT_SEASON: EXPECTED_ARCHIVE_COUNTS[SUPPLEMENT_SEASON]
    + EXPECTED_SUPPLEMENT_COUNT,
    TFF_SEASON: 306,
}

# Archive names that need a canonical spelling: clubs in the current 2026-27
# league, and a club whose later seasons come from another source spelled
# differently (Karagümrük: archive 2020-21..2023-24, TFF 2025-26; unaliased,
# preseason Elo treated them as two clubs and the 2025-26 side restarted at
# NEW_CLUB_ELO).  Other historical clubs keep their source name and share the
# model's Other Club bucket.
#
# check_history fails on any two names that fold to the same letters
# (fold_name); Karagumruk / Karagümrük was the only such pair.  These look
# alike but are distinct clubs and stay apart:
#   Gaziantep (Gaziantep FK, formerly Gazişehir Gaziantep) and Gaziantepspor
#   (a different, now defunct club); Erciyesspor (Kayseri Erciyesspor) and
#   Kayserispor; Ad. Demirspor and Adanaspor; Osmanlispor (ex-Ankaraspor) and
#   Ankaragucu; and the near-spellings Alanyaspor / Antalyaspor / Adanaspor /
#   Hatayspor and Bodrumspor / Orduspor.
ARCHIVE_ALIASES = {
    "Besiktas": "Beşiktaş",
    "Buyuksehyr": "Başakşehir",
    "Erzurum BB": "Erzurumspor",
    "Eyupspor": "Eyüpspor",
    "Fenerbahce": "Fenerbahçe",
    "Genclerbirligi": "Gençlerbirliği",
    "Goztep": "Göztepe",
    "Karagumruk": "Karagümrük",
    "Kasimpasa": "Kasımpaşa",
}

# Every openfootball 2024-25 club name -> the name the history uses for it
# (the archive's, after ARCHIVE_ALIASES).  Identity entries are deliberate: a
# name missing here fails the run instead of creating a new club.
OPENFOOTBALL_ALIASES = {
    "Adana Demirspor": "Ad. Demirspor",
    "Alanyaspor": "Alanyaspor",
    "Antalyaspor": "Antalyaspor",
    "Beşiktaş": "Beşiktaş",
    "Bodrum FK": "Bodrumspor",
    "Çaykur Rizespor": "Rizespor",
    "Eyüpspor": "Eyüpspor",
    "Fenerbahçe": "Fenerbahçe",
    "Galatasaray": "Galatasaray",
    "Gaziantep FK": "Gaziantep",
    "Göztepe": "Göztepe",
    "Hatayspor": "Hatayspor",
    "İstanbul Başakşehir": "Başakşehir",
    "Kasımpaşa SK": "Kasımpaşa",
    "Kayserispor": "Kayserispor",
    "Konyaspor": "Konyaspor",
    "Samsunspor": "Samsunspor",
    "Sivasspor": "Sivasspor",
    "Trabzonspor": "Trabzonspor",
}


class Award(NamedTuple):
    date: str
    home: str
    away: str
    score: tuple[int, int]
    reason: str


# Administrative results that are not football and must not be added from
# openfootball.  Identified by date and ordered pair (history names), never by
# a heuristic such as a 3-0 score or a status flag.
EXCLUDED_AWARDS = (
    Award(
        "2025-02-09",
        "Galatasaray",
        "Ad. Demirspor",
        (3, 0),
        "Adana Demirspor left the pitch in the first half and did not return; "
        "the TFF awarded the match 3-0 to Galatasaray.",
    ),
)

# The official 2025-26 archive uses sponsored names.  Only the clubs that are
# also in the current model need canonical aliases; the rest remain historical.
TFF_ALIASES = {
    "CORENDON ALANYASPOR": "Alanyaspor",
    "HESAP.COM ANTALYASPOR": "Antalyaspor",
    "BEŞİKTAŞ A.Ş.": "Beşiktaş",
    "İKAS EYÜPSPOR": "Eyüpspor",
    "MISIRLI.COM.TR FATİH KARAGÜMRÜK": "Karagümrük",
    "FENERBAHÇE A.Ş.": "Fenerbahçe",
    "GALATASARAY A.Ş.": "Galatasaray",
    "GAZİANTEP FUTBOL KULÜBÜ A.Ş.": "Gaziantep",
    "GENÇLERBİRLİĞİ": "Gençlerbirliği",
    "NATURA DÜNYASI GENÇLERBİRLİĞİ": "Gençlerbirliği",
    "GÖZTEPE A.Ş.": "Göztepe",
    "KASIMPAŞA A.Ş.": "Kasımpaşa",
    "ZECORNER KAYSERİSPOR": "Kayserispor",
    "KOCAELİSPOR": "Kocaelispor",
    "TÜMOSAN KONYASPOR": "Konyaspor",
    "ÇAYKUR RİZESPOR A.Ş.": "Rizespor",
    "SAMSUNSPOR A.Ş.": "Samsunspor",
    "TRABZONSPOR A.Ş.": "Trabzonspor",
    "RAMS BAŞAKŞEHİR FUTBOL KULÜBÜ": "Başakşehir",
    "FATİH KARAGÜMRÜK A.Ş.": "Karagümrük",
}

EXPECTED_TFF_TEAMS = {
    "Alanyaspor",
    "Antalyaspor",
    "Başakşehir",
    "Beşiktaş",
    "Eyüpspor",
    "Fenerbahçe",
    "Galatasaray",
    "Gaziantep",
    "Gençlerbirliği",
    "Göztepe",
    "Karagümrük",
    "Kasımpaşa",
    "Kayserispor",
    "Kocaelispor",
    "Konyaspor",
    "Rizespor",
    "Samsunspor",
    "Trabzonspor",
}

TFF_ROW = re.compile(
    r'haftaninMaclariTarih">.*?lblTarih">(.*?)</span>'
    r'.*?haftaninMaclariEv">.*?<span[^>]*>(.*?)</span>'
    r'.*?haftaninMaclariSkor">.*?<span[^>]*>(\d+)</span>\s*-\s*'
    r'<span[^>]*>(\d+)</span>'
    r'.*?haftaninMaclariDeplasman">.*?<span[^>]*>(.*?)</span>',
    re.S,
)
TAG = re.compile(r"<[^>]+>")
ROUND = re.compile(r"Matchday (\d+)")
TURKISH_FOLD = str.maketrans(
    {"ı": "i", "İ": "i", "ş": "s", "Ş": "s", "ç": "c", "Ç": "c",
     "ğ": "g", "Ğ": "g", "ö": "o", "Ö": "o", "ü": "u", "Ü": "u"}
)


def clean(value: str) -> str:
    return " ".join(html.unescape(TAG.sub("", value)).split())


def fold_name(name: str) -> str:
    """Case, accents and punctuation removed: "Karagümrük" -> "karagumruk"."""
    decomposed = unicodedata.normalize("NFKD", name.translate(TURKISH_FOLD))
    plain = "".join(c for c in decomposed if not unicodedata.combining(c))
    return re.sub(r"[^0-9a-z]", "", plain.casefold())


def match_row(day: str, home: str, away: str, hs: int, as_: int) -> list[object]:
    return [day, home, away, hs, as_, "Super Lig", "", "Turkey", "False"]


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def rows_digest(rows: Iterable[Sequence[object]]) -> str:
    """SHA-256 of date, home, away and score of each row, in a canonical
    order and serialisation, so the same matches always give one digest."""
    keyed = sorted(tuple(str(value) for value in row[:5]) for row in rows)
    out = io.StringIO()
    csv.writer(out, lineterminator="\n").writerows(keyed)
    return sha256(out.getvalue().encode("utf-8"))


def fetch(url: str, *, tff: bool = False) -> bytes:
    request = urllib.request.Request(url, headers={"User-Agent": UA})
    context = None
    if tff:
        # TFF serves an incomplete TLS chain.  Add the pinned intermediate it
        # should send while keeping normal certificate verification enabled.
        context = ssl.create_default_context()
        context.load_verify_locations(cafile=str(INTERMEDIATE))
    with urllib.request.urlopen(request, timeout=45, context=context) as response:
        if response.status != 200:
            raise RuntimeError(f"{url}: HTTP {response.status}")
        return response.read()


def season_start(match_date: date) -> int:
    # Turkish seasons normally begin in August. The pandemic-delayed 2019-20
    # season continued through July 2020, so a July boundary misclassifies its
    # final 45 matches as part of 2020-21.
    return match_date.year if match_date.month >= 8 else match_date.year - 1


def season_label(season: int) -> str:
    return f"{season}-{(season + 1) % 100:02d}"


def check_counts(counts: dict[int, int], expected: dict[int, int], what: str) -> None:
    if dict(counts) != expected:
        raise RuntimeError(
            f"{what} season counts changed: {dict(sorted(counts.items()))}; "
            f"expected {expected}"
        )


def check_digest(got: str, pinned: str, what: str) -> None:
    if got != pinned:
        raise RuntimeError(
            f"{what} changed upstream: sha256 {got}, pinned {pinned}. "
            "Re-verify the upstream change (what moved, and why) before "
            "updating the pinned digest in scripts/fetch_history.py."
        )


def parse_archive(text: str) -> tuple[list[list[object]], Counter[int]]:
    """Süper Lig rows of seasons FIRST_SEASON..LAST_ARCHIVE_SEASON, aliased."""
    rows: list[list[object]] = []
    counts: Counter[int] = Counter()
    reader = csv.DictReader(io.StringIO(text))
    required = {"Division", "MatchDate", "HomeTeam", "AwayTeam", "FTHome", "FTAway"}
    if not reader.fieldnames or not required.issubset(reader.fieldnames):
        raise RuntimeError(f"archive columns changed: {reader.fieldnames}")

    for source in reader:
        if source["Division"] != "T1":
            continue
        match_date = date.fromisoformat(source["MatchDate"])
        season = season_start(match_date)
        if not FIRST_SEASON <= season <= LAST_ARCHIVE_SEASON:
            continue
        try:
            home_score = int(float(source["FTHome"]))
            away_score = int(float(source["FTAway"]))
        except (TypeError, ValueError) as exc:
            raise RuntimeError(f"invalid score in archive row: {source}") from exc
        if float(source["FTHome"]) != home_score or float(source["FTAway"]) != away_score:
            raise RuntimeError(f"non-integral score in archive row: {source}")

        home_team = ARCHIVE_ALIASES.get(source["HomeTeam"], source["HomeTeam"])
        away_team = ARCHIVE_ALIASES.get(source["AwayTeam"], source["AwayTeam"])
        rows.append(
            match_row(match_date.isoformat(), home_team, away_team, home_score, away_score)
        )
        counts[season] += 1
    return rows, counts


def archive_rows() -> list[list[object]]:
    body = fetch(ARCHIVE_URL)
    print(
        f"archive: {len(body):,} bytes, file sha256 {sha256(body)} "
        "(informational: the file grows upstream)",
        file=sys.stderr,
    )
    if len(body) < MIN_ARCHIVE_BYTES:
        raise RuntimeError(
            f"archive returned only {len(body):,} bytes (expected at least "
            f"{MIN_ARCHIVE_BYTES:,}); refusing a partial or replaced file"
        )
    rows, counts = parse_archive(body.decode("utf-8-sig"))
    check_counts(counts, EXPECTED_ARCHIVE_COUNTS, "archive")
    digest = rows_digest(rows)
    print(
        f"archive slice {season_label(FIRST_SEASON)}..{season_label(LAST_ARCHIVE_SEASON)}: "
        f"{len(rows)} played matches, sha256 {digest}",
        file=sys.stderr,
    )
    check_digest(digest, ARCHIVE_SLICE_SHA256, "archive slice")
    return rows


class OfMatch(NamedTuple):
    round: int
    date: str
    home: str
    away: str
    score: tuple[int, int] | None
    status: str | None


def parse_openfootball(body: bytes) -> list[OfMatch]:
    """openfootball fixtures with every club name mapped to its history name."""
    data = json.loads(body.decode("utf-8"))
    matches: list[OfMatch] = []
    unknown: set[str] = set()
    for entry in data["matches"]:
        names = []
        for side in ("team1", "team2"):
            name = entry[side]
            if name not in OPENFOOTBALL_ALIASES:
                unknown.add(name)
            names.append(OPENFOOTBALL_ALIASES.get(name, name))
        found = ROUND.fullmatch(entry["round"])
        if not found:
            raise RuntimeError(f"openfootball: unexpected round {entry['round']!r}")
        ft = entry.get("score", {}).get("ft")
        if ft is not None and (len(ft) != 2 or not all(isinstance(g, int) for g in ft)):
            raise RuntimeError(f"openfootball: malformed score in {entry}")
        matches.append(
            OfMatch(
                int(found.group(1)),
                date.fromisoformat(entry["date"]).isoformat(),
                names[0],
                names[1],
                None if ft is None else (ft[0], ft[1]),
                entry.get("status"),
            )
        )
    if unknown:
        raise RuntimeError(
            f"openfootball: unmapped club names {sorted(unknown)}; "
            "add them to OPENFOOTBALL_ALIASES"
        )
    return matches


class Reconciliation(NamedTuple):
    added: list[list[object]]
    excluded: list[Award]
    unscored: list[list[object]]
    date_differences: list[tuple[list[object], str]]


def reconcile(
    archive: Sequence[Sequence[object]],
    supplement: Sequence[OfMatch],
    season: int = SUPPLEMENT_SEASON,
    exclusions: Sequence[Award] = EXCLUDED_AWARDS,
) -> Reconciliation:
    """Check `archive`'s rows of `season` against `supplement` and return the
    supplement matches the archive lacks, minus the listed awards.

    Every archive row needs a supplement entry with the same ordered pair and
    score (an entry with no score cannot contradict it) and a date at most
    MAX_DATE_SKEW_DAYS away; the archive's date is kept.  Anything else, and
    any added match without a score or with a status (awarded, postponed...)
    that the exclusion list does not cover, fails."""
    by_pair: dict[tuple[str, str], OfMatch] = {}
    for match in supplement:
        pair = (match.home, match.away)
        if pair in by_pair:
            raise RuntimeError(f"supplement: duplicate fixture {pair[0]} v {pair[1]}")
        by_pair[pair] = match

    archive_pairs: set[tuple[str, str]] = set()
    unscored: list[list[object]] = []
    date_differences: list[tuple[list[object], str]] = []
    problems: list[str] = []
    for row in archive:
        if season_start(date.fromisoformat(str(row[0]))) != season:
            continue
        pair = (str(row[1]), str(row[2]))
        archive_pairs.add(pair)
        match = by_pair.get(pair)
        if match is None:
            problems.append(f"archive {row[0]} {pair[0]} v {pair[1]} is not in the supplement")
            continue
        if match.status is not None:
            problems.append(
                f"archive {row[0]} {pair[0]} v {pair[1]} is marked {match.status!r} upstream"
            )
        if match.score is None:
            unscored.append(list(row))
        elif match.score != (row[3], row[4]):
            problems.append(
                f"{pair[0]} v {pair[1]}: archive {row[3]}-{row[4]} on {row[0]}, "
                f"supplement {match.score[0]}-{match.score[1]} on {match.date}"
            )
        skew = abs((date.fromisoformat(match.date) - date.fromisoformat(str(row[0]))).days)
        if skew > MAX_DATE_SKEW_DAYS:
            problems.append(
                f"{pair[0]} v {pair[1]}: archive date {row[0]}, supplement {match.date}"
            )
        elif skew:
            date_differences.append((list(row), match.date))

    excluded_pairs: set[tuple[str, str]] = set()
    for award in exclusions:
        pair = (award.home, award.away)
        excluded_pairs.add(pair)
        match = by_pair.get(pair)
        if match is None or match.date != award.date or match.score != award.score:
            problems.append(f"excluded award not found as listed: {award}")
        if pair in archive_pairs:
            problems.append(f"excluded award {pair[0]} v {pair[1]} is in the archive")

    added: list[list[object]] = []
    for pair, match in by_pair.items():
        if season_start(date.fromisoformat(match.date)) != season:
            problems.append(f"supplement {match} is not in season {season_label(season)}")
        if pair in archive_pairs or pair in excluded_pairs:
            continue
        if match.status is not None:
            problems.append(
                f"{pair[0]} v {pair[1]} on {match.date} has status {match.status!r}; "
                "list it in EXCLUDED_AWARDS if it is not a played result"
            )
        elif match.score is None:
            problems.append(f"{pair[0]} v {pair[1]} on {match.date} has no score to add")
        else:
            added.append(match_row(match.date, pair[0], pair[1], *match.score))

    if problems:
        raise RuntimeError(
            f"{season_label(season)} archive and supplement disagree:\n  "
            + "\n  ".join(problems)
        )
    added.sort(key=lambda row: (row[0], row[1], row[2]))
    return Reconciliation(
        added,
        list(exclusions),
        sorted(unscored, key=lambda row: (row[0], row[1], row[2])),
        date_differences,
    )


def check_supplement(added: Sequence[Sequence[object]], supplement: Sequence[OfMatch]) -> None:
    """The archive's known gap, exactly: every match of the expected rounds."""
    expected = {
        (m.home, m.away) for m in supplement if m.round in EXPECTED_SUPPLEMENT_ROUNDS
    }
    got = {(str(row[1]), str(row[2])) for row in added}
    if got != expected or len(added) != EXPECTED_SUPPLEMENT_COUNT:
        raise RuntimeError(
            f"supplement adds {len(added)} matches, expected the "
            f"{EXPECTED_SUPPLEMENT_COUNT} of rounds {sorted(EXPECTED_SUPPLEMENT_ROUNDS)}; "
            f"unexpected {sorted(got - expected)}, missing {sorted(expected - got)}"
        )


def openfootball_rows(archive: Sequence[Sequence[object]]) -> list[list[object]]:
    body = fetch(OPENFOOTBALL_URL)
    digest = sha256(body)
    print(f"openfootball 2024-25: {len(body):,} bytes, sha256 {digest}", file=sys.stderr)
    check_digest(digest, OPENFOOTBALL_SHA256, "openfootball 2024-25/tr.1.json")
    supplement = parse_openfootball(body)
    result = reconcile(archive, supplement)
    check_supplement(result.added, supplement)
    for row in result.unscored:
        print(
            f"  openfootball has no score for {row[0]} {row[1]} v {row[2]}; "
            f"archive {row[3]}-{row[4]} kept",
            file=sys.stderr,
        )
    for row, other in result.date_differences:
        print(
            f"  date differs for {row[1]} v {row[2]}: archive {row[0]} (kept), "
            f"openfootball {other}",
            file=sys.stderr,
        )
    for award in result.excluded:
        print(
            f"  excluded award {award.date} {award.home} v {award.away} "
            f"{award.score[0]}-{award.score[1]}: {award.reason}",
            file=sys.stderr,
        )
    print(
        f"openfootball 2024-25: {len(supplement)} fixtures, archive rows all agree, "
        f"{len(result.added)} added",
        file=sys.stderr,
    )
    return result.added


def parse_tff_date(raw: str) -> date:
    # Examples: "08.08.2025 Cuma" and "17.05.2026 Pazar".
    value = clean(raw).split()[0]
    day, month, year = (int(part) for part in value.split("."))
    return date(year, month, day)


def check_tff_season(rows: Sequence[Sequence[object]]) -> None:
    pairs = {(row[1], row[2]) for row in rows}
    if len(rows) != 306 or len(pairs) != 306:
        raise RuntimeError(f"TFF 2025-26: got {len(rows)} rows / {len(pairs)} unique fixtures")
    teams = {team for pair in pairs for team in pair}
    if teams != EXPECTED_TFF_TEAMS:
        raise RuntimeError(
            "TFF 2025-26 club names changed: "
            f"missing {sorted(EXPECTED_TFF_TEAMS - teams)}, "
            f"unexpected {sorted(teams - EXPECTED_TFF_TEAMS)}"
        )


def tff_rows() -> list[list[object]]:
    rows: list[list[object]] = []
    seen_pairs: set[tuple[str, str]] = set()
    for week in range(1, 35):
        body = fetch(TFF_URL.format(week), tff=True)
        page = body.decode("windows-1254")
        matches = TFF_ROW.findall(page)
        if len(matches) != 9:
            raise RuntimeError(
                f"TFF 2025-26 week {week}: parsed {len(matches)} matches, expected 9"
            )
        for raw_date, raw_home, hs, as_, raw_away in matches:
            match_date = parse_tff_date(raw_date)
            home_source, away_source = clean(raw_home), clean(raw_away)
            home_team = TFF_ALIASES.get(home_source, home_source)
            away_team = TFF_ALIASES.get(away_source, away_source)
            pair = (home_team, away_team)
            if pair in seen_pairs:
                raise RuntimeError(f"TFF duplicate fixture: {home_team} v {away_team}")
            seen_pairs.add(pair)
            rows.append(match_row(match_date.isoformat(), home_team, away_team, int(hs), int(as_)))
        print(f"TFF 2025-26 week {week}: 9 matches", file=sys.stderr)
        time.sleep(0.2)

    check_tff_season(rows)
    return rows


def committed_tff_rows(path: Path) -> list[list[object]]:
    """The 2025-26 rows of a previously written history, unchanged."""
    with path.open(encoding="utf-8", newline="") as handle:
        reader = csv.reader(handle)
        header = next(reader, None)
        if header != HEADER:
            raise RuntimeError(f"{path}: unexpected header {header}")
        rows: list[list[object]] = []
        for record in reader:
            if season_start(date.fromisoformat(record[0])) != TFF_SEASON:
                continue
            day, home, away, hs, as_, *rest = record
            rows.append([day, home, away, int(hs), int(as_), *rest])
    check_tff_season(rows)
    print(f"TFF 2025-26: {len(rows)} matches kept from {path} (--keep-tff)", file=sys.stderr)
    return rows


def unplayed_pairs(
    exclusions: Sequence[Award] = EXCLUDED_AWARDS,
) -> dict[int, set[tuple[str, str]]]:
    out: dict[int, set[tuple[str, str]]] = defaultdict(set)
    for award in exclusions:
        out[season_start(date.fromisoformat(award.date))].add((award.home, award.away))
    return dict(out)


def check_history(
    rows: Sequence[Sequence[object]],
    expected_counts: dict[int, int] = EXPECTED_SEASON_COUNTS,
    unplayed: dict[int, set[tuple[str, str]]] | None = None,
) -> dict[int, Counter[str]]:
    """Season totals, one spelling per club, and a full double round-robin in
    every season except the unplayed (awarded) pairs.  Returns games per club
    per season."""
    if unplayed is None:
        unplayed = unplayed_pairs()
    pairs: dict[int, Counter[tuple[str, str]]] = defaultdict(Counter)
    for row in rows:
        pairs[season_start(date.fromisoformat(str(row[0])))][(str(row[1]), str(row[2]))] += 1
    check_counts(
        {season: sum(c.values()) for season, c in pairs.items()}, expected_counts, "history"
    )

    spellings: dict[str, set[str]] = defaultdict(set)
    for season_pairs in pairs.values():
        for pair in season_pairs:
            for name in pair:
                spellings[fold_name(name)].add(name)
    clashes = sorted(sorted(names) for names in spellings.values() if len(names) > 1)
    if clashes:
        raise RuntimeError(f"one club under several spellings: {clashes}; alias them")

    games: dict[int, Counter[str]] = {}
    problems: list[str] = []
    for season, season_pairs in sorted(pairs.items()):
        label = season_label(season)
        clubs = {name for pair in season_pairs for name in pair}
        repeated = sorted(pair for pair, n in season_pairs.items() if n > 1)
        if repeated:
            problems.append(f"{label}: fixtures recorded twice {repeated}")
        skipped = unplayed.get(season, set())
        expected = {(h, a) for h in clubs for a in clubs if h != a} - skipped
        present = set(season_pairs)
        if expected - present:
            problems.append(f"{label}: missing fixtures {sorted(expected - present)}")
        if present & skipped:
            problems.append(f"{label}: unplayed awards present {sorted(present & skipped)}")
        games[season] = Counter()
        for (home, away), n in season_pairs.items():
            games[season][home] += n
            games[season][away] += n
    if problems:
        raise RuntimeError(
            "history is not a set of double round-robins:\n  " + "\n  ".join(problems)
        )
    return games


def write_csv(rows: Sequence[Sequence[object]], path: Path) -> None:
    with path.open("w", newline="", encoding="utf-8") as handle:
        writer = csv.writer(handle, lineterminator="\n")
        writer.writerow(HEADER)
        writer.writerows(rows)


def main(argv: Sequence[str] | None = None) -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument(
        "--keep-tff",
        action="store_true",
        help="reuse the committed 2025-26 rows of data/superlig_results.csv "
        "instead of fetching them from tff.org",
    )
    args = parser.parse_args(argv)

    archive = archive_rows()
    supplement = openfootball_rows(archive)
    current = committed_tff_rows(OUT) if args.keep_tff else tff_rows()
    rows = archive + supplement + current
    rows.sort(key=lambda row: (row[0], row[1], row[2]))
    keys = [(row[0], row[1], row[2]) for row in rows]
    if len(keys) != len(set(keys)):
        raise RuntimeError("duplicate date/home/away rows in combined history")

    games = check_history(rows)
    for season, per_club in sorted(games.items()):
        spread = Counter(per_club.values())
        short = {club: n for club, n in sorted(per_club.items()) if n != max(spread)}
        print(
            f"  {season_label(season)}: {sum(per_club.values()) // 2} matches, "
            f"{len(per_club)} clubs, games per club {dict(sorted(spread.items()))}"
            + (f", short: {short}" if short else ""),
            file=sys.stderr,
        )

    write_csv(rows, OUT)
    print(
        f"wrote {len(rows)} chronological matches to {OUT} "
        f"(sha256 {sha256(OUT.read_bytes())})",
        file=sys.stderr,
    )


if __name__ == "__main__":
    main()
