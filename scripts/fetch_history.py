#!/usr/bin/env python3
"""Build the chronological Süper Lig history used by the strength models.

2012-13 through 2024-25 come from xgabora's MIT-licensed Club Football
Match Data archive.  The completed 2025-26 season comes from TFF's official
week-by-week archive.  Real match dates matter: Dixon-Coles applies recency
weights and Pi ratings are sequential, so assigning one nominal date to every
match in a season produces order-dependent, misleading ratings.

Sources:
  https://github.com/xgabora/Club-Football-Match-Data-2000-2025
  https://www.tff.org/Default.aspx?pageId=1768
"""

from __future__ import annotations

import csv
import html
import io
import re
import ssl
import sys
import time
import urllib.request
from collections import Counter
from datetime import date
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "data" / "superlig_results.csv"
INTERMEDIATE = ROOT / "data" / "tff_intermediate.pem"
ARCHIVE_URL = (
    "https://raw.githubusercontent.com/xgabora/"
    "Club-Football-Match-Data-2000-2025/main/data/Matches.csv"
)
TFF_URL = "https://www.tff.org/Default.aspx?pageId=1768&hafta={}"
UA = "superlig-sim/0.8 (educational project; data acquisition)"
FIRST_SEASON, LAST_ARCHIVE_SEASON = 2012, 2024

# The archive omits two administratively awarded fixtures in 2024-25.  That
# is intentional here: no football was played, so those rows should not move
# a strength rating.  All other included seasons have their played-match total.
EXPECTED_ARCHIVE_COUNTS = {
    **{year: 306 for year in range(2012, 2020)},
    2020: 420,
    2021: 380,
    2022: 342,
    2023: 380,
    2024: 314,
}

# Names used by the archive for clubs in the current 2026-27 league.  Other
# historical clubs keep their source name and share the model's Other Club
# bucket, so similarly named but distinct entities must not be merged here.
ARCHIVE_ALIASES = {
    "Besiktas": "Beşiktaş",
    "Buyuksehyr": "Başakşehir",
    "Erzurum BB": "Erzurumspor",
    "Eyupspor": "Eyüpspor",
    "Fenerbahce": "Fenerbahçe",
    "Genclerbirligi": "Gençlerbirliği",
    "Goztep": "Göztepe",
    "Kasimpasa": "Kasımpaşa",
}

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


def clean(value: str) -> str:
    return " ".join(html.unescape(TAG.sub("", value)).split())


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


def archive_rows() -> list[list[object]]:
    body = fetch(ARCHIVE_URL)
    if len(body) < 20_000_000:
        raise RuntimeError(
            f"archive returned only {len(body):,} bytes; refusing a partial file"
        )

    rows: list[list[object]] = []
    counts: Counter[int] = Counter()
    reader = csv.DictReader(io.StringIO(body.decode("utf-8-sig")))
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
            [
                match_date.isoformat(),
                home_team,
                away_team,
                home_score,
                away_score,
                "Super Lig",
                "",
                "Turkey",
                "False",
            ]
        )
        counts[season] += 1

    if dict(counts) != EXPECTED_ARCHIVE_COUNTS:
        raise RuntimeError(
            f"archive season counts changed: {dict(sorted(counts.items()))}; "
            f"expected {EXPECTED_ARCHIVE_COUNTS}"
        )
    print(f"archive: {len(rows)} played matches", file=sys.stderr)
    return rows


def parse_tff_date(raw: str) -> date:
    # Examples: "08.08.2025 Cuma" and "17.05.2026 Pazar".
    value = clean(raw).split()[0]
    day, month, year = (int(part) for part in value.split("."))
    return date(year, month, day)


def tff_rows() -> list[list[object]]:
    rows: list[list[object]] = []
    seen_pairs: set[tuple[str, str]] = set()
    seen_teams: set[str] = set()
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
            seen_teams.update(pair)
            rows.append(
                [
                    match_date.isoformat(),
                    home_team,
                    away_team,
                    int(hs),
                    int(as_),
                    "Super Lig",
                    "",
                    "Turkey",
                    "False",
                ]
            )
        print(f"TFF 2025-26 week {week}: 9 matches", file=sys.stderr)
        time.sleep(0.2)

    if len(rows) != 306 or len(seen_pairs) != 306:
        raise RuntimeError(
            f"TFF 2025-26: got {len(rows)} rows / {len(seen_pairs)} unique fixtures"
        )
    if seen_teams != EXPECTED_TFF_TEAMS:
        raise RuntimeError(
            "TFF 2025-26 club names changed: "
            f"missing {sorted(EXPECTED_TFF_TEAMS - seen_teams)}, "
            f"unexpected {sorted(seen_teams - EXPECTED_TFF_TEAMS)}"
        )
    return rows


def main() -> None:
    rows = archive_rows() + tff_rows()
    rows.sort(key=lambda row: (row[0], row[1], row[2]))
    keys = [(row[0], row[1], row[2]) for row in rows]
    if len(keys) != len(set(keys)):
        raise RuntimeError("duplicate date/home/away rows in combined history")

    with OUT.open("w", newline="", encoding="utf-8") as handle:
        writer = csv.writer(handle, lineterminator="\n")
        writer.writerow(
            [
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
        )
        writer.writerows(rows)
    print(f"wrote {len(rows)} chronological matches to {OUT}", file=sys.stderr)


if __name__ == "__main__":
    main()
