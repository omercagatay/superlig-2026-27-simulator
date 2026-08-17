#!/usr/bin/env python3
"""Build data/fixtures_2026_27.json from the official TFF fixture page.

The whole season is embedded in one page: 306 macId links grouped under
34 "N. Hafta" headings, as <td>home</td><td>score</td><td>away</td> rows.

Kick-off dates live in a *different* widget: the season fixture list carries
no dates, but `&hafta=N` renders that week's matches with them. So the script
makes 34 extra polite requests and merges the dates in by (home, away).

The page is served as windows-1254. Decoding it as UTF-8 mangles every
Turkish club name, so the encoding is pinned explicitly.

tff.org's TLS chain is also incomplete: it serves only the leaf and omits the
GlobalSign intermediate. Browsers paper over this by chasing the certificate's
AIA URL; urllib and curl do not, and fail with "unable to get local issuer
certificate". We do the same AIA fetch, pinned by SHA-256 so the plain-HTTP
download cannot be substituted. Certificate verification stays ON.
"""
import hashlib, html, json, re, ssl, sys, time, urllib.request

URL = "https://www.tff.org/default.aspx?pageID=198"
UA = "Mozilla/5.0 (X11; Linux x86_64)"

# From the leaf's Authority Information Access extension.
AIA_URL = "http://secure.globalsign.com/cacert/gsrsaovsslca2018.crt"
AIA_SHA256 = "b676ffa3179e8812093a1b5eafee876ae7a6aaf231078dad1bfb21cd2893764a"

TFF_NAMES = {
    "AMED SPORTİF FAALİYETLER": "Amedspor",
    "ARCA ÇORUM FK": "Çorum",
    "BEŞİKTAŞ A.Ş.": "Beşiktaş",
    "CORENDON ALANYASPOR": "Alanyaspor",
    "ERZURUMSPOR FK": "Erzurumspor",
    "EYÜPSPOR": "Eyüpspor",
    "FENERBAHÇE A.Ş.": "Fenerbahçe",
    "GALATASARAY A.Ş.": "Galatasaray",
    "GAZİANTEP FUTBOL KULÜBÜ A.Ş.": "Gaziantep",
    "GENÇLERBİRLİĞİ": "Gençlerbirliği",
    "GÖZTEPE A.Ş.": "Göztepe",
    "KASIMPAŞA A.Ş.": "Kasımpaşa",
    "KOCAELİSPOR": "Kocaelispor",
    "SAMSUNSPOR A.Ş.": "Samsunspor",
    "TRABZONSPOR A.Ş.": "Trabzonspor",
    "TÜMOSAN KONYASPOR": "Konyaspor",
    "ÇAYKUR RİZESPOR A.Ş.": "Rizespor",
    "İSTANBUL BAŞAKŞEHİR FK": "Başakşehir",
}

TOKEN = re.compile(
    r"(?P<wk>(\d+)\.\s*Hafta)"
    r"|(?P<m><td[^>]*class=\"altCizgi\"[^>]*>\s*<a[^>]*kulupId=\d+[^>]*>(.*?)</a>\s*</td>\s*"
    r"<td[^>]*>\s*<a[^>]*macId=(\d+)[^>]*>(.*?)</a>\s*</td>\s*"
    r"<td[^>]*>\s*<a[^>]*kulupId=\d+[^>]*>(.*?)</a>\s*</td>)",
    re.S)


# One row of the per-week widget: date, optional kick-off time, and the two
# clubs in home-away order.
WEEK_ROW = re.compile(
    r'haftaninMaclariTarih">.*?lblTarih">(.*?)</span>'
    r'.*?lblSaat"[^>]*>(.*?)</span>'
    r'.*?haftaninMaclariEv">.*?<span[^>]*>(.*?)</span>'
    r'.*?haftaninMaclariSkor">.*?(?:</span>)?'
    r'.*?kulupID=\d+"[^>]*>\s*<span[^>]*>(.*?)</span>',
    re.S)


def parse_week_dates(page):
    """[(home, away, iso_date, time_or_none)] for one `&hafta=N` page."""
    out = []
    for raw_date, raw_time, raw_home, raw_away in WEEK_ROW.findall(page):
        d = html.unescape(raw_date).strip()
        m = re.match(r"^(\d{2})\.(\d{2})\.(\d{4})$", d)
        if not m:
            continue
        iso = f"{m.group(3)}-{m.group(2)}-{m.group(1)}"
        t = html.unescape(raw_time).strip() or None
        out.append((canonical(raw_home), canonical(raw_away), iso, t))
    return out


def canonical(raw):
    name = html.unescape(raw).strip()
    if name not in TFF_NAMES:
        raise SystemExit(f"Unmapped TFF club name: {name!r} — add it to TFF_NAMES")
    return TFF_NAMES[name]


def tls_context():
    """System trust store plus the intermediate tff.org forgets to send."""
    req = urllib.request.Request(AIA_URL, headers={"User-Agent": UA})
    with urllib.request.urlopen(req, timeout=30) as r:
        der = r.read()
    got = hashlib.sha256(der).hexdigest()
    if got != AIA_SHA256:
        raise SystemExit(
            f"AIA certificate fingerprint mismatch: {got} != {AIA_SHA256}. "
            "Refusing to trust it. If GlobalSign legitimately rotated this "
            "intermediate, verify the new one out-of-band before updating.")
    ctx = ssl.create_default_context()
    ctx.load_verify_locations(cadata=ssl.DER_cert_to_PEM_cert(der))
    return ctx


def main():
    ctx = tls_context()

    def fetch(url):
        req = urllib.request.Request(url, headers={"User-Agent": UA})
        with urllib.request.urlopen(req, timeout=30, context=ctx) as r:
            return r.read().decode("windows-1254")

    text = fetch(URL)

    out, week = [], None
    for m in TOKEN.finditer(text):
        if m.group("wk"):
            week = int(m.group(2))
            continue
        home, away = canonical(m.group(4)), canonical(m.group(7))
        score = html.unescape(m.group(6)).strip()
        sm = re.match(r"^(\d+)\s*-\s*(\d+)$", score)
        out.append({
            "round": week,
            "home": home,
            "away": away,
            "home_score": int(sm.group(1)) if sm else None,
            "away_score": int(sm.group(2)) if sm else None,
        })

    # Kick-off dates come from the per-week widget, one request per matchday.
    dates = {}
    for week in range(1, 35):
        for home, away, iso, kickoff in parse_week_dates(fetch(f"{URL}&hafta={week}")):
            dates[(home, away)] = (iso, kickoff)
        time.sleep(0.6)
    for f in out:
        iso, kickoff = dates.get((f["home"], f["away"]), (None, None))
        f["date"] = iso
        f["kickoff"] = kickoff
    missing = [f for f in out if not f["date"]]
    assert not missing, f"{len(missing)} fixtures without a date, e.g. {missing[:3]}"

    # Structural validation: a complete double round-robin.
    assert len(out) == 306, f"expected 306 fixtures, got {len(out)}"
    rounds = {}
    for f in out:
        rounds.setdefault(f["round"], []).append(f)
    assert sorted(rounds) == list(range(1, 35)), f"rounds: {sorted(rounds)}"
    assert all(len(v) == 9 for v in rounds.values()), "every round has 9 fixtures"

    clubs = {c for f in out for c in (f["home"], f["away"])}
    assert len(clubs) == 18, f"expected 18 clubs, got {len(clubs)}"
    played_count = {c: 0 for c in clubs}
    ordered, unordered = set(), {}
    for f in out:
        played_count[f["home"]] += 1
        played_count[f["away"]] += 1
        key = (f["home"], f["away"])
        assert key not in ordered, f"duplicate ordered pair {key}"
        ordered.add(key)
        unordered[frozenset(key)] = unordered.get(frozenset(key), 0) + 1
    assert all(v == 34 for v in played_count.values()), "every club plays 34"
    assert len(unordered) == 153, f"expected 153 pairs, got {len(unordered)}"
    assert set(unordered.values()) == {2}, "every pair meets exactly twice"

    out.sort(key=lambda f: (f["round"], f["home"]))
    with open("data/fixtures_2026_27.json", "w", encoding="utf-8") as f:
        json.dump(out, f, ensure_ascii=False, indent=1)

    played = [f for f in out if f["home_score"] is not None]
    span = (min(f["date"] for f in out), max(f["date"] for f in out))
    print(f"306 fixtures, 34 rounds, {len(played)} played, {span[0]}..{span[1]}",
          file=sys.stderr)
    for f in played:
        print(f"  R{f['round']}  {f['home']} {f['home_score']}-"
              f"{f['away_score']} {f['away']}", file=sys.stderr)


if __name__ == "__main__":
    main()
