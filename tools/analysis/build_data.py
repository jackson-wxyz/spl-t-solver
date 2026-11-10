"""Bundle the data for the "SPL-T Clockwork" page (tools/viz/index.html) into tools/viz/data.js.

usage: python -I tools/analysis/build_data.py [--game SEQ --stats JSON]

Without --game, the featured game is the longest verified game among runs/best/*.txt and
runs/ratchet/*/best_*.txt, and its stats are (re)exported with target/release/stats.exe
(build it first). Also collects: every rbeam run (runs/exp, runs/overnight), every ratchet
iteration (runs/ratchet/*/ratchet.log), the width-30 beam genealogy (runs/exp/tree_w30_s5.txt),
and the single-pipe interval model behind the upper-bound argument.
"""
import json, os, re, sys, glob, subprocess

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), '..', '..'))
os.chdir(ROOT)
args = sys.argv[1:]
def opt(name, default=None):
    return args[args.index(name) + 1] if name in args else default

# ---------- featured game ----------
def seq_len(path):
    t = open(path).read()
    return len([x for x in re.split(r'[^0-9]+', t) if x])

game_path = opt('--game')
if game_path is None:
    cands = [p for p in glob.glob('runs/best/*.txt') + glob.glob('runs/ratchet/*/best_*.txt') if open(p).read(1) == '[']
    game_path = max(cands, key=seq_len)
stats_path = opt('--stats')
if stats_path is None:
    stats_path = 'runs/best/featured_stats.json'
    exe = os.path.abspath('target/release/stats.exe' if os.path.exists('target/release/stats.exe') else 'target/release/stats')
    subprocess.run([exe, game_path, '--rollout', '--alts', '--json', stats_path], check=True, stdout=subprocess.DEVNULL)
frames = json.load(open(stats_path))
game = {
    'name': os.path.basename(game_path),
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

# ---------- rbeam runs ----------
runs = []
pat = re.compile(r'RESULT splits=(\d+) start=(\d+) (.*?) rollouts=\d+ time=([\d.]+)s -> (\S+)')
seen = set()
for logp in ['runs/queue/results.log'] + glob.glob('runs/exp/*.log'):
    if not os.path.exists(logp):
        continue
    for line in open(logp, encoding='utf-8', errors='replace'):
        m = pat.search(line)
        if not m:
            continue
        splits, start, cfg, secs, path = int(m.group(1)), int(m.group(2)), m.group(3), float(m.group(4)), m.group(5)
        if start != 0 or path in seen:
            continue
        seen.add(path)
        kv = dict(x.split('=', 1) for x in cfg.split())
        if int(kv.get('width', 0)) < 100:
            continue
        key = 'w%s k%s cap%s' % (kv['width'], kv['k'], kv['rollout'])
        for name, default in (('stoch', '0'), ('maxkids', '0'), ('rankpen', '0'), ('wtwin', '0'), ('wstuck', '0'), ('wdebt', '0'), ('policy', 'greedy'), ('hparams', 'default')):
            if kv.get(name, default) != default:
                key += ' %s %s' % (name, kv[name])
        if kv.get('noise') not in (None, '0.5'):
            key += ' noise %s' % kv['noise']
        runs.append({'splits': splits, 'config': key, 'seed': int(kv.get('seed', 0)), 'secs': secs})
# Session-2 runs that predate the results log: recover them from file names in runs/exp.
fpat = re.compile(r'rbeam_(\d+)_w(\d+)_k(\d+)_r(\d+)(?:_([a-z]+))?(?:_st(\d+))?(?:_s(\d+))?\.txt$')
logged = {os.path.basename(p) for p in seen}
for fn in sorted(os.listdir('runs/exp')):
    m = fpat.match(fn)
    if not m or fn in logged:
        continue
    splits, w, k, cap, pol, st, seed = m.groups()
    if int(w) < 100:
        continue
    key = 'w%s k%s cap%s' % (w, k, cap)
    if pol and pol != 'greedy':
        key += ' policy ' + pol
    if st and st != '0':
        key += ' stoch ' + st
    runs.append({'splits': int(splits), 'config': key, 'seed': int(seed or 0), 'secs': None})

# ---------- ratchet lines ----------
# Wall-clock start of each ratchet job, from the queue's worker log ("begin" lines).
import datetime
starts = {}
if os.path.exists('runs/queue/workers.log'):
    for line in open('runs/queue/workers.log'):
        m = re.match(r'(\S+ \S+) \S+ begin \S+ .*--out runs/ratchet/(\S+)', line)
        if m:
            starts[m.group(2)] = datetime.datetime.strptime(m.group(1), '%Y-%m-%d %H:%M:%S')
# Each run's iterations as [iteration, moves back, result, best before, seconds into the run].
runs_raw = []
rpat = re.compile(r'RATCHET iter (\d+) cut (\d+) back (\d+) seed \d+ -> (\d+) \(best (\d+)\) ([\d.]+)s total ([\d.]+)s')
ipat = re.compile(r'ILS iter (\d+) \w+ parent \d+ back (\d+) kicked \d+ -> (\d+) \(best (\d+)\) [\d.]+s total ([\d.]+)s')
spat = re.compile(r'STAT iter (\d+) .* total (\d+)s')
for logp in sorted(glob.glob('runs/ratchet/*/ratchet.log') + glob.glob('runs/ratchet/*/ils.log')):
    name = os.path.basename(os.path.dirname(logp))
    head = open(logp).readline()
    its, attempts, dur = [], 0, 0.0
    for line in open(logp):
        m = rpat.match(line)
        if m:
            it, cut, back, res, best, secs, total = m.groups()
            its.append([int(it), int(back), int(res), int(best), float(total)])
            attempts, dur = int(it), float(total)
            continue
        m = ipat.match(line)
        if m:
            it, back, res, best, total = m.groups()
            its.append([int(it), int(back), int(res), int(best), float(total)])
            attempts, dur = max(attempts, int(it)), max(dur, float(total))
            continue
        m = spat.match(line)
        if m:
            attempts, dur = max(attempts, int(m.group(1))), max(dur, float(m.group(2)))
    if its:
        runs_raw.append({'name': name, 'start': head.strip(), 'iters': its, 'attempts': attempts, 'dur': dur,
                         'kind': 'ils' if logp.endswith('ils.log') else 'ratchet'})

# Time axis: hours during which at least one run was going (idle gaps between sessions removed).
ivs = sorted((starts[r['name']].timestamp(), starts[r['name']].timestamp() + r['dur']) for r in runs_raw if r['name'] in starts)
merged = []
for a0, a1 in ivs:
    if merged and a0 <= merged[-1][1]:
        merged[-1][1] = max(merged[-1][1], a1)
    else:
        merged.append([a0, a1])


def active(t):
    """Seconds of search before wall-clock time t."""
    return sum(max(0.0, min(t, m1) - m0) for m0, m1 in merged)


ratchets = []
for r in runs_raw:
    t0 = starts[r['name']].timestamp() if r['name'] in starts else (merged[0][0] if merged else 0.0)
    its = [[i[0], i[1], i[2], i[3], round(active(t0 + i[4]) / 3600, 4)] for i in r['iters']]
    ratchets.append({'name': r['name'], 'start': r['start'], 'iters': its, 'attempts': r['attempts'], 'kind': r['kind']})

# ---------- beam genealogy ----------
tree = []
tp = 'runs/exp/tree_w30_s5.txt'
if os.path.exists(tp):
    for line in open(tp):
        parts = line.split()
        nodes = [p.split(',') for p in parts[1:]]
        tree.append({'p': [int(n[0]) for n in nodes], 'r': [int(n[3]) for n in nodes]})

# ---------- single-pipe interval model ----------
def pipe_model(depth, n0=900, n_end=9000, delay=3):
    chain = [6, 13, 27, 55, 111, 224, 450, 900][:depth]
    t, pending, last, out = n0, [], n0, []
    while t < n_end:
        t += 1
        chain = [v - 1 for v in chain]
        while pending and t >= pending[0]:
            pending.pop(0)
            if len(chain) < depth:
                chain.append(t)
        zero = [i for i, v in enumerate(chain) if v <= 0]
        if zero:
            i = zero[0]
            chain.pop(i)
            chain = chain[:i] + [max(1, v // 2) for v in chain[i:]]
            out.append((t, t - last))
            last = t
            pending.append(t + delay)
    pts = []
    for s in range(1000, n_end, 200):
        xs = [iv for (tt, iv) in out if s <= tt < s + 200]
        if xs:
            pts.append([s + 100, round(sum(xs) / len(xs), 2)])
    return pts

model = {'depth8': pipe_model(8), 'depth7': pipe_model(7)}

data = {'game': game, 'runs': runs, 'ratchets': ratchets, 'tree': tree, 'tree_width': 30, 'tree_splits': 1106, 'model': model}
s = 'const DATA = ' + json.dumps(data, separators=(',', ':')) + ';\n'
open('tools/viz/data.js', 'w', encoding='utf-8', newline='\n').write(s)
print('featured', game_path, game['splits'], '| runs', len(runs), '| ratchet lines', len(ratchets), sum(len(r['iters']) for r in ratchets), 'iters | bytes', len(s))
