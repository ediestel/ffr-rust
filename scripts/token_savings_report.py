#!/usr/bin/env python3
"""Token-savings report for ffr's bounded reads.

Compares bytes actually served by ffr tools (from ~/.local/share/ffr/reads.log)
against a naive baseline: what a plain whole-file Read would have returned for
the same call, capped at 2000 lines (the built-in Read tool's default window).

Honesty rules, chosen to UNDER-state rather than over-state savings:
  * classify_file is counted as pure overhead (baseline 0) — a naive agent
    would not have made that call at all.
  * Calls whose file no longer exists are counted neutral (baseline = served).
  * Files are stat'd as they are TODAY; a file that grew since the call
    inflates the baseline, one that shrank deflates it. Roughly a wash.
  * Error calls are counted as overhead (served bytes, baseline 0).

Usage:
  python3 token_savings_report.py [--log PATH] [--since YYYY-MM-DD]
                                  [--project SUBSTRING] [--per-project]
"""

import argparse
import datetime as dt
import os
import sys
from collections import defaultdict

LOG_DEFAULT = os.path.expanduser("~/.local/share/ffr/reads.log")
READ_CAP_LINES = 2000  # built-in Read default window
BYTES_PER_TOKEN = 4.0  # rough average for source code

# Tools where the naive equivalent is "read the whole file (capped)".
WHOLE_FILE_BASELINE = {
    "read_file",
    "read_chunk",
    "read_range_around_line",
    "search_in_file",
    "outline",
    "extract_pdf_text",
}
# Tools a naive agent would not have called: overhead, baseline 0.
OVERHEAD_TOOLS = {"classify_file", "stat_file", "build_line_index", "list_archive", "diff_files"}


class FileInfo:
    """Cached stat + line count for one path."""

    __slots__ = ("size", "capped_bytes")

    def __init__(self, path):
        try:
            self.size = os.path.getsize(path)
        except OSError:
            self.size = None
            self.capped_bytes = None
            return
        # Bytes of the first READ_CAP_LINES lines (what built-in Read serves).
        if self.size == 0:
            self.capped_bytes = 0
            return
        try:
            n = 0
            with open(path, "rb") as f:
                for i, _ in enumerate(f):
                    if i + 1 >= READ_CAP_LINES:
                        n = f.tell()
                        break
                else:
                    n = self.size
            self.capped_bytes = n
        except OSError:
            self.capped_bytes = self.size


def parse_args():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--log", default=LOG_DEFAULT)
    p.add_argument("--since", help="only count calls on/after this date (YYYY-MM-DD)")
    p.add_argument("--project", help="only count paths containing this substring")
    p.add_argument("--per-project", action="store_true", help="break down by Coding_Projects/<name>")
    return p.parse_args()


def project_of(path):
    marker = "/Coding_Projects/"
    i = path.find(marker)
    if i < 0:
        return "(other)"
    rest = path[i + len(marker):]
    return rest.split("/", 1)[0]


def fmt_bytes(n):
    for unit in ("B", "KB", "MB", "GB"):
        if abs(n) < 1024:
            return f"{n:,.0f} {unit}"
        n /= 1024
    return f"{n:,.1f} TB"


def main():
    args = parse_args()
    since_ts = 0
    if args.since:
        since_ts = int(dt.datetime.strptime(args.since, "%Y-%m-%d").timestamp())

    if not os.path.exists(args.log):
        sys.exit(f"log not found: {args.log}")

    files = {}  # path -> FileInfo
    per_tool = defaultdict(lambda: [0, 0, 0])   # tool -> [calls, served, baseline]
    per_proj = defaultdict(lambda: [0, 0, 0])
    skipped_missing = 0
    error_calls = 0

    with open(args.log, errors="replace") as f:
        for line in f:
            parts = line.rstrip("\n").split("\t")
            if len(parts) != 5:
                continue
            ts_s, tool, bytes_s, outcome, path = parts
            try:
                ts, served = int(ts_s), int(bytes_s)
            except ValueError:
                continue
            if ts < since_ts:
                continue
            if args.project and args.project not in path:
                continue

            if outcome.startswith("error"):
                error_calls += 1
                baseline = 0
            elif tool in OVERHEAD_TOOLS:
                baseline = 0
            elif tool in WHOLE_FILE_BASELINE:
                info = files.get(path)
                if info is None:
                    info = files[path] = FileInfo(path)
                if info.capped_bytes is None:
                    skipped_missing += 1
                    baseline = served  # neutral: no claim either way
                else:
                    # Naive Read never serves less than ffr did for the same
                    # need; floor the baseline at served bytes.
                    baseline = max(info.capped_bytes, served)
            else:
                baseline = served  # unknown tool: neutral

            per_tool[tool][0] += 1
            per_tool[tool][1] += served
            per_tool[tool][2] += baseline
            proj = project_of(path)
            per_proj[proj][0] += 1
            per_proj[proj][1] += served
            per_proj[proj][2] += baseline

    total_calls = sum(v[0] for v in per_tool.values())
    served = sum(v[1] for v in per_tool.values())
    baseline = sum(v[2] for v in per_tool.values())
    if total_calls == 0:
        sys.exit("no matching log entries")

    saved = baseline - served
    pct = 100.0 * saved / baseline if baseline else 0.0

    print(f"ffr token-savings report — {total_calls:,} calls"
          + (f" since {args.since}" if args.since else "")
          + (f", project filter: {args.project!r}" if args.project else ""))
    print(f"  served by ffr:    {fmt_bytes(served):>12}  (~{served / BYTES_PER_TOKEN:,.0f} tokens)")
    print(f"  naive baseline:   {fmt_bytes(baseline):>12}  (~{baseline / BYTES_PER_TOKEN:,.0f} tokens)")
    print(f"  saved:            {fmt_bytes(saved):>12}  (~{saved / BYTES_PER_TOKEN:,.0f} tokens, {pct:.1f}%)")
    if skipped_missing:
        print(f"  ({skipped_missing:,} calls on since-deleted files counted neutral; "
              f"{error_calls:,} error calls counted as overhead)")

    print("\nper tool:")
    print(f"  {'tool':<24} {'calls':>7} {'served':>10} {'baseline':>10} {'saved %':>8}")
    for tool, (c, s, b) in sorted(per_tool.items(), key=lambda kv: -(kv[1][2] - kv[1][1])):
        tp = 100.0 * (b - s) / b if b else 0.0
        tag = "  (overhead)" if tool in OVERHEAD_TOOLS else ""
        print(f"  {tool:<24} {c:>7,} {fmt_bytes(s):>10} {fmt_bytes(b):>10} {tp:>7.1f}%{tag}")

    if args.per_project:
        print("\nper project (top 15 by savings):")
        print(f"  {'project':<32} {'calls':>7} {'served':>10} {'baseline':>10} {'saved %':>8}")
        ranked = sorted(per_proj.items(), key=lambda kv: -(kv[1][2] - kv[1][1]))[:15]
        for proj, (c, s, b) in ranked:
            tp = 100.0 * (b - s) / b if b else 0.0
            print(f"  {proj:<32} {c:>7,} {fmt_bytes(s):>10} {fmt_bytes(b):>10} {tp:>7.1f}%")


if __name__ == "__main__":
    main()
