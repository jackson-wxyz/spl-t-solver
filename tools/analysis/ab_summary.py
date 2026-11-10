"""Summarize an abtest log (also a partial one): per arm, the mean result minus the parent,
how often it ties or beats the parent, the mean gain, seconds, and the paired difference to
the first arm. usage: python -I tools/analysis/ab_summary.py LOG [LOG...]"""
import math
import re
import sys

for path in sys.argv[1:]:
    arms, rows = [], []
    for line in open(path):
        m = re.match(r'ATTEMPT \d+ parent (\d+) back (\d+) seed \d+:(.*)', line)
        if not m:
            continue
        parent = int(m.group(1))
        res = re.findall(r' (\S+) (\d+) \(([\d.]+)s\)', m.group(3))
        if not arms:
            arms = [r[0] for r in res]
        rows.append((parent, int(m.group(2)), [(int(r[1]) - parent, float(r[2])) for r in res]))
    print('%s: %d attempts' % (path, len(rows)))
    print('%-26s %8s %6s %6s %7s %7s %14s' % ('arm', 'mean', 'tie+', 'gain', 'gain+', 'secs', 'vs first (se)'))
    for a, name in enumerate(arms):
        d = [r[2][a][0] for r in rows if len(r[2]) > a]
        t = [r[2][a][1] for r in rows if len(r[2]) > a]
        pd = [r[2][a][0] - r[2][0][0] for r in rows if len(r[2]) > a]
        mean = sum(d) / len(d)
        mp = sum(pd) / len(pd)
        se = math.sqrt(sum((x - mp) ** 2 for x in pd) / max(1, len(pd) - 1) / len(pd))
        print('%-26s %8.1f %5.0f%% %5.0f%% %7.2f %7.1f %8.1f (%.1f)' % (
            name, mean, 100 * sum(x >= 0 for x in d) / len(d), 100 * sum(x > 0 for x in d) / len(d),
            sum(max(0, x) for x in d) / len(d), sum(t) / len(t), mp, se))
