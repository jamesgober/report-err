"""Turns criterion's results into a table of report-err against each other
library, and checks the ratios against a stored baseline (PLAN 11.3).

    python benches/ratios.py <criterion dir> <platform> [--baseline FILE] [--write-baseline FILE]

A ratio is report-err's median time over the other library's, measured in
the same run, so a slow or busy machine moves both sides. The gate fails when
a ratio is more than 5% above the baseline's for the same platform.
"""

import json
import pathlib
import sys

TOLERANCE = 1.05


def load(root):
    rows = {}
    for bench in pathlib.Path(root).rglob("new/benchmark.json"):
        meta = json.loads(bench.read_text())
        est = json.loads((bench.parent / "estimates.json").read_text())
        title = meta.get("title") or meta["full_id"]
        rows[title] = est["median"]["point_estimate"]
    return rows


def split(title):
    """'group/B3 question mark/report-err' -> ('group/B3 question mark', 'report-err', '')
    'group/B8 create + layers/anyhow/3' -> ('group/B8 create + layers', 'anyhow', '3')"""
    parts = title.split("/")
    for i, part in enumerate(parts):
        if part in LIBS:
            return "/".join(parts[:i]), part, "/".join(parts[i + 1:])
    return title, "", ""


LIBS = ["report-err", "anyhow", "eyre", "error-stack", "box"]


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

    rows = load(root)
    table = {}
    for title, value in rows.items():
        name, lib, param = split(title)
        key = name + (f" ({param})" if param else "")
        table.setdefault(key, {})[lib or title] = value

    ratios = {}
    out = [f"### {platform}", "", "| benchmark | report-err | anyhow | eyre | error-stack | box |", "|---|---|---|---|---|---|"]
    for key in sorted(table):
        libs = table[key]
        ours = libs.get("report-err")
        cells = []
        for lib in LIBS:
            v = libs.get(lib)
            if v is None:
                cells.append("")
            elif lib == "report-err" or ours is None:
                cells.append(ns(v))
            else:
                r = ours / v
                ratios[f"{key} / {lib}"] = r
                cells.append(f"{ns(v)} ({r:.2f})")
        if ours is None and not any(lib in libs for lib in LIBS):
            cells = [ns(next(iter(libs.values())))] + [""] * 4
        out.append(f"| {key} | " + " | ".join(cells) + " |")
    out.append("")
    out.append("In brackets: report-err's time over that library's, from the same run.")
    print("\n".join(out))

    failed = []
    if baseline:
        for name, base in baseline.items():
            now = ratios.get(name)
            if now is not None and now > base * TOLERANCE:
                failed.append(f"{name}: {base:.3f} -> {now:.3f}")
    if write:
        data = json.loads(write.read_text()) if write.exists() else {}
        data[platform] = {k: round(v, 4) for k, v in sorted(ratios.items())}
        write.write_text(json.dumps(data, indent=2, sort_keys=True) + "\n")
    if failed:
        print("\nRegressions over 5% against the baseline:")
        print("\n".join(failed))
        sys.exit(1)


if __name__ == "__main__":
    main()
