"""Turns criterion's results into a table of report-err against each other
library, and gates regressions (PLAN 11.3, D-056).

    python benches/ratios.py <criterion dir> <platform> [--baseline FILE] [--write-baseline FILE]

A ratio is report-err's median time over the other library's, measured in
the same run, so a slow or busy machine moves both sides. A ratio more than
5% above the stored baseline's for the same platform is a warning: runners
differ in CPU from run to run, so stored ratios drift.

The gate is criterion's own comparison with the base branch, measured
earlier in the same job (`--save-baseline base`): it fails when one of
report-err's times is slower than the base's by more than 5% at the lower
end of the confidence interval.
"""

import json
import pathlib
import sys

TOLERANCE = 1.05

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
        if change.exists():
            interval = json.loads(change.read_text())["mean"]["confidence_interval"]
            changes[title] = interval["lower_bound"]
    return rows, changes


def split(title):
    """'group/B3 question mark/report-err' -> ('group/B3 question mark', 'report-err', '')
    'group/B8 create + layers/anyhow/3' -> ('group/B8 create + layers', 'anyhow', '3')"""
    parts = title.split("/")
    for i, part in enumerate(parts):
        if part in LIBS:
            return "/".join(parts[:i]), part, "/".join(parts[i + 1:])
    return title, "", ""


def ours(title):
    """report-err's own rows: its side of a comparison, or a group only it
    has. The RD-14 model is not report-err's code."""
    name, lib, _ = split(title)
    if name.startswith("rd14"):
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
        f"- {title}: at least {changes[title] * 100:+.1f}% against the base branch"
        for title in sorted(compared)
        if changes[title] > TOLERANCE - 1
    ]
    out.append("")
    out.append(f"Compared with the base branch on this runner: {len(compared)} of report-err's times.")
    if failed:
        out.append("")
        out.append("Slower than the base branch by more than 5%:")
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
