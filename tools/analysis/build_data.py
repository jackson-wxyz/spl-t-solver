"""Bundle visualization data into one JS file: const DATA = {...}.
usage: build_data.py <solver-root> <out.js>"""
import json, re, sys, os
root, out = sys.argv[1], sys.argv[2]

# 1. Per-move stats of the 2064 game.
frames = json.load(open(os.path.join(root, 'runs/best/scratch_2064_stats.json')))
game = {
    'name': 'scratch_2064',
    'splits': len(frames) - 1,
    'roll': [f['roll'] for f in frames],
    'free': [f['free'] for f in frames],
    'pts': [f['pts'] for f in frames],
    'legal': [f['legal'] for f in frames],
    'next': [f['next'] for f in frames],
    'cluster': [f['cluster'] for f in frames],
    'rank': [f['rank'] for f in frames],
    'alts': [f['alts'] for f in frames],
    'pipes': [f['pipes'] for f in frames],
}

# 2. Runs: splits per configuration (from file names in runs/exp).
runs = []
pat = re.compile(r'rbeam_(\d+)_w(\d+)_k(\d+)_r(\d+)(?:_([a-z]+))?(?:_st(\d+))?(?:_s(\d+))?\.txt$')
for fn in sorted(os.listdir(os.path.join(root, 'runs/exp'))):
    m = pat.match(fn)
    if not m:
        continue
    splits, w, k, cap, pol, st, seed = m.groups()
    w, k, cap = int(w), int(k), int(cap)
    if w < 100 and not (w == 30):
        continue  # smoke tests
    pol = pol or 'greedy'
    st = int(st or 0)
    seed = int(seed or 0)
    # Flags not in the file name (wtwin/wstuck/wdebt runs) are recovered from the sweep logs.
    runs.append({'file': fn, 'splits': int(splits), 'w': w, 'k': k, 'cap': cap, 'pol': pol, 'stoch': st, 'seed': seed})
# Tag penalty runs from the sweep logs.
tags = {}
for log in ['rbeam_sweep3.log', 'rbeam_sweep4.log']:
    p = os.path.join(root, 'runs/exp', log)
    if not os.path.exists(p):
        continue
    for line in open(p):
        mm = re.search(r'wtwin=([0-9.]+)', line)
        ms = re.search(r'wstuck=([0-9.]+)', line)
        md = re.search(r'wdebt=([0-9.]+)', line)
        mf = re.search(r'-> runs/exp/(\S+)', line)
        if not mf:
            continue
        t = []
        if mm and float(mm.group(1)) != 0: t.append('twin %s' % mm.group(1))
        if ms and float(ms.group(1)) != 0: t.append('stuck %s' % ms.group(1))
        if md and float(md.group(1)) != 0: t.append('debt %s' % md.group(1))
        tags[mf.group(1)] = ' '.join(t)
for r in runs:
    r['penalty'] = tags.get(r['file'], '')
    base = 'w%d k%d cap%d' % (r['w'], r['k'], r['cap'])
    extra = []
    if r['pol'] != 'greedy': extra.append(r['pol'])
    if r['stoch']: extra.append('stoch %d' % r['stoch'])
    if r['penalty']: extra.append(r['penalty'])
    r['config'] = base + (' ' + ' '.join(extra) if extra else '')

# 3. Beam genealogy (width 30): per level, parent index and rollout length of each node.
tree = []
tp = os.path.join(root, 'runs/exp/tree_w30_s5.txt')
for line in open(tp):
    parts = line.split()
    nodes = [p.split(',') for p in parts[1:]]
    tree.append({'p': [int(n[0]) for n in nodes], 'r': [int(n[3]) for n in nodes]})

data = {'game': game, 'runs': runs, 'tree': tree, 'tree_width': 30, 'tree_splits': 1106}
s = 'const DATA = ' + json.dumps(data, separators=(',', ':')) + ';\n'
open(out, 'w', encoding='utf-8', newline='\n').write(s)
print('runs', len(runs), 'frames', len(frames), 'tree levels', len(tree), 'bytes', len(s))
from collections import defaultdict
by = defaultdict(list)
for r in runs:
    by[r['config']].append(r['splits'])
for c, v in sorted(by.items(), key=lambda kv: -max(kv[1])):
    print('%-32s n=%2d max=%4d mean=%6.0f  %s' % (c, len(v), max(v), sum(v)/len(v), sorted(v)))
