"""Offline tests for fetch_history.py: no network, standard library only.

    python3 -m unittest discover -s scripts -p 'test_*.py'

The pure helpers are tested on small synthetic data.  `main(["--keep-tff"])`
is replayed end to end against an archive file and an openfootball file
rebuilt from the committed history, with urlopen stubbed and the output
redirected to a temp file: the rebuilt archive must hash to the pinned slice
digest and the run must write the committed history byte for byte.
"""
import csv, hashlib, io, json, os, shutil, tempfile, unittest
from collections import Counter
from pathlib import Path
from unittest import mock

import fetch_history as fh

COMMITTED = fh.OUT
GS, ADS = "Galatasaray", "Ad. Demirspor"
# Matchdays 10-12 of 2024-25, the archive's gap: it has no row in this window.
GAP = ("2024-10-25", "2024-11-10")


def gap_round(day):
    """The 2024-25 matchday of a date in GAP, or 1 for any other date."""
    for rnd, last in ((10, "2024-10-28"), (11, "2024-11-04"), (12, GAP[1])):
        if GAP[0] <= day <= last:
            return rnd
    return 1


def archive_csv(rows):
    """Archive-shaped CSV text from (division, date, home, away, hs, as) rows."""
    out = io.StringIO()
    w = csv.writer(out, lineterminator="\n")
    w.writerow(["Division", "MatchDate", "MatchTime", "HomeTeam", "AwayTeam",
                "HomeElo", "FTHome", "FTAway", "FTResult"])
    for div, day, home, away, hs, as_ in rows:
        w.writerow([div, day, "19:00:00", home, away, "1500.0", hs, as_, "H"])
    return out.getvalue()


def row(day, home, away, hs, as_):
    return fh.match_row(day, home, away, hs, as_)


def of(rnd, day, home, away, score=(1, 0), status=None):
    return fh.OfMatch(rnd, day, home, away, score, status)


def of_json(entries):
    """openfootball-shaped bytes from (round, date, team1, team2, ft|None, status)."""
    matches = []
    for rnd, day, t1, t2, ft, status in entries:
        m = {"round": f"Matchday {rnd}", "date": day, "time": "19:00",
             "team1": t1, "team2": t2, "score": {} if ft is None else {"ft": list(ft)}}
        if status:
            m["status"] = status
        matches.append(m)
    return json.dumps({"name": "Turkish Süper Lig 2024/25", "matches": matches},
                      ensure_ascii=False, indent=2).encode("utf-8")


def round_robin(clubs, season=2024):
    day = f"{season}-09-01"
    return [row(day, h, a, 1, 0) for h in clubs for a in clubs if h != a]


class Names(unittest.TestCase):
    def test_archive_aliases_map_to_canonical_names(self):
        text = archive_csv([
            ("T1", "2021-01-10", "Karagumruk", "Besiktas", "1.0", "2.0"),
            ("T1", "2024-08-10", "Goztep", "Ad. Demirspor", "0", "0"),
            ("T2", "2021-01-10", "Karagumruk", "Altay", "1", "0"),       # other division
            ("T1", "2012-07-31", "Galatasaray", "Bursaspor", "1", "0"),  # 2011-12
            ("T1", "2025-08-08", "Galatasaray", "Gaziantep", "3", "0"),  # beyond the slice
        ])
        rows, counts = fh.parse_archive(text)
        self.assertEqual(rows, [row("2021-01-10", "Karagümrük", "Beşiktaş", 1, 2),
                                row("2024-08-10", "Göztepe", "Ad. Demirspor", 0, 0)])
        self.assertEqual(counts, Counter({2020: 1, 2024: 1}))

    def test_non_integral_or_missing_score_fails(self):
        for score in ("1.5", ""):
            with self.subTest(score=score), self.assertRaises(RuntimeError):
                fh.parse_archive(archive_csv([("T1", "2020-01-10", "A", "B", score, "0")]))

    def test_changed_columns_fail(self):
        with self.assertRaisesRegex(RuntimeError, "columns changed"):
            fh.parse_archive("Division,MatchDate,Home,Away\nT1,2020-01-10,A,B\n")

    def test_fold_name(self):
        self.assertEqual(fh.fold_name("Karagumruk"), "karagumruk")
        self.assertEqual(fh.fold_name("Karagümrük"), "karagumruk")
        self.assertEqual(fh.fold_name("İstanbul Başakşehir"), "istanbulbasaksehir")
        self.assertEqual(fh.fold_name("GENÇLERBİRLİĞİ"), fh.fold_name("Genclerbirligi"))
        self.assertNotEqual(fh.fold_name("Ad. Demirspor"), fh.fold_name("Adanaspor"))
        self.assertNotEqual(fh.fold_name("Gaziantep"), fh.fold_name("Gaziantepspor"))

    def test_no_archive_spelling_folds_onto_another_canonical_name(self):
        # The Karagumruk class of bug: a raw archive spelling that folds to a
        # canonical name must be aliased to exactly that name.
        canonical = set(fh.ARCHIVE_ALIASES.values()) | fh.EXPECTED_TFF_TEAMS
        for raw, target in fh.ARCHIVE_ALIASES.items():
            for name in canonical:
                if fh.fold_name(name) == fh.fold_name(raw):
                    self.assertEqual(target, name, raw)
        self.assertEqual(fh.ARCHIVE_ALIASES["Karagumruk"], "Karagümrük")

    def test_openfootball_names_map_to_history_names(self):
        body = of_json([(1, "2024-08-10", "Bodrum FK", "İstanbul Başakşehir", (0, 1), None),
                        (1, "2024-08-10", "Kasımpaşa SK", "Çaykur Rizespor", None, None)])
        got = fh.parse_openfootball(body)
        self.assertEqual(got, [of(1, "2024-08-10", "Bodrumspor", "Başakşehir", (0, 1)),
                               of(1, "2024-08-10", "Kasımpaşa", "Rizespor", None)])
        # Targets are history names, never a raw archive spelling.
        self.assertFalse(set(fh.OPENFOOTBALL_ALIASES.values()) & set(fh.ARCHIVE_ALIASES))

    def test_unknown_openfootball_club_fails(self):
        body = of_json([(1, "2024-08-10", "Galatasaray", "MKE Ankaragücü", (2, 0), None)])
        with self.assertRaisesRegex(RuntimeError, "MKE Ankaragücü"):
            fh.parse_openfootball(body)

    def test_unexpected_round_fails(self):
        body = of_json([(1, "2024-08-10", "Galatasaray", "Hatayspor", (2, 1), None)])
        body = body.replace(b"Matchday 1", b"Round 1")
        with self.assertRaisesRegex(RuntimeError, "round"):
            fh.parse_openfootball(body)


class Reconcile(unittest.TestCase):
    archive = [row("2024-08-10", GS, "Hatayspor", 2, 1),
               row("2024-08-11", "Sivasspor", "Trabzonspor", 0, 0),
               row("2023-09-01", GS, "Sivasspor", 5, 0)]   # another season: ignored

    def supplement(self, *extra):
        return [of(1, "2024-08-10", GS, "Hatayspor", (2, 1)),
                of(1, "2024-08-11", "Sivasspor", "Trabzonspor", (0, 0)), *extra]

    def test_missing_pair_is_added(self):
        result = fh.reconcile(self.archive, self.supplement(
            of(10, "2024-10-28", GS, "Beşiktaş", (2, 1))), exclusions=())
        self.assertEqual(result.added, [row("2024-10-28", GS, "Beşiktaş", 2, 1)])
        self.assertEqual((result.unscored, result.date_differences), ([], []))

    def test_conflicting_score_fails(self):
        supplement = [of(1, "2024-08-10", GS, "Hatayspor", (2, 2)),
                      of(1, "2024-08-11", "Sivasspor", "Trabzonspor", (0, 0))]
        with self.assertRaisesRegex(RuntimeError, "archive 2-1 on 2024-08-10, supplement 2-2"):
            fh.reconcile(self.archive, supplement, exclusions=())

    def test_listed_award_is_not_added(self):
        award = fh.Award("2025-02-09", GS, ADS, (3, 0), "walk-off")
        result = fh.reconcile(self.archive, self.supplement(
            of(23, "2025-02-09", GS, ADS, (3, 0), "awarded"),
            of(10, "2024-10-28", GS, "Beşiktaş", (2, 1))), exclusions=[award])
        self.assertEqual([r[1:3] for r in result.added], [[GS, "Beşiktaş"]])
        self.assertEqual(result.excluded, [award])

    def test_award_not_on_the_list_fails(self):
        with self.assertRaisesRegex(RuntimeError, "status 'awarded'.*EXCLUDED_AWARDS"):
            fh.reconcile(self.archive, self.supplement(
                of(23, "2025-02-09", GS, ADS, (3, 0), "awarded")), exclusions=())

    def test_listed_award_must_be_found_as_listed(self):
        award = fh.Award("2025-02-09", GS, ADS, (3, 0), "walk-off")
        for entry in ([], [of(23, "2025-02-10", GS, ADS, (3, 0), "awarded")],
                      [of(23, "2025-02-09", GS, ADS, (2, 0), "awarded")]):
            with self.subTest(entry=entry), self.assertRaisesRegex(RuntimeError, "not found"):
                fh.reconcile(self.archive, self.supplement(*entry), exclusions=[award])

    def test_listed_award_in_the_archive_fails(self):
        award = fh.Award("2024-08-10", GS, "Hatayspor", (2, 1), "test")
        with self.assertRaisesRegex(RuntimeError, "is in the archive"):
            fh.reconcile(self.archive, self.supplement(), exclusions=[award])

    def test_archive_row_marked_upstream_fails(self):
        supplement = [of(1, "2024-08-10", GS, "Hatayspor", (2, 1), "awarded"),
                      of(1, "2024-08-11", "Sivasspor", "Trabzonspor", (0, 0))]
        with self.assertRaisesRegex(RuntimeError, "marked 'awarded' upstream"):
            fh.reconcile(self.archive, supplement, exclusions=())

    def test_date_skew_of_a_day_is_reported_and_the_archive_date_kept(self):
        supplement = [of(1, "2024-08-11", GS, "Hatayspor", (2, 1)),
                      of(1, "2024-08-11", "Sivasspor", "Trabzonspor", (0, 0))]
        result = fh.reconcile(self.archive, supplement, exclusions=())
        self.assertEqual(result.date_differences, [(self.archive[0], "2024-08-11")])
        self.assertEqual(result.added, [])

    def test_date_skew_of_two_days_fails(self):
        supplement = [of(1, "2024-08-12", GS, "Hatayspor", (2, 1)),
                      of(1, "2024-08-11", "Sivasspor", "Trabzonspor", (0, 0))]
        with self.assertRaisesRegex(RuntimeError, "archive date 2024-08-10, supplement 2024-08-12"):
            fh.reconcile(self.archive, supplement, exclusions=())

    def test_archive_row_missing_from_supplement_fails(self):
        with self.assertRaisesRegex(RuntimeError, "Sivasspor v Trabzonspor is not in the supplement"):
            fh.reconcile(self.archive, self.supplement()[:1], exclusions=())

    def test_duplicate_supplement_fixture_fails(self):
        with self.assertRaisesRegex(RuntimeError, "duplicate"):
            fh.reconcile(self.archive, self.supplement(self.supplement()[0]), exclusions=())

    def test_unscored_entry_is_fine_for_an_archive_row_but_cannot_be_added(self):
        supplement = [of(38, "2024-08-10", GS, "Hatayspor", None),
                      of(1, "2024-08-11", "Sivasspor", "Trabzonspor", (0, 0))]
        result = fh.reconcile(self.archive, supplement, exclusions=())
        self.assertEqual(result.unscored, [self.archive[0]])
        with self.assertRaisesRegex(RuntimeError, "has no score to add"):
            fh.reconcile(self.archive, supplement + [of(10, "2024-10-28", GS, "Beşiktaş", None)],
                         exclusions=())

    def test_supplement_from_another_season_fails(self):
        with self.assertRaisesRegex(RuntimeError, "not in season 2024-25"):
            fh.reconcile(self.archive, self.supplement(
                of(1, "2025-08-10", GS, "Beşiktaş", (1, 1))), exclusions=())

    def test_check_supplement_wants_exactly_the_gap_rounds(self):
        gap = [of(r, "2024-10-28", f"H{r}{i}", f"A{r}{i}", (1, 0))
               for r in sorted(fh.EXPECTED_SUPPLEMENT_ROUNDS) for i in range(9)]
        other = of(13, "2024-11-20", "X", "Y", (1, 0))
        added = [row(m.date, m.home, m.away, *m.score) for m in gap]
        fh.check_supplement(added, gap + [other])
        with self.assertRaisesRegex(RuntimeError, "unexpected"):
            fh.check_supplement(added + [row(other.date, "X", "Y", 1, 0)], gap + [other])
        with self.assertRaisesRegex(RuntimeError, "missing"):
            fh.check_supplement(added[1:], gap + [other])


class Digest(unittest.TestCase):
    rows = [row("2024-08-10", GS, "Hatayspor", 2, 1), row("2024-08-11", "Göztepe", ADS, 0, 0)]

    def test_canonical_serialisation_is_pinned(self):
        text = "2024-08-10,Galatasaray,Hatayspor,2,1\n2024-08-11,Göztepe,Ad. Demirspor,0,0\n"
        self.assertEqual(fh.rows_digest(self.rows), hashlib.sha256(text.encode()).hexdigest())

    def test_order_independent_and_score_sensitive(self):
        self.assertEqual(fh.rows_digest(self.rows), fh.rows_digest(self.rows[::-1]))
        changed = [self.rows[0], row("2024-08-11", "Göztepe", ADS, 0, 1)]
        self.assertNotEqual(fh.rows_digest(self.rows), fh.rows_digest(changed))

    def test_slice_digest_ignores_upstream_growth_but_not_edits(self):
        base = [("T1", "2024-08-10", GS, "Hatayspor", "2", "1"),
                ("T1", "2024-08-11", "Goztep", ADS, "0", "0")]
        grown = base + [("T1", "2025-08-08", GS, "Gaziantep", "3", "0"),
                        ("T2", "2024-08-10", "Bandirmaspor", "Sakaryaspor", "1", "1")]
        digest = fh.rows_digest(fh.parse_archive(archive_csv(base))[0])
        self.assertEqual(digest, fh.rows_digest(self.rows))
        self.assertEqual(digest, fh.rows_digest(fh.parse_archive(archive_csv(grown))[0]))
        edited = [base[0], ("T1", "2024-08-11", "Goztep", ADS, "1", "0")]
        self.assertNotEqual(digest, fh.rows_digest(fh.parse_archive(archive_csv(edited))[0]))

    def test_digest_mismatch_says_to_reverify(self):
        fh.check_digest("a" * 64, "a" * 64, "x")
        with self.assertRaisesRegex(RuntimeError, "x changed upstream.*Re-verify"):
            fh.check_digest("b" * 64, "a" * 64, "x")


class Seasons(unittest.TestCase):
    clubs = [GS, "Beşiktaş", "Hatayspor", ADS]

    def test_expected_totals(self):
        self.assertEqual(fh.EXPECTED_ARCHIVE_COUNTS[2024], 314)
        self.assertEqual(fh.EXPECTED_SUPPLEMENT_COUNT, 27)
        self.assertEqual(fh.EXPECTED_SEASON_COUNTS[2024], 341)
        self.assertEqual(fh.EXPECTED_SEASON_COUNTS[2025], 306)
        self.assertEqual(set(fh.EXPECTED_SEASON_COUNTS), set(range(2012, 2026)))
        self.assertEqual(fh.unplayed_pairs(), {2024: {(GS, ADS)}})

    def test_check_counts(self):
        fh.check_counts(Counter({2024: 2}), {2024: 2}, "x")
        with self.assertRaisesRegex(RuntimeError, "x season counts changed"):
            fh.check_counts(Counter({2024: 2, 2025: 1}), {2024: 2}, "x")

    def test_full_double_round_robin_passes(self):
        games = fh.check_history(round_robin(self.clubs), {2024: 12}, {})
        self.assertEqual(games[2024], Counter({club: 6 for club in self.clubs}))

    def test_only_listed_awards_may_be_missing(self):
        rows = [r for r in round_robin(self.clubs) if (r[1], r[2]) != (GS, ADS)]
        with self.assertRaisesRegex(RuntimeError, "missing fixtures"):
            fh.check_history(rows, {2024: 11}, {})
        games = fh.check_history(rows, {2024: 11}, {2024: {(GS, ADS)}})
        self.assertEqual((games[2024][GS], games[2024][ADS], games[2024]["Hatayspor"]), (5, 5, 6))
        with self.assertRaisesRegex(RuntimeError, "unplayed awards present"):
            fh.check_history(round_robin(self.clubs), {2024: 12}, {2024: {(GS, ADS)}})

    def test_wrong_total_or_repeat_fails(self):
        rows = round_robin(self.clubs)
        with self.assertRaisesRegex(RuntimeError, "history season counts changed"):
            fh.check_history(rows, {2024: 13}, {})
        with self.assertRaisesRegex(RuntimeError, "recorded twice"):
            fh.check_history(rows + [row("2024-09-02", GS, ADS, 0, 0)], {2024: 13}, {})

    def test_two_spellings_of_one_club_fail(self):
        rows = round_robin(["Karagumruk", "Sivasspor"], 2022) + \
            round_robin(["Karagümrük", "Sivasspor"], 2025)
        with self.assertRaisesRegex(RuntimeError, "Karagumruk.*Karagümrük"):
            fh.check_history(rows, {2022: 2, 2025: 2}, {})

    def test_tff_season_check(self):
        teams = sorted(fh.EXPECTED_TFF_TEAMS)
        rows = round_robin(teams, 2025)
        fh.check_tff_season(rows)
        with self.assertRaisesRegex(RuntimeError, "305 rows"):
            fh.check_tff_season(rows[1:])
        renamed = [[r[0], "Fatih Karagümrük" if r[1] == "Karagümrük" else r[1], *r[2:]]
                   for r in rows]
        with self.assertRaisesRegex(RuntimeError, "club names changed"):
            fh.check_tff_season(renamed)


def committed_rows():
    with COMMITTED.open(encoding="utf-8", newline="") as handle:
        return [[r[0], r[1], r[2], int(r[3]), int(r[4]), *r[5:]]
                for r in list(csv.reader(handle))[1:]]


class Committed(unittest.TestCase):
    def test_committed_history_is_complete(self):
        rows = committed_rows()
        games = fh.check_history(rows)
        self.assertEqual(len(rows), sum(fh.EXPECTED_SEASON_COUNTS.values()))
        season = games[2024]
        self.assertEqual(sum(season.values()) // 2, 341)
        self.assertEqual(len(season), 19)
        self.assertEqual({c: n for c, n in season.items() if n != 36}, {GS: 35, ADS: 35})
        names = {name for r in rows for name in r[1:3]}
        self.assertIn("Karagümrük", names)
        self.assertNotIn("Karagumruk", names)

    def test_committed_2025_26_rows_are_kept_unchanged(self):
        with mock.patch("sys.stderr", io.StringIO()):
            kept = fh.committed_tff_rows(COMMITTED)
        expected = [r for r in committed_rows() if r[0] >= "2025-08-01"]
        self.assertEqual(kept, expected)


class Response:
    def __init__(self, body):
        self.status, self.body = 200, body

    def read(self):
        return self.body

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        return False


class EndToEnd(unittest.TestCase):
    """main(["--keep-tff"]) against sources rebuilt from the committed file."""

    def setUp(self):
        self.tmp = tempfile.mkdtemp()
        self.addCleanup(shutil.rmtree, self.tmp)
        self.out = Path(self.tmp) / "superlig_results.csv"
        shutil.copyfile(COMMITTED, self.out)
        rows = committed_rows()
        raw = {v: k for k, v in fh.ARCHIVE_ALIASES.items()}
        archive = [("T1", r[0], raw.get(r[1], r[1]), raw.get(r[2], r[2]), f"{r[3]}.0", f"{r[4]}.0")
                   for r in rows if r[0] < "2025-08-01" and not GAP[0] <= r[0] <= GAP[1]]
        # Upstream growth and other divisions are outside the pinned slice.
        archive += [("T1", r[0], raw.get(r[1], r[1]), raw.get(r[2], r[2]), r[3], r[4])
                    for r in rows if r[0] >= "2025-08-01"]
        archive.append(("T2", "2024-09-01", "Bandirmaspor", "Sakaryaspor", "1", "1"))
        self.archive = archive_csv(archive).encode("utf-8")

        team = {v: k for k, v in fh.OPENFOOTBALL_ALIASES.items()}
        entries = [(gap_round(r[0]), r[0], team[r[1]], team[r[2]],
                    (r[3], r[4]), None)
                   for r in rows if "2024-08-01" <= r[0] < "2025-08-01"]
        entries.append((23, "2025-02-09", "Galatasaray", "Adana Demirspor", (3, 0), "awarded"))
        self.openfootball = of_json(entries)

    def run_main(self, archive=None):
        bodies = {fh.ARCHIVE_URL: archive or self.archive, fh.OPENFOOTBALL_URL: self.openfootball}

        def urlopen(request, timeout=None, context=None):
            return Response(bodies[request.full_url])

        with mock.patch.object(fh, "OUT", self.out), \
                mock.patch.object(fh, "MIN_ARCHIVE_BYTES", 0), \
                mock.patch.object(fh, "OPENFOOTBALL_SHA256",
                                  hashlib.sha256(self.openfootball).hexdigest()), \
                mock.patch("urllib.request.urlopen", urlopen), \
                mock.patch("sys.stderr", io.StringIO()):
            fh.main(["--keep-tff"])

    def test_rebuilds_the_committed_history_byte_for_byte(self):
        self.run_main()
        self.assertEqual(self.out.read_bytes(), COMMITTED.read_bytes())

    def test_an_edited_archive_row_stops_the_run_before_writing(self):
        edited = self.archive.replace(b"2024-08-09,19:00:00,Galatasaray,Hatayspor,1500.0,2.0,1.0",
                                      b"2024-08-09,19:00:00,Galatasaray,Hatayspor,1500.0,2.0,2.0")
        self.assertNotEqual(edited, self.archive)
        before = self.out.read_bytes()
        with self.assertRaisesRegex(RuntimeError, "archive slice changed upstream"):
            self.run_main(edited)
        self.assertEqual(self.out.read_bytes(), before)

    def test_a_short_archive_is_refused(self):
        with mock.patch.object(fh, "MIN_ARCHIVE_BYTES", 10**9), \
                mock.patch("urllib.request.urlopen", lambda *a, **k: Response(self.archive)), \
                mock.patch("sys.stderr", io.StringIO()):
            with self.assertRaisesRegex(RuntimeError, "refusing a partial"):
                fh.archive_rows()


if __name__ == "__main__":
    unittest.main()
