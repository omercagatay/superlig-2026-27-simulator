"""Offline tests for fetch_odds.py: no network, standard library only.

    python3 -m unittest discover -s scripts -p 'test_*.py'

The odds are not committed, so nothing here can rebuild the pinned slice.
The helpers are tested on small inline samples, and main() is replayed end
to end against a synthetic archive, history and calendar in a temp
directory, with the pinned digest patched to the synthetic slice's.
"""
import csv, hashlib, io, json, shutil, tempfile, unittest
from datetime import date
from pathlib import Path
from unittest import mock

import fetch_odds as fo

GS, FB = "Galatasaray", "Fenerbahçe"
# Archive columns: fetch_odds reads these, and ignores the rest.
ARCHIVE_HEADER = ["Division", "MatchDate", "MatchTime", "HomeTeam", "AwayTeam", "FTHome",
                  "FTAway", *fo.ODDS_COLUMNS, "HandiSize"]
# Bet365 home/draw/away, best home/draw/away, Bet365 and best over/under.
PRICES = ("1.8", "3.6", "4.5", "1.9", "3.8", "4.8", "1.9", "1.9", "2.0", "2.0")


def archive_csv(rows):
    """Archive-shaped CSV text from (division, date, home, away, hs, as[,
    prices]) rows."""
    out = io.StringIO()
    w = csv.writer(out, lineterminator="\n")
    w.writerow(ARCHIVE_HEADER)
    for div, day, home, away, hs, as_, *prices in rows:
        w.writerow([div, day, "19:00:00", home, away, hs, as_, *(prices or PRICES), "-0.5"])
    return out.getvalue()


def odds_row(day, home, away, hs, as_, prices=PRICES, season=None):
    odds = {col: fo.odds_value(p, "test") for col, p in zip(fo.ODDS_COLUMNS.values(), prices)}
    return fo.OddsRow(season or fo.season_start(date.fromisoformat(day)), day, home, away,
                      hs, as_, odds)


def played(day, hs, as_):
    return fo.Played(day, hs, as_)


class Values(unittest.TestCase):
    def test_odds_are_normalised_prices_above_one(self):
        self.assertEqual(fo.odds_value("2", "x"), "2.0")
        self.assertEqual(fo.odds_value(" 2.00 ", "x"), "2.0")
        self.assertEqual(fo.odds_value("1.333", "x"), "1.333")
        self.assertEqual(fo.odds_value("", "x"), "")
        self.assertEqual(fo.odds_value(None, "x"), "")
        for bad in ("1.0", "0.9", "-2", "nan", "inf", "evens"):
            with self.subTest(bad=bad), self.assertRaisesRegex(RuntimeError, "x: odds"):
                fo.odds_value(bad, "x")

    def test_quake_walkovers_are_src_history_rs_rule(self):
        self.assertTrue(fo.is_quake_walkover(date(2023, 2, 6), "Hatayspor", GS))
        self.assertTrue(fo.is_quake_walkover(date(2023, 5, 1), FB, "Gaziantep"))
        self.assertFalse(fo.is_quake_walkover(date(2023, 2, 5), "Hatayspor", GS))
        self.assertFalse(fo.is_quake_walkover(date(2023, 8, 1), "Hatayspor", GS))
        self.assertFalse(fo.is_quake_walkover(date(2023, 3, 1), GS, FB))


class Parse(unittest.TestCase):
    def test_rows_of_the_seasons_aliased_with_their_odds(self):
        text = archive_csv([
            ("T1", "2016-08-19", "Besiktas", "Fenerbahce", "1.0", "2.0"),
            ("T2", "2016-08-19", "Altay", "Boluspor", "1", "0"),       # other division
            ("T1", "2016-07-31", GS, "Bursaspor", "1", "0"),           # 2015-16
            ("T1", "2026-08-14", GS, "Corum", "2", "2"),               # beyond the slice
        ])
        rows = fo.parse_archive_odds(text, range(2016, 2026))
        self.assertEqual(rows, [odds_row("2016-08-19", "Beşiktaş", FB, 1, 2)])
        self.assertEqual(rows[0].odds["b365_h"], "1.8")
        live = fo.parse_archive_odds(text, range(2026, 2027), fo.LIVE_ALIASES)
        self.assertEqual([(r.home, r.away) for r in live], [(GS, "Çorum")])

    def test_missing_prices_stay_blank(self):
        prices = ("", "3.6", "4.5", *PRICES[3:])
        text = archive_csv([("T1", "2017-09-01", GS, FB, "1", "0", *prices)])
        (row,) = fo.parse_archive_odds(text, range(2016, 2026))
        self.assertEqual(row.odds["b365_h"], "")
        self.assertFalse(row.complete(fo.MARKETS["b365 1x2"]))
        self.assertTrue(row.complete(fo.MARKETS["max 1x2"]))

    def test_changed_columns_fail(self):
        text = archive_csv([("T1", "2016-08-19", GS, FB, "1", "0")]).replace("MaxOver25", "MaxO25")
        with self.assertRaisesRegex(RuntimeError, "missing \\['MaxOver25'\\]"):
            fo.parse_archive_odds(text, range(2016, 2026))

    def test_bad_scores_and_prices_fail(self):
        for row in [("T1", "2016-08-19", GS, FB, "1.5", "0"),
                    ("T1", "2016-08-19", GS, FB, "", "0"),
                    ("T1", "2016-08-19", GS, FB, "nan", "0"),
                    ("T1", "2016-08-19", GS, FB, "1", "0", "0.8", *PRICES[1:])]:
            with self.subTest(row=row), self.assertRaises(RuntimeError):
                fo.parse_archive_odds(archive_csv([row]), range(2016, 2026))

    def test_a_book_that_is_not_a_price_fails(self):
        # Bet365 1X2 at 3.0 each: a book of exactly 1, no margin.
        fair = ("3.0", "3.0", "3.0", *PRICES[3:])
        # Over/under prices in two of the 1X2 columns: a book of 1.27.
        swapped = ("1.9", "1.9", "4.5", *PRICES[3:])
        # An over/under book of 1.4.
        wide = (*PRICES[:6], "1.4", "1.5", "2.0", "2.0")
        for prices, market in ((fair, "b365 1x2"), (swapped, "b365 1x2"), (wide, "b365 o/u")):
            text = archive_csv([("T1", "2016-08-19", GS, FB, "1", "0", *prices)])
            with self.subTest(prices=prices), self.assertRaisesRegex(RuntimeError, market):
                fo.parse_archive_odds(text, range(2016, 2026))
        # Near-equal over and under prices (synthetic; two archive rows carry
        # such a pair upstream): a Bet365 over/under book of 1.29, above any
        # 1X2 book's range but inside the over/under one, so the row passes.
        near_equal = (*PRICES[:6], "1.55", "1.55", "1.60", "1.66")
        (row,) = fo.parse_archive_odds(
            archive_csv([("T1", "2016-08-19", GS, FB, "1", "0", *near_equal)]), range(2016, 2026))
        self.assertEqual((row.odds["b365_over25"], row.odds["b365_under25"]), ("1.55", "1.55"))


class Digest(unittest.TestCase):
    rows = [odds_row("2016-08-19", GS, FB, 1, 0), odds_row("2016-08-20", FB, GS, 0, 0)]

    def test_canonical_serialisation_is_pinned(self):
        prices = ",".join(repr(float(p)) for p in PRICES)
        text = (f"2016-17,2016-08-19,{GS},{FB},1,0,{prices}\n"
                f"2016-17,2016-08-20,{FB},{GS},0,0,{prices}\n")
        self.assertEqual(fo.odds_digest(self.rows), hashlib.sha256(text.encode()).hexdigest())

    def test_order_independent_and_price_sensitive(self):
        self.assertEqual(fo.odds_digest(self.rows), fo.odds_digest(self.rows[::-1]))
        moved = odds_row("2016-08-20", FB, GS, 0, 0, ("1.85", *PRICES[1:]))
        self.assertNotEqual(fo.odds_digest(self.rows), fo.odds_digest([self.rows[0], moved]))

    def test_digest_mismatch_names_this_script(self):
        with self.assertRaisesRegex(RuntimeError, "Re-verify.*scripts/fetch_odds.py"):
            fo.check_digest("b" * 64, "a" * 64, "archive odds slice", "scripts/fetch_odds.py")


class HistoryMatches(unittest.TestCase):
    HEADER = "date,home_team,away_team,home_score,away_score,tournament,city,country,neutral\n"

    def history(self, *lines):
        tmp = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, tmp)
        path = tmp / "superlig_results.csv"
        path.write_text(self.HEADER + "".join(f"{l},Super Lig,,Turkey,False\n" for l in lines),
                        encoding="utf-8")
        return path

    def test_matches_by_season_and_ordered_pair(self):
        path = self.history(f"2016-08-19,{GS},{FB},1,0", f"2017-03-01,{FB},{GS},2,2",
                            f"2017-08-20,{GS},{FB},3,1", "2023-03-01,Hatayspor,Konyaspor,0,3")
        self.assertEqual(fo.history_matches(path, range(2016, 2023)), {
            2016: {(GS, FB): played("2016-08-19", 1, 0), (FB, GS): played("2017-03-01", 2, 2)},
            2017: {(GS, FB): played("2017-08-20", 3, 1)},
        })

    def test_a_pair_twice_in_a_season_fails(self):
        path = self.history(f"2016-08-19,{GS},{FB},1,0", f"2017-03-01,{GS},{FB},2,2")
        with self.assertRaisesRegex(RuntimeError, f"history: {GS} v {FB} twice in 2016-17"):
            fo.history_matches(path, range(2016, 2026))
        # The same pair in two seasons is two matches, not a duplicate.
        path = self.history(f"2016-08-19,{GS},{FB},1,0", f"2017-08-20,{GS},{FB},2,2")
        self.assertEqual(sorted(fo.history_matches(path, range(2016, 2026))), [2016, 2017])


class ReconcileHistory(unittest.TestCase):
    history = {2016: {(GS, FB): played("2016-08-19", 1, 0), (FB, GS): played("2017-03-01", 2, 2)}}

    def test_every_row_matches_and_takes_the_history_date(self):
        archive = [odds_row("2016-08-20", GS, FB, 1, 0), odds_row("2017-03-01", FB, GS, 2, 2)]
        result = fo.reconcile_history(archive, self.history)
        self.assertEqual([r.date for r in result.rows], ["2016-08-19", "2017-03-01"])
        self.assertEqual(result.date_differences, [(archive[0], "2016-08-19")])
        self.assertEqual((result.missing, result.walkovers), ([], 0))
        self.assertEqual(result.history, {2016: 2})

    def test_disagreements_fail(self):
        cases = {
            "archive 1-1, history 1-0": [odds_row("2016-08-19", GS, FB, 1, 1)],
            "archive 2016-08-21, history 2016-08-19": [odds_row("2016-08-21", GS, FB, 1, 0)],
            "not in the history": [odds_row("2016-08-19", GS, "Besiktas", 1, 0)],
            "twice": [odds_row("2016-08-19", GS, FB, 1, 0)] * 2,
        }
        for message, archive in cases.items():
            with self.subTest(message=message), self.assertRaisesRegex(RuntimeError, message):
                fo.reconcile_history(archive + [odds_row("2017-03-01", FB, GS, 2, 2)],
                                     self.history)

    def test_only_the_known_gap_may_lack_odds(self):
        archive = [odds_row("2016-08-19", GS, FB, 1, 0)]
        with self.assertRaisesRegex(RuntimeError, "2017-03-01 Fenerbahçe v Galatasaray has no archive row"):
            fo.reconcile_history(archive, self.history)
        gap = (date(2017, 2, 25), date(2017, 3, 5))
        result = fo.reconcile_history(archive, self.history, gap)
        self.assertEqual(result.missing, [(2016, "2017-03-01", FB, GS)])
        self.assertEqual(fo.ARCHIVE_GAP, (date(2024, 10, 25), date(2024, 11, 10)))

    def test_walkovers_are_dropped(self):
        archive = [odds_row("2016-08-19", GS, FB, 1, 0), odds_row("2017-03-01", FB, GS, 2, 2),
                   odds_row("2023-03-01", "Hatayspor", GS, 0, 3)]
        self.assertEqual(fo.reconcile_history(archive, self.history).walkovers, 1)


class ReconcileLive(unittest.TestCase):
    fixtures = {(GS, "Çorum"): played("2026-08-14", 2, 2), (FB, GS): None}

    def test_played_fixtures_match(self):
        result = fo.reconcile_live([odds_row("2026-08-15", GS, "Çorum", 2, 2)], self.fixtures)
        self.assertEqual([r.date for r in result.rows], ["2026-08-14"])
        self.assertEqual(len(result.date_differences), 1)
        self.assertEqual(result.unplayed, [])

    def test_a_fixture_the_calendar_has_not_played_is_skipped(self):
        # The archive can run ahead of the committed calendar until
        # fetch_fixtures.py runs: the row is skipped and reported, not fatal.
        ahead = odds_row("2026-09-01", FB, GS, 1, 0)
        result = fo.reconcile_live([ahead, odds_row("2026-08-14", GS, "Çorum", 2, 2)],
                                   self.fixtures)
        self.assertEqual([(r.home, r.away) for r in result.rows], [(GS, "Çorum")])
        self.assertEqual(result.unplayed, [ahead])

    def test_disagreements_fail(self):
        # Each alongside a row the calendar has not played: skipping that one
        # must not hide the disagreement.
        ahead = odds_row("2026-09-01", FB, GS, 1, 0)
        cases = {
            "fixture list 2-2": [odds_row("2026-08-14", GS, "Çorum", 2, 1), ahead],
            "not a 2026-27 fixture": [odds_row("2026-08-14", GS, "Corum", 2, 2), ahead],
            "archive 2026-08-17": [odds_row("2026-08-17", GS, "Çorum", 2, 2), ahead],
            "twice": [ahead, ahead],
        }
        for message, rows in cases.items():
            with self.subTest(message=message), self.assertRaisesRegex(RuntimeError, message):
                fo.reconcile_live(rows, self.fixtures)

    def test_every_accented_club_of_the_season_has_an_alias(self):
        # The archive spells clubs in plain ASCII.  A 2026-27 club whose name
        # is not ASCII, with no alias, would match no fixture.
        clubs = {name for (h, a) in fo.fixture_results(fo.FIXTURES) for name in (h, a)}
        self.assertEqual(len(clubs), 18)
        targets = set(fo.LIVE_ALIASES.values())
        for club in clubs:
            if not club.isascii():
                self.assertIn(club, targets, club)
        self.assertTrue(all(raw.isascii() for raw in fo.LIVE_ALIASES))


class Coverage(unittest.TestCase):
    def test_lines_count_complete_markets_against_the_history(self):
        prices = ("", *PRICES[1:])
        rows = [odds_row("2016-08-19", GS, FB, 1, 0), odds_row("2016-08-20", FB, GS, 0, 0, prices)]
        (line,) = fo.coverage_lines(rows, {2016: 4})
        self.assertEqual(
            line,
            "  2016-17: 4 matches; b365 1x2 1/4 (25.0%), max 1x2 2/4 (50.0%), "
            "b365 o/u 2/4 (50.0%), max o/u 2/4 (50.0%)",
        )


class EndToEnd(unittest.TestCase):
    """main() on a synthetic archive, history and calendar."""

    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.tmp)
        self.history = self.tmp / "superlig_results.csv"
        self.history.write_text(
            "date,home_team,away_team,home_score,away_score,tournament,city,country,neutral\n"
            f"2016-08-19,{GS},{FB},1,0,Super Lig,,Turkey,False\n"
            f"2017-03-01,{FB},{GS},2,2,Super Lig,,Turkey,False\n"
            f"2023-03-01,Hatayspor,{GS},0,3,Super Lig,,Turkey,False\n",
            encoding="utf-8",
        )
        self.fixtures = self.tmp / "fixtures.json"
        self.fixtures.write_text(json.dumps([
            {"round": 1, "home": GS, "away": "Çorum", "home_score": 2, "away_score": 2,
             "date": "2026-08-14", "kickoff": "21:30"},
            {"round": 2, "home": "Çorum", "away": GS, "home_score": None, "away_score": None,
             "date": "2026-08-22", "kickoff": None},
        ]), encoding="utf-8")
        self.archive = self.tmp / "Matches.csv"
        self.archive.write_text(archive_csv([
            ("T1", "2016-08-19", GS, "Fenerbahce", "1.0", "0.0"),
            ("T1", "2017-03-01", "Fenerbahce", GS, "2.0", "2.0"),
            ("T1", "2023-03-01", "Hatayspor", GS, "0.0", "3.0", *([""] * 10)),
            ("T1", "2026-08-14", GS, "Corum", "2.0", "2.0"),
            # Ahead of the committed calendar, which has not played it yet.
            ("T1", "2026-08-22", "Corum", GS, "0.0", "1.0"),
            ("T2", "2016-08-19", "Altay", "Boluspor", "1.0", "0.0"),
        ]), encoding="utf-8")
        slice_rows = fo.parse_archive_odds(self.archive.read_text(encoding="utf-8"),
                                           range(fo.FIRST_SEASON, fo.LAST_SEASON + 1))
        self.digest = fo.odds_digest(slice_rows)

    def run_main(self, digest=None):
        with mock.patch.object(fo, "HISTORY", self.history), \
                mock.patch.object(fo, "FIXTURES", self.fixtures), \
                mock.patch.object(fo, "ROOT", self.tmp), \
                mock.patch.object(fo, "OUT", self.tmp / "odds" / "superlig_odds.csv"), \
                mock.patch.object(fo, "LIVE_OUT", self.tmp / "odds" / "live.csv"), \
                mock.patch.object(fo, "MIN_ARCHIVE_BYTES", 0), \
                mock.patch.object(fo, "ODDS_SLICE_SHA256", digest or self.digest), \
                mock.patch("sys.stderr", io.StringIO()) as log:
            fo.main(["--archive", str(self.archive)])
        return log.getvalue()

    def test_writes_both_files_and_reports_coverage(self):
        log = self.run_main()
        with (self.tmp / "odds" / "superlig_odds.csv").open(encoding="utf-8") as handle:
            rows = list(csv.reader(handle))
        self.assertEqual(rows[0], fo.HEADER)
        self.assertEqual([r[:6] for r in rows[1:]],
                         [["2016-17", "2016-08-19", GS, FB, "1", "0"],
                          ["2016-17", "2017-03-01", FB, GS, "2", "2"]])
        self.assertEqual(rows[1][6:], [repr(float(p)) for p in PRICES])
        live = (self.tmp / "odds" / "live.csv").read_text(encoding="utf-8").splitlines()
        self.assertEqual(len(live), 2)
        self.assertEqual(live[1].split(",")[:6], ["2026-27", "2026-08-14", GS, "Çorum", "2", "2"])
        self.assertIn("1 earthquake walkovers dropped", log)
        self.assertIn("2016-17: 2 matches; b365 1x2 2/2 (100.0%)", log)
        self.assertIn("2026-27 (live, not pinned): 1 played matches, 1 with Bet365 1X2; "
                      "1 archive rows skipped", log)
        self.assertIn(f"skipped 2026-08-22 Çorum v {GS} (0-1): not played", log)

    def test_a_changed_slice_stops_the_run_before_writing(self):
        with self.assertRaisesRegex(RuntimeError, "archive odds slice changed upstream"):
            self.run_main("0" * 64)
        self.assertFalse((self.tmp / "odds").exists())

    def test_a_short_archive_is_refused(self):
        with mock.patch.object(fo, "MIN_ARCHIVE_BYTES", 10**9), \
                mock.patch("sys.stderr", io.StringIO()):
            with self.assertRaisesRegex(RuntimeError, "refusing a partial"):
                fo.read_archive(self.archive)


if __name__ == "__main__":
    unittest.main()
