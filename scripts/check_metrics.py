#!/usr/bin/env python3
"""Numbers gate: the committed evaluation metrics, and the docs that quote
them, must agree with what the code produces.

data/metrics/backtest.json is the walk-forward backtest's summary
(examples/backtest.rs with EVAL_METRICS=<path>): one JSON object of
metric name to number, rounded to the precision the backtest prints.  A
figure the backtest could not compute (a calibration fit that did not
converge prints NaN) is null: it matches only another null, and no doc may
quote it.  data/metrics/season.json is the season evaluation's, in the same
format (examples/season_eval.rs with SEASON_METRICS=<path>).

Usage:
  python3 scripts/check_metrics.py diff FRESH [COMMITTED]
      FRESH was just written by the backtest (or the season evaluation);
      COMMITTED defaults to data/metrics/backtest.json (pass
      data/metrics/season.json for the season evaluation's).  Every metric
      must match exactly (both are rounded at the printed precision), and
      neither file may have a metric the other lacks.
  python3 scripts/check_metrics.py docs [--metrics PATH] [--season-metrics PATH] [FILE ...]
      Every metric marker in FILE (default README.md, README.tr.md and
      CLAUDE.md) must show the committed value, and there must be at least
      one: a check that finds nothing to check fails.  --metrics defaults to
      data/metrics/backtest.json, --season-metrics to
      data/metrics/season.json, which is read only when a marker quotes it.

A marker wraps the figure it guards, on one line, invisible once rendered:
  <!-- metric:KEY -->VALUE<!-- /metric -->
      VALUE is the committed number itself (0.97564 or 0.975640, not 0.9756).
  <!-- metric:KEY:SPEC -->VALUE<!-- /metric -->
      VALUE is exactly format(committed, SPEC), a Python format spec such as
      .4f, .1% or ,d.  Rounding is half-up from the committed digits, as a
      reader rounding the printed figure by hand would.
  <!-- metric:KEY:SPEC:tr -->VALUE<!-- /metric -->
      The same in Turkish notation, as README.tr.md writes numbers:
      decimal comma, thousands point, leading percent sign (0,9756, %52,3).
  <!-- metric:season:KEY[:SPEC[:tr]] -->VALUE<!-- /metric -->
      KEY from the season metrics instead of the backtest's.
Markers inside code (backtick spans or fenced blocks) are text, not markers,
so the syntax can be documented.

Standard library only; tested offline by scripts/test_check_metrics.py.
"""

from __future__ import annotations

import argparse
import decimal
import json
import re
import sys
from decimal import Decimal
from pathlib import Path
from typing import NamedTuple, Sequence, Union

ROOT = Path(__file__).resolve().parents[1]
COMMITTED = ROOT / "data" / "metrics" / "backtest.json"
SEASON_COMMITTED = ROOT / "data" / "metrics" / "season.json"
DOCS = [ROOT / "README.md", ROOT / "README.tr.md", ROOT / "CLAUDE.md"]
REGENERATE = (
    "EVAL_HOLDOUT=1 EVAL_METRICS=data/metrics/backtest.json "
    "cargo run --release --example backtest"
)
SEASON_REGENERATE = (
    "EVAL_HOLDOUT=1 SEASON_METRICS=data/metrics/season.json "
    "cargo run --release --example season_eval"
)
# A marker key in this namespace (metric:season:KEY) names KEY in the season
# metrics; a key without one names a backtest metric.
SEASON = "season"

Number = Union[int, Decimal]
# None is a metric the backtest wrote as null.
Value = Union[Number, None]

OPEN = re.compile(r"<!--\s*metric:")
CODE_SPAN = re.compile(r"(`+).+?\1")
FENCE = re.compile(r"^\s*(```|~~~)")
MARKER = re.compile(
    rf"<!--\s*metric:(?P<key>(?:{SEASON}:)?[A-Za-z0-9_]+)"
    r"(?::(?P<spec>[^:>\s]*))?(?::(?P<locale>[A-Za-z]+))?\s*-->"
    r"(?P<value>.*?)<!--\s*/metric\s*-->"
)
LOCALES = ("tr",)


class MetricsError(Exception):
    """A metrics file that is missing or not in the backtest's format."""


def writer_of(committed: Path) -> tuple[str, str]:
    """What writes `committed`, and the command that regenerates it: the
    season evaluation for a file named season.json, the backtest otherwise."""
    if committed.name == SEASON_COMMITTED.name:
        return "the season evaluation with SEASON_METRICS", SEASON_REGENERATE
    return "the backtest with EVAL_METRICS", REGENERATE


def shown(path: Path) -> str:
    """`path` relative to the repository when it is inside it."""
    try:
        return str(path.resolve().relative_to(ROOT))
    except ValueError:
        return str(path)


def load_metrics(path: Path, missing: str | None = None) -> dict[str, Value]:
    """Metric name to number, or None for a null.  Decimals keep the digits
    exactly as written, so a comparison is of the printed figures, not of
    binary floats.  `missing` explains a file that does not exist."""
    if not path.is_file():
        hint = missing or f"the backtest writes it with\n  {REGENERATE}"
        raise MetricsError(f"{shown(path)} does not exist; {hint}")

    def no_constant(name: str) -> None:
        raise MetricsError(f"{shown(path)}: {name} is not a metric value")

    try:
        data = json.loads(
            path.read_text(encoding="utf-8"),
            parse_float=Decimal,
            parse_constant=no_constant,
        )
    except json.JSONDecodeError as e:
        raise MetricsError(f"{shown(path)} is not valid JSON: {e}") from None
    if not isinstance(data, dict) or not data:
        raise MetricsError(f"{shown(path)} must be a non-empty JSON object of metrics")
    for key, value in data.items():
        if value is not None and (
            isinstance(value, bool) or not isinstance(value, (int, Decimal))
        ):
            raise MetricsError(f"{shown(path)}: {key} is {value!r}, not a number")
    return data


def cell(value: Value) -> str:
    return "null" if value is None else str(value)


def diff_metrics(
    fresh: dict[str, Value], committed: dict[str, Value]
) -> list[tuple[str, str, str, str]]:
    """(metric, committed, fresh, change) for every metric that differs."""
    rows = []
    for key in sorted(set(fresh) | set(committed)):
        if key not in committed:
            rows.append((key, "-", cell(fresh[key]), "new"))
        elif key not in fresh:
            rows.append((key, cell(committed[key]), "-", "missing"))
        else:
            old, new = committed[key], fresh[key]
            if old is None or new is None:
                if old is not new:
                    rows.append((key, cell(old), cell(new), "null"))
            elif old != new:
                rows.append((key, str(old), str(new), f"{Decimal(new) - Decimal(old):+f}"))
    return rows


def table(header: Sequence[str], rows: Sequence[Sequence[str]]) -> str:
    """Plain text table: first column left-aligned, the rest right-aligned."""
    widths = [max(len(r[i]) for r in [header, *rows]) for i in range(len(header))]
    lines = []
    for r in [header, *rows]:
        cells = [r[0].ljust(widths[0])] + [c.rjust(w) for c, w in zip(r[1:], widths[1:])]
        lines.append("  ".join(cells).rstrip())
    return "\n".join(lines)


TR_SEPARATORS = str.maketrans(".,", ",.")


def format_value(value: Number, spec: str | None, locale: str | None) -> str:
    """How a marker with `spec` and `locale` shows `value`."""
    if spec and spec[-1:] == "d" and isinstance(value, Decimal):
        # Decimal has no integer presentation; a count written as 647.0 is
        # still 647, but 0.5 is not a count.
        if value != value.to_integral_value():
            raise ValueError(f"{value} is not a whole number")
        value = int(value)
    if not spec:
        text = str(value)
    else:
        with decimal.localcontext() as ctx:
            ctx.rounding = decimal.ROUND_HALF_UP
            text = format(value, spec)
    if locale == "tr":
        text = text.translate(TR_SEPARATORS)
        if text.endswith("%"):
            sign = "-" if text.startswith("-") else ""
            text = f"{sign}%{text[len(sign):-1]}"
    return text


def shows(text: str, value: Number, spec: str | None, locale: str | None) -> bool:
    """Whether a marker's text is `value`: exactly as formatted with a spec,
    the same number (trailing zeros aside) without one."""
    if spec:
        return text == format_value(value, spec, locale)
    plain = text.translate(TR_SEPARATORS) if locale == "tr" else text
    try:
        return Decimal(plain) == value
    except decimal.InvalidOperation:
        return False


class Marker(NamedTuple):
    path: Path
    line: int
    key: str
    spec: str | None
    locale: str | None
    value: str


def find_markers(path: Path) -> list[Marker | str]:
    """Every marker in `path` in reading order, an error in place of each one
    that is malformed."""
    found: list[Marker | str] = []
    fenced = False
    text = path.read_text(encoding="utf-8")
    for number, line in enumerate(text.splitlines(), start=1):
        if FENCE.match(line):
            fenced = not fenced
            continue
        if fenced:
            continue
        code = [span.span() for span in CODE_SPAN.finditer(line)]
        for opening in OPEN.finditer(line):
            if any(start <= opening.start() < end for start, end in code):
                continue
            m = MARKER.match(line, opening.start())
            if m is None:
                found.append(
                    f"{shown(path)}:{number}: malformed metric marker; the form is "
                    "<!-- metric:KEY[:SPEC[:tr]] -->VALUE<!-- /metric --> on one line"
                )
            elif m["locale"] is not None and m["locale"] not in LOCALES:
                found.append(f"{shown(path)}:{number}: unknown marker locale {m['locale']!r}")
            else:
                found.append(
                    Marker(path, number, m["key"], m["spec"] or None, m["locale"], m["value"])
                )
    return found


def check_docs(paths: Sequence[Path], metrics: dict[str, Value]) -> tuple[int, list[str]]:
    """How many markers were checked, and one error per marker that is
    malformed, names an unknown or null metric or shows the wrong figure."""
    checked, errors = 0, []
    for path in paths:
        if not path.is_file():
            errors.append(f"{shown(path)} does not exist")
            continue
        for mk in find_markers(path):
            checked += 1
            if isinstance(mk, str):
                errors.append(mk)
                continue
            where = f"{shown(mk.path)}:{mk.line}"
            if mk.key not in metrics:
                errors.append(f"{where}: {mk.key} is not a committed metric")
                continue
            value = metrics[mk.key]
            if value is None:
                errors.append(f"{where}: {mk.key} is null in the committed metrics")
                continue
            try:
                expected = format_value(value, mk.spec, mk.locale)
            except ValueError as e:
                errors.append(f"{where}: format spec {mk.spec!r} for {mk.key}: {e}")
                continue
            if not shows(mk.value.strip(), value, mk.spec, mk.locale):
                how = f" ({mk.spec}{':' + mk.locale if mk.locale else ''})" if mk.spec else ""
                errors.append(
                    f"{where}: {mk.key} shows {mk.value.strip()!r}, the committed "
                    f"{value} gives {expected!r}{how}"
                )
    return checked, errors


def run_diff(fresh_path: Path, committed_path: Path) -> int:
    writer, command = writer_of(committed_path)
    try:
        fresh = load_metrics(fresh_path, f"run {writer}={fresh_path} first")
        committed = load_metrics(committed_path, f"it is written with\n  {command}")
    except MetricsError as e:
        print(f"error: {e}", file=sys.stderr)
        return 1
    rows = diff_metrics(fresh, committed)
    if not rows:
        print(f"{len(committed)} metrics match {shown(committed_path)}")
        return 0
    print(
        f"{len(rows)} of {len(set(fresh) | set(committed))} metrics differ from "
        f"{shown(committed_path)}:\n"
        + table(("metric", "committed", "fresh", "change"), rows)
        + "\n\nIf the change is intended, regenerate the committed file with\n"
        f"  {command}\n"
        "and update every figure the docs quote from it "
        "(python3 scripts/check_metrics.py docs lists the stale ones).",
        file=sys.stderr,
    )
    return 1


def quotes_season(paths: Sequence[Path]) -> bool:
    """Whether a marker in `paths` quotes the season metrics."""
    return any(
        isinstance(mk, Marker) and mk.key.startswith(f"{SEASON}:")
        for path in paths
        if path.is_file()
        for mk in find_markers(path)
    )


def run_docs(paths: Sequence[Path], metrics_path: Path, season_path: Path) -> int:
    sources = [metrics_path]
    try:
        metrics = load_metrics(metrics_path)
        if quotes_season(paths):
            sources.append(season_path)
            season = load_metrics(season_path, f"it is written with\n  {SEASON_REGENERATE}")
            metrics.update({f"{SEASON}:{key}": value for key, value in season.items()})
    except MetricsError as e:
        print(f"error: {e}", file=sys.stderr)
        return 1
    checked, errors = check_docs(paths, metrics)
    if not checked:
        errors.append(
            f"no metric markers in {', '.join(shown(p) for p in paths)}; wrap each "
            "figure the docs quote from the backtest as "
            "<!-- metric:KEY[:SPEC[:tr]] -->VALUE<!-- /metric -->"
        )
    if errors:
        print(
            "\n".join(errors)
            + f"\n\n{len(errors)} problem(s) in {checked} metric marker(s).  Quote the "
            f"figures in {shown(metrics_path)} ({SEASON}: markers: "
            f"{shown(season_path)}); if those are stale, regenerate them first with\n"
            f"  {REGENERATE}\n  {SEASON_REGENERATE}",
            file=sys.stderr,
        )
        return 1
    print(
        f"{checked} metric marker(s) in {', '.join(shown(p) for p in paths)} "
        f"match {' and '.join(shown(p) for p in sources)}"
    )
    return 0


def main(argv: Sequence[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = parser.add_subparsers(dest="mode", required=True)
    diff = sub.add_parser("diff", help="compare fresh metrics with the committed ones")
    diff.add_argument("fresh", type=Path)
    diff.add_argument("committed", type=Path, nargs="?", default=COMMITTED)
    docs = sub.add_parser("docs", help="check the figures the docs quote")
    docs.add_argument("--metrics", type=Path, default=COMMITTED)
    docs.add_argument("--season-metrics", type=Path, default=SEASON_COMMITTED)
    docs.add_argument("files", type=Path, nargs="*", default=DOCS)
    args = parser.parse_args(argv)
    if args.mode == "diff":
        return run_diff(args.fresh, args.committed)
    return run_docs(args.files, args.metrics, args.season_metrics)


if __name__ == "__main__":
    sys.exit(main())
