#!/usr/bin/env python3
"""Build data/superlig_results.csv from Wikipedia Süper Lig season pages.

Wikipedia renders each season's results matrix as a {{#invoke:sports results}}
call, which is machine-parseable:

    | name_GAL      = [[Galatasaray S.K. (football)|Galatasaray]]
    | match_ADE_GAL = 1-5

`match_<HOME>_<AWAY>` gives home/away orientation directly. Output uses the
same CSV schema src/history.rs already parses, so no Rust changes are needed
to read it.
"""
import csv, re, sys, time, urllib.parse, urllib.request

FIRST_YEAR, LAST_YEAR = 2012, 2025  # 2012-13 .. 2025-26
UA = "superlig-sim/0.1 (educational project; data acquisition)"
MIN_BYTES = 20_000  # a rate-limited stub is ~2 KB and returns HTTP 200

# Season display name -> canonical name. Wikipedia's display names drift
# between seasons; every drift must be listed here or that club's history
# silently lands in the "Other Club" bucket.
ALIASES = {
    "İstanbul B.B.": "Başakşehir",
    "İstanbul Başakşehir": "Başakşehir",
    "Başakşehir": "Başakşehir",
    "Gazişehir Gaziantep": "Gaziantep",
    "Gaziantep F.K.": "Gaziantep",
    "Gaziantep": "Gaziantep",
    "Çaykur Rizespor": "Rizespor",
    "Rizespor": "Rizespor",
    "Amed S.F.K.": "Amedspor",
    "Amedspor": "Amedspor",
    "Çorum F.K.": "Çorum",
    "Çorum": "Çorum",
    "Büyükşehir Belediye Erzurumspor": "Erzurumspor",
    "BB Erzurumspor": "Erzurumspor",
    "Erzurum BB": "Erzurumspor",
    "Erzurumspor": "Erzurumspor",
    "Fatih Karagümrük": "Karagümrük",
}


def fetch(title):
    url = ("https://en.wikipedia.org/w/index.php?title="
           + urllib.parse.quote(title) + "&action=raw")
    req = urllib.request.Request(url, headers={"User-Agent": UA})
    with urllib.request.urlopen(req, timeout=30) as r:
        body = r.read()
    if len(body) < MIN_BYTES:
        raise SystemExit(
            f"{title}: got {len(body)} bytes, expected >= {MIN_BYTES}. "
            "Wikipedia is rate-limiting; increase the sleep and retry.")
    return body.decode("utf-8")


def parse(text):
    """Return (code -> display name, [(home_code, away_code, hg, ag)])."""
    names = {}
    for code, link in re.findall(r"\|\s*name_(\S+?)\s*=\s*\[\[(.+?)\]\]", text):
        names[code] = link.split("|")[-1].strip()
    matches = re.findall(
        r"\|\s*match_(\S+?)_(\S+?)\s*=\s*(\d+)\s*[–-]\s*(\d+)", text)
    return names, [(h, a, int(x), int(y)) for h, a, x, y in matches]


def canonical(name):
    return ALIASES.get(name, name)


def standings_from(names, matches):
    tbl = {}
    for h, a, x, y in matches:
        for code in (h, a):
            tbl.setdefault(code, {"pts": 0, "gf": 0, "ga": 0})
        tbl[h]["gf"] += x
        tbl[h]["ga"] += y
        tbl[a]["gf"] += y
        tbl[a]["ga"] += x
        if x > y:
            tbl[h]["pts"] += 3
        elif x == y:
            tbl[h]["pts"] += 1
            tbl[a]["pts"] += 1
        else:
            tbl[a]["pts"] += 3
    return tbl


def published_standings(text):
    """Parse the season's own league table for cross-checking."""
    out = {}
    for field in ("win", "draw", "loss", "gf", "ga"):
        for code, val in re.findall(rf"\|\s*{field}_(\S+?)\s*=\s*(\d+)", text):
            out.setdefault(code, {})[field] = int(val)
    return out


def norm(code):
    """The results matrix and the standings table on the same page use
    different codes for the same club (RIZ vs RIZ with a dotted capital I)."""
    return code.replace("İ", "I").replace("ı", "i").upper()


def main():
    rows, seen_names = [], set()
    for year in range(FIRST_YEAR, LAST_YEAR + 1):
        title = f"{year}–{str(year + 1)[2:]} Süper Lig"
        text = fetch(title)
        names, matches = parse(text)
        if not matches:
            raise SystemExit(f"{title}: parsed 0 matches")
        # Season-level nominal date: 1 January of the season's second year.
        # The DC fit uses a 1460-day half-life, so intra-season precision
        # does not matter.
        date = f"{year + 1}-01-01"
        for h, a, x, y in matches:
            hn, an = canonical(names.get(h, h)), canonical(names.get(a, a))
            seen_names.update((hn, an))
            rows.append([date, hn, an, x, y, "Super Lig", "", "Turkey", "False"])

        pub = {norm(k): v for k, v in published_standings(text).items()}
        calc = {norm(k): v for k, v in standings_from(names, matches).items()}
        for code, c in calc.items():
            p = pub.get(code)
            if not p or "gf" not in p:
                continue
            if p["gf"] != c["gf"] or p["ga"] != c["ga"]:
                print(f"  WARN {title} {code}: matrix gf/ga {c['gf']}/{c['ga']}"
                      f" != published {p['gf']}/{p['ga']}"
                      " (expected on withdrawal/forfeit seasons)",
                      file=sys.stderr)
        print(f"{title}: {len(matches)} matches", file=sys.stderr)
        time.sleep(2.5)

    with open("data/superlig_results.csv", "w", newline="", encoding="utf-8") as f:
        w = csv.writer(f)
        w.writerow(["date", "home_team", "away_team", "home_score",
                    "away_score", "tournament", "city", "country", "neutral"])
        w.writerows(rows)
    print(f"wrote {len(rows)} matches, {len(seen_names)} distinct clubs",
          file=sys.stderr)
    for n in sorted(seen_names):
        print("  " + n, file=sys.stderr)


if __name__ == "__main__":
    main()
