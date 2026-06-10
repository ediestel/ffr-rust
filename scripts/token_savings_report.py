#!/usr/bin/env python3
"""ffr token-savings benchmark — tokenizer-accurate, built-ins-baseline.

Measures what ffr's bounded reads saved versus what Claude Code's built-in
tools would have served for the same work, from the call log at
~/.local/share/ffr/reads.log (format: ts \\t tool \\t bytes \\t outcome \\t path).

Two baselines are computed per call and reported as a RANGE; the honest
headline claim is the lower bound:

  default-window  An agent using only built-in tools reads each touched file
                  with Read's default window (first 2000 lines). This is the
                  commonly observed pattern; upper bound of the claim.

  strict-parity   A maximally skilled agent would have used built-in Grep
                  instead of search_in_file, and ranged Read (offset/limit)
                  instead of read_range_around_line / read_chunk — those
                  calls count as ZERO savings. Only read_file and outline
                  keep the default-window baseline. Lower bound of the claim.

Honesty rules (all chosen to UNDER-state savings):
  * classify_file / stat_file / list_archive / diff_files are pure overhead:
    baseline 0, their served bytes count AGAINST ffr.
  * Error calls count against ffr (baseline 0).
  * extract_pdf_text is neutral (baseline = served) — built-in Read renders
    PDF pages as images, whose token cost we refuse to estimate.
  * Files that no longer exist, or are binary, are neutral (baseline = served).
  * Per-call baseline is floored at tokens served (built-in Read never serves
    less than ffr did for the same need).
  * Files are stat'd as they exist TODAY; drift since the call is a wash and
    is noted, not corrected.

Token counting:
  * With tiktoken installed (o200k_base), each baseline payload (the file's
    first 2000 lines) is tokenized EXACTLY; served tokens are estimated as
    served_bytes x that same file's measured tokens-per-byte ratio.
  * --calibrate N additionally samples N files against the Anthropic
    count_tokens API (free endpoint; needs ANTHROPIC_API_KEY) and rescales
    absolute token counts by the measured Claude/tiktoken ratio.
    Percentages are unaffected; absolute tokens and dollars are.
  * Without tiktoken, falls back to bytes/4 with a loud warning.

Cost framing: savings are in INPUT tokens at the moment of serving. Prompt
caching discounts re-sent conversation prefixes equally for ffr and the
baseline, so the percentage claim stands under caching; dollar figures
(--price-mtok) assume uncached input rates and are an upper bound.

Usage:
  python3 token_savings_report.py [--log PATH] [--since YYYY-MM-DD]
      [--project SUBSTRING] [--per-project] [--price-mtok USD]
      [--calibrate N] [--model ID] [--json]
"""

import argparse
import datetime as dt
import json
import os
import sys
from collections import defaultdict

LOG_DEFAULT = os.path.expanduser("~/.local/share/ffr/reads.log")
READ_CAP_LINES = 2000          # built-in Read default window
MAX_SCAN_BYTES = 4 * 1024 * 1024
FALLBACK_TOKENS_PER_BYTE = 0.25
CALIBRATE_SNIPPET_BYTES = 24_000

PARITY_IN_STRICT = {"search_in_file", "read_range_around_line", "read_chunk"}
WINDOW_TOOLS = {"read_file", "outline"} | PARITY_IN_STRICT
NEUTRAL_TOOLS = {"extract_pdf_text"}
OVERHEAD_TOOLS = {"classify_file", "stat_file", "build_line_index",
                  "list_archive", "diff_files"}


def load_encoder():
    try:
        import tiktoken
        return tiktoken.get_encoding("o200k_base")
    except Exception:
        return None


class FileInfo:
    """Capped (first 2000 lines) size and exact token count for one path."""

    __slots__ = ("capped_bytes", "capped_tokens", "ratio")

    def __init__(self, path, enc):
        self.capped_bytes = None    # None => missing or binary => neutral
        self.capped_tokens = None
        self.ratio = None
        try:
            size = os.path.getsize(path)
        except OSError:
            return
        if size == 0:
            self.capped_bytes = 0
            self.capped_tokens = 0
            return
        try:
            with open(path, "rb") as f:
                buf = f.read(MAX_SCAN_BYTES)
        except OSError:
            return
        if b"\x00" in buf[:8192]:
            return
        pos = 0
        for _ in range(READ_CAP_LINES):
            nxt = buf.find(b"\n", pos)
            if nxt == -1:
                pos = len(buf)
                break
            pos = nxt + 1
        capped = buf[:pos]
        self.capped_bytes = len(capped)
        if enc is not None:
            text = capped.decode("utf-8", errors="replace")
            self.capped_tokens = len(enc.encode(text, disallowed_special=()))
        else:
            self.capped_tokens = round(len(capped) * FALLBACK_TOKENS_PER_BYTE)
        if self.capped_bytes:
            self.ratio = self.capped_tokens / self.capped_bytes


def calibrate_against_api(paths, enc, model, n):
    """Return (Claude/tiktoken ratio, files used) from up to n sample files."""
    key = os.environ.get("ANTHROPIC_API_KEY")
    if not key:
        print("calibration skipped: ANTHROPIC_API_KEY not set", file=sys.stderr)
        return None
    import urllib.request
    paths = sorted(paths)
    if len(paths) > n:  # evenly spaced sample for content diversity
        step = len(paths) / n
        paths = [paths[int(i * step)] for i in range(n)]
    api_total = tk_total = used = 0
    for path in paths:
        try:
            with open(path, "rb") as f:
                text = f.read(CALIBRATE_SNIPPET_BYTES).decode("utf-8", errors="replace")
        except OSError:
            continue
        if not text.strip():
            continue
        body = json.dumps({"model": model,
                           "messages": [{"role": "user", "content": text}]}).encode()
        req = urllib.request.Request(
            "https://api.anthropic.com/v1/messages/count_tokens", data=body,
            headers={"x-api-key": key, "anthropic-version": "2023-06-01",
                     "content-type": "application/json"})
        try:
            with urllib.request.urlopen(req, timeout=30) as r:
                api_tokens = json.load(r)["input_tokens"]
        except Exception as e:
            print(f"calibration: {os.path.basename(path)}: {e}", file=sys.stderr)
            continue
        api_total += api_tokens
        tk_total += len(enc.encode(text, disallowed_special=()))
        used += 1
    if tk_total == 0:
        return None
    return api_total / tk_total, used


def project_of(path):
    marker = "/Coding_Projects/"
    i = path.find(marker)
    if i < 0:
        return "(other)"
    return path[i + len(marker):].split("/", 1)[0]


def fmt_tok(n):
    if abs(n) >= 1e9:
        return f"{n / 1e9:,.2f}B"
    if abs(n) >= 1e6:
        return f"{n / 1e6:,.1f}M"
    if abs(n) >= 1e3:
        return f"{n / 1e3:,.1f}k"
    return f"{n:,.0f}"


def parse_args():
    p = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--log", default=LOG_DEFAULT)
    p.add_argument("--since", help="only count calls on/after this date (YYYY-MM-DD)")
    p.add_argument("--project", help="only count paths containing this substring")
    p.add_argument("--per-project", action="store_true",
                   help="break down by Coding_Projects/<name>")
    p.add_argument("--price-mtok", type=float,
                   help="USD per 1M input tokens for the dollar line (e.g. 3.0)")
    p.add_argument("--calibrate", type=int, metavar="N",
                   help="sample N files against the Anthropic count_tokens API "
                        "and rescale absolute token counts (needs ANTHROPIC_API_KEY)")
    p.add_argument("--model", default="claude-sonnet-4-6",
                   help="model id for --calibrate (default: %(default)s)")
    p.add_argument("--json", action="store_true", help="emit machine-readable JSON")
    return p.parse_args()


def main():
    args = parse_args()
    since_ts = 0
    if args.since:
        since_ts = int(dt.datetime.strptime(args.since, "%Y-%m-%d").timestamp())
    if not os.path.exists(args.log):
        sys.exit(f"log not found: {args.log}")

    enc = load_encoder()
    if enc is None:
        print("WARNING: tiktoken not installed — falling back to bytes/4. "
              "Absolute token counts are rough; percentages still hold.",
              file=sys.stderr)

    files = {}                  # path -> FileInfo
    # tool -> [calls, served_bytes, served_tok, window_tok, strict_tok]
    per_tool = defaultdict(lambda: [0, 0, 0.0, 0.0, 0.0])
    per_proj = defaultdict(lambda: [0, 0, 0.0, 0.0, 0.0])
    calib_candidates = []
    malformed = error_calls = neutral_calls = 0
    ts_min, ts_max = None, None

    with open(args.log, errors="replace") as f:
        for line in f:
            parts = line.rstrip("\n").split("\t")
            if len(parts) != 5:
                malformed += 1
                continue
            ts_s, tool, bytes_s, outcome, path = parts
            try:
                ts, served_b = int(ts_s), int(bytes_s)
            except ValueError:
                malformed += 1
                continue
            if ts < since_ts:
                continue
            if args.project and args.project not in path:
                continue
            ts_min = ts if ts_min is None else min(ts_min, ts)
            ts_max = ts if ts_max is None else max(ts_max, ts)

            info = None
            if tool in WINDOW_TOOLS:
                info = files.get(path)
                if info is None:
                    info = files[path] = FileInfo(path, enc)
                    if (enc is not None and info.capped_tokens
                            and info.capped_tokens > 500):
                        calib_candidates.append(path)

            ratio = (info.ratio if info is not None and info.ratio
                     else FALLBACK_TOKENS_PER_BYTE)
            served_t = served_b * ratio

            if outcome.startswith("error"):
                error_calls += 1
                window_t = strict_t = 0.0
            elif tool in OVERHEAD_TOOLS:
                window_t = strict_t = 0.0
            elif tool in NEUTRAL_TOOLS:
                window_t = strict_t = served_t
            elif tool in WINDOW_TOOLS:
                if info.capped_tokens is None:
                    neutral_calls += 1
                    window_t = strict_t = served_t
                else:
                    window_t = max(float(info.capped_tokens), served_t)
                    strict_t = served_t if tool in PARITY_IN_STRICT else window_t
            else:
                window_t = strict_t = served_t

            for table, key in ((per_tool, tool), (per_proj, project_of(path))):
                row = table[key]
                row[0] += 1
                row[1] += served_b
                row[2] += served_t
                row[3] += window_t
                row[4] += strict_t

    total = [sum(r[i] for r in per_tool.values()) for i in range(5)]
    if total[0] == 0:
        sys.exit("no matching log entries")

    calib_ratio, calib_used = None, 0
    if args.calibrate:
        if enc is None:
            print("calibration requires tiktoken", file=sys.stderr)
        else:
            res = calibrate_against_api(calib_candidates, enc, args.model,
                                        args.calibrate)
            if res:
                calib_ratio, calib_used = res
                for table in (per_tool, per_proj):
                    for row in table.values():
                        row[2] *= calib_ratio
                        row[3] *= calib_ratio
                        row[4] *= calib_ratio
                total = [sum(r[i] for r in per_tool.values()) for i in range(5)]

    calls, served_b, served_t, window_t, strict_t = total
    saved_w, saved_s = window_t - served_t, strict_t - served_t
    pct_w = 100.0 * saved_w / window_t if window_t else 0.0
    pct_s = 100.0 * saved_s / strict_t if strict_t else 0.0

    if args.json:
        out = {
            "calls": calls, "served_bytes": served_b,
            "served_tokens": round(served_t),
            "tokenizer": "o200k_base" if enc else "bytes/4 fallback",
            "calibration": ({"model": args.model, "ratio": calib_ratio,
                             "files": calib_used} if calib_ratio else None),
            "baselines": {
                "default_window": {"tokens": round(window_t),
                                   "saved": round(saved_w), "pct": round(pct_w, 1)},
                "strict_parity": {"tokens": round(strict_t),
                                  "saved": round(saved_s), "pct": round(pct_s, 1)},
            },
            "per_tool": {t: {"calls": r[0], "served_tokens": round(r[2]),
                             "window_tokens": round(r[3]),
                             "strict_tokens": round(r[4])}
                         for t, r in sorted(per_tool.items())},
            "counted_against_ffr": {"errors": error_calls,
                                    "overhead_tools": sorted(OVERHEAD_TOOLS)},
            "neutral_calls": neutral_calls, "malformed_lines": malformed,
        }
        if args.price_mtok:
            out["usd_saved_uncached"] = {
                "price_per_mtok": args.price_mtok,
                "strict": round(saved_s / 1e6 * args.price_mtok, 2),
                "default_window": round(saved_w / 1e6 * args.price_mtok, 2)}
        print(json.dumps(out, indent=2))
        return

    span = ""
    if ts_min:
        d0 = dt.datetime.fromtimestamp(ts_min).strftime("%Y-%m-%d")
        d1 = dt.datetime.fromtimestamp(ts_max).strftime("%Y-%m-%d")
        span = f" ({d0} → {d1})"
    print(f"ffr token-savings benchmark — {calls:,} calls{span}"
          + (f", project filter: {args.project!r}" if args.project else ""))
    if enc:
        print("tokenizer: tiktoken o200k_base — baselines tokenized exactly; "
              "served scaled by per-file token density")
    else:
        print("tokenizer: bytes/4 FALLBACK — install tiktoken for exact counts")
    if calib_ratio:
        print(f"calibrated: ×{calib_ratio:.3f} vs {args.model} "
              f"count_tokens API ({calib_used} files)")

    print()
    print(f"  {'baseline':<18} {'served':>10} {'baseline':>10} "
          f"{'saved':>10} {'reduction':>10}")
    for name, base_t, saved, pct in (
            ("default-window", window_t, saved_w, pct_w),
            ("strict-parity", strict_t, saved_s, pct_s)):
        print(f"  {name:<18} {fmt_tok(served_t):>10} {fmt_tok(base_t):>10} "
              f"{fmt_tok(saved):>10} {pct:>9.1f}%")

    lo, hi = sorted((pct_s, pct_w))
    print(f"\n  headline (honest range): {lo:.0f}–{hi:.0f}% fewer input tokens "
          f"than built-in tools")
    if args.price_mtok:
        usd_lo, usd_hi = sorted((saved_s, saved_w))
        print(f"  at ${args.price_mtok:.2f}/MTok uncached input: "
              f"${usd_lo / 1e6 * args.price_mtok:,.2f}–"
              f"${usd_hi / 1e6 * args.price_mtok:,.2f} saved")
    print(f"\n  counted against ffr: {error_calls:,} error calls + all "
          f"classify/stat overhead; {neutral_calls:,} calls on "
          f"missing/binary files counted neutral; "
          f"{malformed:,} malformed log lines skipped")

    print(f"\nper tool:")
    print(f"  {'tool':<24} {'calls':>7} {'served':>9} "
          f"{'window':>9} {'win %':>7} {'strict':>9} {'str %':>7}")
    for tool, (c, _, s, w, st) in sorted(per_tool.items(),
                                         key=lambda kv: -(kv[1][3] - kv[1][2])):
        wp = 100.0 * (w - s) / w if w else 0.0
        sp = 100.0 * (st - s) / st if st else 0.0
        tag = "  (overhead)" if tool in OVERHEAD_TOOLS else ""
        print(f"  {tool:<24} {c:>7,} {fmt_tok(s):>9} "
              f"{fmt_tok(w):>9} {wp:>6.1f}% {fmt_tok(st):>9} {sp:>6.1f}%{tag}")

    if args.per_project:
        print(f"\nper project (top 15 by default-window savings):")
        print(f"  {'project':<32} {'calls':>7} {'served':>9} "
              f"{'window':>9} {'win %':>7} {'strict':>9} {'str %':>7}")
        ranked = sorted(per_proj.items(),
                        key=lambda kv: -(kv[1][3] - kv[1][2]))[:15]
        for proj, (c, _, s, w, st) in ranked:
            wp = 100.0 * (w - s) / w if w else 0.0
            sp = 100.0 * (st - s) / st if st else 0.0
            print(f"  {proj:<32} {c:>7,} {fmt_tok(s):>9} "
                  f"{fmt_tok(w):>9} {wp:>6.1f}% {fmt_tok(st):>9} {sp:>6.1f}%")


if __name__ == "__main__":
    main()
