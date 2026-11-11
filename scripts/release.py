#!/usr/bin/env python3
"""The table of releases in README.md.

  scripts/release.py measure <name> [size ...]   solve the fixed scrambles of each size (all of
                                                 2 to 21 unless sizes are given), one run after
                                                 another, and record moves, seconds and memory
                                                 under <name> in bench/releases.tsv
  scripts/release.py readme                      write the tables into README.md from that file

A release is whatever name is given: a tag or a commit. Measuring it again
replaces its rows. Run it on a quiet machine: the seconds are wall-clock.
"""
import datetime
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BIN = os.path.join(ROOT, "target", "release", "cubenxnxn")
DATA = os.path.join(ROOT, "bench", "releases.tsv")
README = os.path.join(ROOT, "README.md")
HEAD = "release\tcommit\tdate\tsize\tcubes\tmoves\tseconds\tlongest\tmemory_mb"
SIZES = range(2, 22)
BEGIN, END = "<!-- releases:begin -->", "<!-- releases:end -->"


def scrambles(n):
    """The scramble file of a size and how many of its cubes are solved."""
    old = {4: ("scrambles-1000.txt", 100), 5: ("scrambles555-200.txt", 200),
           6: ("scrambles666-100.txt", 20), 7: ("scrambles777-100.txt", 20)}
    if n in old:
        name, count = old[n]
    else:
        count = 100 if n < 4 else 20 if n <= 10 else 5
        name = "scrambles%s-%d.txt" % (str(n) * 3, count)
    path = os.path.join(ROOT, "bench", name)
    if not os.path.exists(path):
        # Random moves, more of them on the larger cubes; the seed is the program's own.
        length = 40 if n < 4 else 50 * n
        text = subprocess.run([BIN, "scrambles", "--size", str(n), "--count", str(count),
                               "--length", str(length)], check=True, capture_output=True, text=True).stdout
        with open(path, "w") as f:
            f.write(text)
    return path, count


def rows():
    if not os.path.exists(DATA):
        return []
    with open(DATA) as f:
        return [line.rstrip("\n").split("\t") for line in f.read().splitlines()[1:] if line.strip()]


def save(all_rows):
    with open(DATA, "w") as f:
        f.write(HEAD + "\n" + "".join("\t".join(r) + "\n" for r in all_rows))


def measure(name, sizes):
    commit = subprocess.run(["git", "-C", ROOT, "rev-parse", "--short", "HEAD"],
                            check=True, capture_output=True, text=True).stdout.strip()
    date = datetime.date.today().isoformat()
    for n in sizes:
        path, count = scrambles(n)
        run = subprocess.run(["/usr/bin/time", "-l", BIN, "row", path, "--size", str(n), "--limit", str(count)],
                             cwd=ROOT, capture_output=True, text=True)
        if run.returncode != 0:
            print("size %d failed: %s" % (n, run.stderr.strip().splitlines()[-1:]), flush=True)
            continue
        size, cubes, moves, seconds, longest = run.stdout.strip().split("\t")
        memory = int(re.search(r"(\d+)\s+maximum resident set size", run.stderr).group(1)) / 2 ** 20
        row = [name, commit, date, size, cubes, moves, seconds, longest, "%.0f" % memory]
        # Appended at once, so that a run cut short keeps what it measured.
        save([r for r in rows() if (r[0], r[3]) != (name, size)] + [row])
        print("\t".join(row), flush=True)


def secs(text):
    s = float(text)
    return "%.2f" % s if s < 1 else "%.1f" % s if s < 100 else "%.0f" % s


def memory(text):
    mb = float(text)
    return "%.0f MB" % mb if mb < 1000 else "%.1f GB" % (mb / 1024)


def tables():
    data = rows()
    names = []
    for r in data:
        if r[0] not in names:
            names.append(r[0])
    names.reverse()  # the newest release first
    by = {(r[0], int(r[3])): r for r in data}
    out = []
    if names:
        new = names[0]
        first = next(r for r in data if r[0] == new)
        out.append("Release **%s** (commit `%s`, %s), default mode:\n" % (new, first[1], first[2]))
        out.append("| Cube | Moves | Seconds | Longest | Memory | Cubes |")
        out.append("|---|---:|---:|---:|---:|---:|")
        for n in SIZES:
            r = by.get((new, n))
            if r:
                out.append("| %dx%dx%d | %s | %s | %s | %s | %s |"
                           % (n, n, n, r[5], secs(r[6]), secs(r[7]), memory(r[8]), r[4]))
        out.append("")
        out.append("Every release, the newest first: mean moves, and under them mean seconds per cube.\n")
        sizes = [n for n in SIZES if any((name, n) in by for name in names)]
        out.append("| Release | " + " | ".join(str(n) for n in sizes) + " |")
        out.append("|---|" + "---:|" * len(sizes))
        for name in names:
            cells = []
            for n in sizes:
                r = by.get((name, n))
                cells.append("%s<br>%s s" % (("%.1f" % float(r[5])), secs(r[6])) if r else "")
            out.append("| **%s** | " % name + " | ".join(cells) + " |")
    return "\n".join(out)


def readme():
    with open(README) as f:
        text = f.read()
    a, b = text.index(BEGIN) + len(BEGIN), text.index(END)
    with open(README, "w") as f:
        f.write(text[:a] + "\n" + tables() + "\n" + text[b:])


if __name__ == "__main__":
    if len(sys.argv) >= 3 and sys.argv[1] == "measure":
        measure(sys.argv[2], [int(a) for a in sys.argv[3:]] or list(SIZES))
    elif sys.argv[1:] == ["readme"]:
        readme()
    else:
        sys.exit(__doc__)
