"""Does the opening predict the final length? Reads the CSV printed by the `opening` binary
(stdin or a file) and reports, for the from-scratch rollout-beam games, each opening
statistic's range and its rank correlation with the game's length.

usage: target/release/opening runs/overnight/*.txt runs/exp/rbeam_*.txt | python -I tools/analysis/opening_corr.py
"""
import csv
import math
import re
import sys

rows = list(csv.DictReader(open(sys.argv[1]) if len(sys.argv) > 1 else sys.stdin))
# From-scratch beam games only (file names carry the settings); group by setting family.
games = []
for r in rows:
    m = re.search(r'rbeam_(\d+)_w(\d+)_k(\d+)_r(\d+)_(\w+?)_st(\d+)', r['file'].replace('\\', '/'))
    if not m:
        continue
    r['setting'] = 'w%s r%s st%s' % (m.group(2), m.group(4), m.group(6))
    for k in r:
        if k not in ('file', 'setting'):
            r[k] = float(r[k])
    games.append(r)


def ranks(v):
    order = sorted(range(len(v)), key=lambda i: v[i])
    rk = [0.0] * len(v)
    i = 0
    while i < len(order):
        j = i
        while j + 1 < len(order) and v[order[j + 1]] == v[order[i]]:
            j += 1
        for k in range(i, j + 1):
            rk[order[k]] = (i + j) / 2
        i = j + 1
    return rk


def spearman(x, y):
    rx, ry = ranks(x), ranks(y)
    mx, my = sum(rx) / len(rx), sum(ry) / len(ry)
    sxy = sum((a - mx) * (b - my) for a, b in zip(rx, ry))
    sx = math.sqrt(sum((a - mx) ** 2 for a in rx))
    sy = math.sqrt(sum((b - my) ** 2 for b in ry))
    return sxy / (sx * sy) if sx and sy else float('nan')


feats = ['first', 'cl30', 'cl60', 'big100', 'lost100', 'pts60', 'pipes3', 'free200']
print('%d from-scratch games; length %d-%d' % (len(games), min(g['splits'] for g in games), max(g['splits'] for g in games)))
print('%-9s %8s %8s %8s   %s' % ('statistic', 'min', 'median', 'max', 'rank correlation with length (|r| > %.2f would be significant at 5%%)' % (1.96 / math.sqrt(len(games) - 1))))
for f in feats:
    v = sorted(g[f] for g in games)
    print('%-9s %8.0f %8.0f %8.0f   %+.2f' % (f, v[0], v[len(v) // 2], v[-1], spearman([g[f] for g in games], [g['splits'] for g in games])))
# Longest game per distinct first-cluster move, to see the spread directly.
by = {}
for g in games:
    by.setdefault(int(g['first']), []).append(int(g['splits']))
print('first cluster at move -> game lengths')
for k in sorted(by):
    v = sorted(by[k])
    print('  %3d: n=%2d mean %5.0f  %s' % (k, len(v), sum(v) / len(v), ' '.join(map(str, v[-6:]))))
