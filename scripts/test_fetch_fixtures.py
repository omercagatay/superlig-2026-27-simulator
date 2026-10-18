"""Offline tests for fetch_fixtures.py: no network, standard library only.

    python3 -m unittest discover -s scripts -p 'test_*.py'

The pure comparison helpers are tested on small synthetic fixtures; `main()`
is replayed end to end against TFF-shaped pages generated from the committed
calendar, with urlopen stubbed and the output redirected to a temp file.
"""
import copy, json, os, ssl, tempfile, unittest
from unittest import mock

import fetch_fixtures as ff

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
TFF = {v: k for k, v in ff.TFF_NAMES.items()}


def fx(home, away, hs=None, as_=None, date="2026-10-17", kickoff="20:00", rnd=9):
    return {"round": rnd, "home": home, "away": away, "home_score": hs,
            "away_score": as_, "date": date, "kickoff": kickoff}


def by_key(fixtures):
    return {(f["home"], f["away"]): f for f in fixtures}


class Helpers(unittest.TestCase):
    def test_undated_fixture_keeps_its_committed_date(self):
        previous = by_key([fx("Galatasaray", "Fenerbahçe", date="2026-10-17", kickoff="20:00")])
        scraped = [fx("Galatasaray", "Fenerbahçe", date=None, kickoff=None),
                   fx("Beşiktaş", "Trabzonspor", date="2026-10-18")]
        out, filled, missing = ff.fill_missing_dates(scraped, previous)
        self.assertEqual((out[0]["date"], out[0]["kickoff"]), ("2026-10-17", "20:00"))
        self.assertEqual([f["home"] for f in filled], ["Galatasaray"])
        self.assertEqual(missing, [])
        self.assertIsNone(scraped[0]["date"], "input is not mutated")
        self.assertEqual(out[1], scraped[1])

    def test_undated_fixture_without_committed_date_is_missing(self):
        previous = by_key([fx("Galatasaray", "Fenerbahçe", date=None)])
        scraped = [fx("Galatasaray", "Fenerbahçe", date=None),
                   fx("Beşiktaş", "Trabzonspor", date=None)]
        _, filled, missing = ff.fill_missing_dates(scraped, previous)
        self.assertEqual(filled, [])
        self.assertEqual(len(missing), 2)

    def test_result_guard(self):
        previous = by_key([fx("A", "B", 2, 1), fx("C", "D", 0, 0), fx("E", "F", 1, 1),
                           fx("G", "H"), fx("I", "J", 3, 0)])
        scraped = [fx("A", "B", 2, 1),   # unchanged
                   fx("C", "D", 1, 0),   # changed
                   fx("E", "F"),         # vanished
                   fx("G", "H", 1, 0)]   # newly played; I v J gone entirely
        lines = ff.check_results(previous, scraped, allow=True)
        self.assertEqual(lines, ["  R9  C v D  0-0 -> 1-0",
                                 "  R9  E v F  1-1 -> no result",
                                 "  R9  I v J  3-0 -> missing"])
        with self.assertRaises(SystemExit) as e:
            ff.check_results(previous, scraped, allow=False)
        self.assertIn("C v D  0-0 -> 1-0", str(e.exception))
        self.assertEqual(ff.check_results(by_key(scraped), scraped, allow=False), [])

    def test_changes_since(self):
        previous = by_key([fx("A", "B"), fx("C", "D", date="2026-10-17"),
                           fx("E", "F", kickoff=None), fx("G", "H", kickoff="19:00"),
                           fx("I", "J", 1, 0)])
        scraped = [fx("A", "B", 2, 2), fx("C", "D", date="2026-11-02"),
                   fx("E", "F", kickoff="21:30"), fx("G", "H", kickoff="20:00"),
                   fx("I", "J", 1, 0)]
        played, redated, kickoffs = ff.changes_since(previous, scraped)
        self.assertEqual([f["home"] for f in played], ["A"])
        self.assertEqual([(p["date"], f["date"]) for p, f in redated],
                         [("2026-10-17", "2026-11-02")])
        self.assertEqual([(p["kickoff"], f["kickoff"]) for p, f in kickoffs],
                         [(None, "21:30"), ("19:00", "20:00")])

    def test_week_page_skips_rows_without_a_date(self):
        rows = [fx("Galatasaray", "Fenerbahçe"), fx("Beşiktaş", "Trabzonspor", kickoff=None)]
        page = week_page(rows, 9, undated={("Galatasaray", "Fenerbahçe")})
        self.assertEqual(ff.parse_week_dates(page),
                         [("Beşiktaş", "Trabzonspor", "2026-10-17", None)])


def season_page(fixtures):
    """The pageID=198 season list: "N. Hafta" headings, then score rows."""
    parts = []
    for rnd in sorted({f["round"] for f in fixtures}):
        parts.append(f"<h3>{rnd}. Hafta</h3><table>")
        for i, f in enumerate(f for f in fixtures if f["round"] == rnd):
            score = "-" if f["home_score"] is None else f"{f['home_score']} - {f['away_score']}"
            parts.append(
                f'<tr><td class="altCizgi"><a href="x?kulupId=1">{TFF[f["home"]]}</a></td>'
                f'<td><a href="x?macId={rnd * 10 + i}">{score}</a></td>'
                f'<td><a href="x?kulupId=2">{TFF[f["away"]]}</a></td></tr>')
        parts.append("</table>")
    return "".join(parts)


def week_page(fixtures, rnd, undated=()):
    """One `&hafta=N` widget; `undated` rows show "Ertelendi" for a date."""
    rows = []
    for f in fixtures:
        if f["round"] != rnd:
            continue
        y, m, d = f["date"].split("-")
        date = "Ertelendi" if (f["home"], f["away"]) in undated else f"{d}.{m}.{y}"
        rows.append(
            f'<div class="haftaninMaclariTarih"><span id="w_lblTarih">{date}</span>'
            f'<span id="w_lblSaat">{f["kickoff"] or ""}</span></div>'
            f'<div class="haftaninMaclariEv"><a href="x?kulupID=1"><span>{TFF[f["home"]]}</span></a></div>'
            f'<div class="haftaninMaclariSkor"><span>-</span></div>'
            f'<div><a href="x?kulupID=2"><span>{TFF[f["away"]]}</span></a></div>')
    return "".join(rows)


class Response:
    def __init__(self, body):
        self.body = body

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        return False

    def read(self):
        return self.body


class EndToEnd(unittest.TestCase):
    """Replay main() against pages built from the committed calendar."""

    @classmethod
    def setUpClass(cls):
        with open(os.path.join(ROOT, ff.OUT), encoding="utf-8") as f:
            cls.base = json.load(f)
        # Pin the shape the guards need, whatever matchday the data is at.
        cls.base[0].update(home_score=2, away_score=1)
        cls.base[-1].update(home_score=None, away_score=None)
        with open(os.path.join(ROOT, "data", "tff_intermediate.pem")) as f:
            cls.aia_der = ssl.PEM_cert_to_DER_cert(f.read())

    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.out = os.path.join(tmp.name, "fixtures.json")

    def written(self):
        with open(self.out, encoding="utf-8") as f:
            return json.load(f)

    def run_main(self, previous, scraped, undated=(), allow=""):
        """main() with `previous` committed; returns what it wrote."""
        if previous is not None:
            with open(self.out, "w", encoding="utf-8") as f:
                json.dump(previous, f, ensure_ascii=False)

        def urlopen(req, *args, **kwargs):
            url = req.full_url
            if url == ff.AIA_URL:
                return Response(self.aia_der)
            if url == ff.URL:
                return Response(season_page(scraped).encode("windows-1254"))
            rnd = int(url.rsplit("&hafta=", 1)[1])
            return Response(week_page(scraped, rnd, undated).encode("windows-1254"))

        with mock.patch.object(ff, "OUT", self.out), \
                mock.patch("urllib.request.urlopen", urlopen), \
                mock.patch("time.sleep"), \
                mock.patch.dict(os.environ, {"ALLOW_RESULT_CHANGES": allow}), \
                mock.patch("sys.stderr"):
            ff.main()
        return self.written()

    def expected(self, fixtures):
        return sorted(fixtures, key=lambda f: (f["round"], f["home"]))

    def test_unchanged_replay_reproduces_the_file(self):
        self.assertEqual(self.run_main(self.base, self.base), self.expected(self.base))

    def test_postponed_rows_keep_their_committed_date(self):
        undated = {(f["home"], f["away"]) for f in (self.base[0], self.base[-1])}
        got = self.run_main(self.base, self.base, undated=undated)
        self.assertEqual(got, self.expected(self.base))

    def test_postponed_row_with_no_committed_date_fails(self):
        undated = {(self.base[-1]["home"], self.base[-1]["away"])}
        with self.assertRaises(AssertionError):
            self.run_main(None, self.base, undated=undated)

    def test_changed_or_vanished_result_fails_without_override(self):
        for hs, as_ in ((1, 1), (None, None)):
            scraped = copy.deepcopy(self.base)
            scraped[0].update(home_score=hs, away_score=as_)
            with self.assertRaises(SystemExit):
                self.run_main(self.base, scraped)
            self.assertEqual(self.written(), self.base, "committed file left alone")

    def test_override_accepts_a_correction(self):
        scraped = copy.deepcopy(self.base)
        scraped[0].update(home_score=3, away_score=0)
        got = self.run_main(self.base, scraped, allow="1")
        self.assertEqual(got, self.expected(scraped))


if __name__ == "__main__":
    unittest.main()
