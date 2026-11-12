"""Turns criterion's results into a table of report-err against each other
library, and gates regressions (PLAN 11.3, D-056).

    python benches/ratios.py <criterion dir> <platform> [--baseline FILE] [--write-baseline FILE]
                             [--alternating <criterion dir> <rounds>]

A ratio is report-err's median time over the other library's, measured in
the same run, so a slow or busy machine moves both sides. A ratio more than
5% above the stored baseline's for the same platform is a warning: runners
differ in CPU from run to run, so stored ratios drift.

In CI the gate is `--alternating`: the job measures the base branch and the
head in turn, `rounds` times, on the same runner (`--save-baseline base1`,
`head1`, `base2`, ...), and fails only when one of report-err's times is
slower than the base's by more than 5% and by more than 1 ns in every round.
A shared runner's noise, which moved a different row on a different
platform in each run, does not repeat in every round; a regression does
(D-065).

On a quiet machine the strict gate is criterion's own comparison: measure
the base with `--save-baseline base`, then the head with `--baseline base`,
and run this script on that criterion directory without `--alternating`. It
fails when one of report-err's times is slower than the base's by more than
5% at the lower end of the confidence interval and by more than 1 ns in the
median, so the code-alignment noise of a few-nanosecond row does not fail a
change (D-058).
"""

import json
import pathlib
import sys

TOLERANCE = 1.05

# Slower by less than this is not a regression, whatever the percentage.
FLOOR_NS = 1.0

LIBS = ["report-err", "anyhow", "eyre", "error-stack", "box"]


def load(root):
    rows = {}
    changes = {}
    for bench in pathlib.Path(root).rglob("new/benchmark.json"):
        meta = json.loads(bench.read_text())
        est = json.loads((bench.parent / "estimates.json").read_text())
        title = meta.get("title") or meta["full_id"]
        rows[title] = est["median"]["point_estimate"]
        change = bench.parent.parent / "change" / "estimates.json"
        base = bench.parent.parent / "base" / "estimates.json"
        if change.exists() and base.exists():
            interval = json.loads(change.read_text())["mean"]["confidence_interval"]
            before = json.loads(base.read_text())["median"]["point_estimate"]
            changes[title] = (interval["lower_bound"], rows[title] - before)
    return rows, changes


def median(path):
    return json.loads(path.read_text())["median"]["point_estimate"]


def alternating(root, rounds):
    """report-err's own times from the alternating runs: for each, one
    (relative change, nanoseconds) pair per round, head against base."""
    results = {}
    for head in pathlib.Path(root).rglob("head1/benchmark.json"):
        meta = json.loads(head.read_text())
        title = meta.get("title") or meta["full_id"]
        if not ours(title):
            continue
        bench = head.parent.parent
        pairs = []
        for i in range(1, rounds + 1):
            before = bench / f"base{i}" / "estimates.json"
            after = bench / f"head{i}" / "estimates.json"
            if not (before.exists() and after.exists()):
                break
            b, a = median(before), median(after)
            pairs.append((a / b - 1, a - b))
        if len(pairs) == rounds:
            results[title] = pairs
    return results


def slower(change, delta):
    return change > TOLERANCE - 1 and delta > FLOOR_NS


def split(title):
    """'group/B3 question mark/report-err' -> ('group/B3 question mark', 'report-err', '')
    'group/B8 create + layers/anyhow/3' -> ('group/B8 create + layers', 'anyhow', '3')"""
    parts = title.split("/")
    for i, part in enumerate(parts):
        if part in LIBS:
            return "/".join(parts[:i]), part, "/".join(parts[i + 1:])
    return title, "", ""


# Rows that time only the standard library, kept beside report-err's as a
# reference: no report-err code runs in them, so they are not gated (D-071).
STD_ONLY = (
    "collect/B16 std collect, stops at the first",
    "collect/B16 std collect, none failing",
)


# Rows a change makes slower on purpose, each with the decision that accepts
# it and the most it may cost in nanoseconds per round; a row slower than its
# allowance still fails. Each entry is for one pull request: once the base
# branch has the change, the row is no longer slower, and the entry is
# removed in the next pull request.
ACCEPTED = {
    # define-err 0.10's identify checks a probed error's Display, so outside
    # text cannot forge an identity: about 16 ns per probe (D-082).
    "codes/code found after 3": (20.0, "D-082"),
    "codes/severity found after 3": (20.0, "D-082"),
    # No report-err code on this path changed; a pinned A/B on Linux shows it
    # level, so the CI runner's +10% is the binary's layout (D-083).
    "collect/B16 collect_all, none failing": (5000.0, "D-083"),
}


def ours(title):
    """report-err's own rows: its side of a comparison, or a group only it
    has. The RD-14 model and the std-only reference rows are not report-err's
    code."""
    name, lib, _ = split(title)
    if name.startswith("rd14") or name in STD_ONLY:
        return False
    return lib in ("report-err", "")


def ns(v):
    return f"{v:.2f} ns" if v < 1000 else f"{v / 1000:.2f} us"


def main():
    args = sys.argv[1:]
    root, platform = args[0], args[1]
    baseline = None
    write = None
    if "--baseline" in args:
        path = pathlib.Path(args[args.index("--baseline") + 1])
        if path.exists():
            baseline = json.loads(path.read_text()).get(platform)
    if "--write-baseline" in args:
        write = pathlib.Path(args[args.index("--write-baseline") + 1])
    alternate = None
    if "--alternating" in args:
        at = args.index("--alternating")
        alternate = (pathlib.Path(args[at + 1]), int(args[at + 2]))

    rows, changes = load(root)
    table = {}
    for title, value in rows.items():
        name, lib, param = split(title)
        key = name + (f" ({param})" if param else "")
        table.setdefault(key, {})[lib or title] = value

    ratios = {}
    out = [f"### {platform}", "", "| benchmark | report-err | anyhow | eyre | error-stack | box |", "|---|---|---|---|---|---|"]
    for key in sorted(table):
        libs = table[key]
        mine = libs.get("report-err")
        cells = []
        for lib in LIBS:
            v = libs.get(lib)
            if v is None:
                cells.append("")
            elif lib == "report-err" or mine is None:
                cells.append(ns(v))
            else:
                r = mine / v
                ratios[f"{key} / {lib}"] = r
                cells.append(f"{ns(v)} ({r:.2f})")
        if mine is None and not any(lib in libs for lib in LIBS):
            cells = [ns(next(iter(libs.values())))] + [""] * 4
        out.append(f"| {key} | " + " | ".join(cells) + " |")
    out.append("")
    out.append("In brackets: report-err's time over that library's, from the same run.")

    drifted = []
    if baseline:
        for name, base in baseline.items():
            now = ratios.get(name)
            if now is not None and now > base * TOLERANCE:
                drifted.append(f"- {name}: {base:.3f} -> {now:.3f}")
    if drifted:
        out.append("")
        out.append("Ratios more than 5% above `benches/baseline.json` (a warning; runners differ):")
        out.extend(drifted)

    compared = [t for t in changes if ours(t)]
    failed = [
        f"- {title}: at least {changes[title][0] * 100:+.1f}% ({changes[title][1]:+.2f} ns) against the base branch"
        for title in sorted(compared)
        if slower(*changes[title])
    ]
    if compared:
        out.append("")
        out.append(f"Compared with the base branch on this runner: {len(compared)} of report-err's times.")
    if alternate:
        rounds = alternate[1]
        results = alternating(*alternate)
        out.append("")
        out.append(f"Base and head measured in turn {rounds} times on this runner: {len(results)} of report-err's times.")
        some = []
        accepted = []
        for title in sorted(results):
            pairs = results[title]
            shown = ", ".join(f"{c * 100:+.1f}%" for c, _ in pairs)
            line = f"- {title}: {shown} ({pairs[-1][1]:+.2f} ns in the last round)"
            hits = sum(slower(c, d) for c, d in pairs)
            allowance = ACCEPTED.get(title)
            if hits == rounds and allowance and all(d <= allowance[0] for _, d in pairs):
                accepted.append(f"{line}; within {allowance[0]:.0f} ns ({allowance[1]})")
            elif hits == rounds:
                failed.append(line)
            elif hits:
                some.append(line)
        if some:
            out.append("")
            out.append("Slower by more than 5% in some rounds but not all (noise, not a regression):")
            out.extend(some)
        if accepted:
            out.append("")
            out.append("Slower in every round, accepted by a recorded decision within its allowance:")
            out.extend(accepted)
    if failed:
        out.append("")
        every = f" in all {alternate[1]} rounds" if alternate else ""
        out.append(f"Slower than the base branch by more than 5%{every}:")
        out.extend(failed)
    print("\n".join(out))

    if write:
        data = json.loads(write.read_text()) if write.exists() else {}
        data[platform] = {k: round(v, 4) for k, v in sorted(ratios.items())}
        write.write_text(json.dumps(data, indent=2, sort_keys=True) + "\n")
    if failed:
        sys.exit(1)


if __name__ == "__main__":
    main()
