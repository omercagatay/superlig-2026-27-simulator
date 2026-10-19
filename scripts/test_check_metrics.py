"""Offline tests for check_metrics.py: standard library only, synthetic files.

    python3 -m unittest discover -s scripts -p 'test_*.py'

Every test writes its own metrics and docs into a temp directory, so none
depends on whether data/metrics/backtest.json has been committed yet.
"""
import contextlib, io, json, tempfile, unittest
from decimal import Decimal
from pathlib import Path

import check_metrics as cm

# The backtest's summary shape: five-decimal scores, four-decimal accuracy,
# integer counts, keys sorted.
METRICS = {
    "dev_1x2_ll": 0.99914,
    "dev_acc": 0.5123,
    "dev_n": 2448,
    "hold_1x2_ll": 0.97564,
    "hold_n": 647,
}


class TempDir(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.dir = Path(self._tmp.name)

    def tearDown(self):
        self._tmp.cleanup()

    def write(self, name, text):
        path = self.dir / name
        path.write_text(text, encoding="utf-8")
        return path

    def metrics_file(self, name="backtest.json", metrics=None, raw=None):
        body = raw if raw is not None else json.dumps(metrics or METRICS, sort_keys=True)
        return self.write(name, body + "\n")

    def run_main(self, argv):
        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            code = cm.main([str(a) for a in argv])
        return code, out.getvalue(), err.getvalue()


class LoadMetrics(TempDir):
    def test_keeps_the_digits_as_written(self):
        path = self.metrics_file(raw='{"a_ll": 0.99910, "a_n": 647}')
        got = cm.load_metrics(path)
        self.assertEqual(got, {"a_ll": Decimal("0.99910"), "a_n": 647})
        self.assertEqual(str(got["a_ll"]), "0.99910")
        self.assertIsInstance(got["a_n"], int)

    def test_a_missing_file_says_how_to_make_it(self):
        with self.assertRaises(cm.MetricsError) as e:
            cm.load_metrics(self.dir / "absent.json")
        self.assertIn("does not exist", str(e.exception))
        self.assertIn(cm.REGENERATE, str(e.exception))

    def test_rejects_what_the_backtest_never_writes(self):
        for raw in (
            "[0.5]",
            "{}",
            '{"a": "0.5"}',
            '{"a": true}',
            '{"a": [0.5]}',
            '{"a": NaN}',
            '{"a": Infinity}',
            '{"a": 0.5',
        ):
            with self.subTest(raw=raw), self.assertRaises(cm.MetricsError):
                cm.load_metrics(self.metrics_file(raw=raw))

    def test_null_is_a_figure_the_backtest_could_not_compute(self):
        # examples/backtest.rs writes a calibration fit that did not
        # converge (printed NaN) as null.
        path = self.metrics_file(raw='{"a_slope": null, "a_n": 647}')
        self.assertEqual(cm.load_metrics(path), {"a_slope": None, "a_n": 647})


class Diff(TempDir):
    def test_identical_metrics_have_no_rows(self):
        m = {"a": Decimal("0.99914"), "n": 647}
        self.assertEqual(cm.diff_metrics(m, dict(m)), [])

    def test_trailing_zeros_are_the_same_figure(self):
        self.assertEqual(
            cm.diff_metrics({"a": Decimal("0.9991")}, {"a": Decimal("0.99910")}), []
        )

    def test_one_digit_in_the_last_place_is_a_difference(self):
        rows = cm.diff_metrics(
            {"a": Decimal("0.99915"), "n": 650}, {"a": Decimal("0.99914"), "n": 647}
        )
        self.assertEqual(
            rows, [("a", "0.99914", "0.99915", "+0.00001"), ("n", "647", "650", "+3")]
        )

    def test_null_matches_only_null(self):
        self.assertEqual(cm.diff_metrics({"s": None}, {"s": None}), [])
        self.assertEqual(
            cm.diff_metrics({"s": None}, {"s": Decimal("0.981")}),
            [("s", "0.981", "null", "null")],
        )
        self.assertEqual(
            cm.diff_metrics({"s": Decimal("0.981")}, {"s": None}),
            [("s", "null", "0.981", "null")],
        )
        self.assertEqual(cm.diff_metrics({"s": None}, {}), [("s", "-", "null", "new")])

    def test_added_and_dropped_metrics_differ(self):
        rows = cm.diff_metrics({"a": 1, "new": Decimal("0.5")}, {"a": 1, "old": 2})
        self.assertEqual(rows, [("new", "-", "0.5", "new"), ("old", "2", "-", "missing")])

    def test_main_passes_on_a_match(self):
        fresh = self.metrics_file("fresh.json")
        committed = self.metrics_file("committed.json")
        code, out, err = self.run_main(["diff", fresh, committed])
        self.assertEqual(code, 0, err)
        self.assertIn("5 metrics match", out)

    def test_main_fails_with_a_table_and_the_command(self):
        fresh = self.metrics_file("fresh.json", {**METRICS, "hold_1x2_ll": 0.97601})
        committed = self.metrics_file("committed.json")
        code, _, err = self.run_main(["diff", fresh, committed])
        self.assertEqual(code, 1)
        self.assertIn("1 of 5 metrics differ", err)
        row = next(line for line in err.splitlines() if line.startswith("hold_1x2_ll"))
        self.assertEqual(row.split(), ["hold_1x2_ll", "0.97564", "0.97601", "+0.00037"])
        self.assertIn(cm.REGENERATE, err)
        self.assertIn("check_metrics.py docs", err)

    def test_main_fails_when_either_file_is_missing(self):
        present = self.metrics_file()
        absent = self.dir / "absent.json"
        code, _, err = self.run_main(["diff", absent, present])
        self.assertEqual(code, 1)
        self.assertIn("EVAL_METRICS=", err)
        code, _, err = self.run_main(["diff", present, absent])
        self.assertEqual(code, 1)
        self.assertIn(cm.REGENERATE, err)


class Format(unittest.TestCase):
    def test_without_a_spec_the_committed_text(self):
        self.assertEqual(cm.format_value(Decimal("0.97560"), None, None), "0.97560")
        self.assertEqual(cm.format_value(647, None, None), "647")

    def test_rounds_half_up_from_the_committed_digits(self):
        # Half-even would give 0.9992: it is how the printed figure reads.
        self.assertEqual(cm.format_value(Decimal("0.99925"), ".4f", None), "0.9993")
        self.assertEqual(cm.format_value(Decimal("0.99914"), ".4f", None), "0.9991")

    def test_percent_and_thousands(self):
        self.assertEqual(cm.format_value(Decimal("0.5234"), ".1%", None), "52.3%")
        self.assertEqual(cm.format_value(4588, ",d", None), "4,588")

    def test_a_count_written_with_a_point_is_still_a_count(self):
        self.assertEqual(cm.format_value(Decimal("4588.0"), ",d", None), "4,588")
        self.assertEqual(cm.format_value(Decimal("647"), "d", "tr"), "647")
        with self.assertRaises(ValueError):
            cm.format_value(Decimal("0.5"), "d", None)

    def test_turkish_notation(self):
        self.assertEqual(cm.format_value(Decimal("0.97564"), ".4f", "tr"), "0,9756")
        self.assertEqual(cm.format_value(Decimal("0.5234"), ".1%", "tr"), "%52,3")
        self.assertEqual(cm.format_value(Decimal("-0.015"), ".1%", "tr"), "-%1,5")
        self.assertEqual(cm.format_value(4588, ",d", "tr"), "4.588")
        self.assertEqual(cm.format_value(Decimal("0.97564"), None, "tr"), "0,97564")

    def test_without_a_spec_any_trailing_zeros_are_the_same_number(self):
        self.assertTrue(cm.shows("0.9756", Decimal("0.97560"), None, None))
        self.assertTrue(cm.shows("0.975600", Decimal("0.97560"), None, None))
        self.assertTrue(cm.shows("0,9756", Decimal("0.97560"), None, "tr"))
        self.assertFalse(cm.shows("0.9756", Decimal("0.97564"), None, None))
        self.assertFalse(cm.shows("n/a", Decimal("0.97564"), None, None))

    def test_with_a_spec_the_text_must_be_exact(self):
        self.assertTrue(cm.shows("0.9756", Decimal("0.97564"), ".4f", None))
        self.assertFalse(cm.shows("0.97560", Decimal("0.97564"), ".4f", None))


class Docs(TempDir):
    def check(self, text, metrics=None):
        doc = self.write("README.md", text)
        return cm.check_docs([doc], metrics or cm.load_metrics(self.metrics_file()))

    def test_correct_markers_pass(self):
        checked, errors = self.check(
            "Dev log-loss <!-- metric:dev_1x2_ll:.4f -->0.9991<!-- /metric --> over\n"
            "| holdout | <!-- metric:hold_1x2_ll -->0.97564<!-- /metric --> | "
            "<!-- metric:hold_n:,d -->647<!-- /metric --> |\n"
            "Doğruluk <!-- metric:dev_acc:.1%:tr -->%51,2<!-- /metric -->, "
            "<!--metric:dev_n:,d:tr-->2.448<!--/metric-->\n"
        )
        self.assertEqual(errors, [])
        self.assertEqual(checked, 5)

    def test_a_stale_figure_names_the_line_and_the_right_value(self):
        checked, errors = self.check(
            "intro\nholdout <!-- metric:hold_1x2_ll:.4f -->0.9759<!-- /metric -->\n"
        )
        self.assertEqual(checked, 1)
        self.assertEqual(len(errors), 1)
        self.assertIn("README.md:2", errors[0])
        self.assertIn("'0.9759'", errors[0])
        self.assertIn("'0.9756'", errors[0])

    def test_unknown_metrics_bad_specs_and_locales_fail(self):
        _, errors = self.check(
            "<!-- metric:hold_brier -->0.2<!-- /metric -->\n"
            "<!-- metric:hold_n:.4q -->647<!-- /metric -->\n"
            "<!-- metric:hold_n:d:de -->647<!-- /metric -->\n"
        )
        self.assertEqual(len(errors), 3, errors)
        self.assertIn("hold_brier is not a committed metric", errors[0])
        self.assertIn("format spec", errors[1])
        self.assertIn("unknown marker locale", errors[2])

    def test_a_null_metric_cannot_be_quoted(self):
        metrics = cm.load_metrics(self.metrics_file(raw='{"hold_slope": null}'))
        _, errors = self.check("<!-- metric:hold_slope -->0.98<!-- /metric -->\n", metrics)
        self.assertEqual(len(errors), 1)
        self.assertIn("hold_slope is null", errors[0])

    def test_a_marker_that_is_not_closed_on_its_line_fails(self):
        _, errors = self.check("<!-- metric:hold_n -->647\n<!-- /metric -->\n")
        self.assertEqual(len(errors), 1)
        self.assertIn("malformed", errors[0])

    def test_markers_in_code_are_documentation(self):
        checked, errors = self.check(
            "Write `<!-- metric:KEY:SPEC -->VALUE<!-- /metric -->` around a figure.\n"
            "```\n<!-- metric:KEY[:SPEC[:tr]] -->VALUE<!-- /metric -->\n```\n"
            "and <!-- metric:hold_n -->647<!-- /metric -->\n"
        )
        self.assertEqual(errors, [])
        self.assertEqual(checked, 1)

    def test_main_reports_every_problem(self):
        metrics = self.metrics_file()
        good = self.write("a.md", "<!-- metric:hold_n -->647<!-- /metric -->\n")
        bad = self.write("b.md", "<!-- metric:hold_n -->646<!-- /metric -->\n")
        code, out, err = self.run_main(["docs", "--metrics", metrics, good])
        self.assertEqual(code, 0, err)
        self.assertIn("1 metric marker(s)", out)
        code, _, err = self.run_main(["docs", "--metrics", metrics, good, bad])
        self.assertEqual(code, 1)
        self.assertIn("b.md:1", err)
        self.assertIn("1 problem(s) in 2 metric marker(s)", err)
        self.assertIn(cm.REGENERATE, err)

    def test_main_fails_when_there_is_nothing_to_check(self):
        # Markers deleted or never added would otherwise pass in silence.
        metrics = self.metrics_file()
        doc = self.write("a.md", "holdout 0.9756, `<!-- metric:hold_n -->647<!-- /metric -->`\n")
        code, _, err = self.run_main(["docs", "--metrics", metrics, doc])
        self.assertEqual(code, 1)
        self.assertIn("no metric markers in", err)

    def test_main_fails_clearly_before_the_metrics_exist(self):
        doc = self.write("a.md", "no markers yet\n")
        code, _, err = self.run_main(["docs", "--metrics", self.dir / "absent.json", doc])
        self.assertEqual(code, 1)
        self.assertIn("absent.json does not exist", err)
        self.assertIn(cm.REGENERATE, err)

    def test_a_missing_doc_fails(self):
        metrics = self.metrics_file()
        code, _, err = self.run_main(["docs", "--metrics", metrics, self.dir / "gone.md"])
        self.assertEqual(code, 1)
        self.assertIn("gone.md does not exist", err)

    def test_defaults_are_the_committed_file_and_the_three_docs(self):
        self.assertEqual(cm.COMMITTED, cm.ROOT / "data" / "metrics" / "backtest.json")
        self.assertEqual(
            [p.name for p in cm.DOCS], ["README.md", "README.tr.md", "CLAUDE.md"]
        )
        for doc in cm.DOCS:
            self.assertTrue(doc.is_file(), doc)


if __name__ == "__main__":
    unittest.main()
