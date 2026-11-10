"""Summarize runs/queue/results.log: per setting, n / mean / best, plus ratchet progress.
usage: python -I tools/queue/summary.py [results.log]"""
import re, sys, statistics as st, os, glob
os.chdir(os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..'))
path = sys.argv[1] if len(sys.argv) > 1 else 'runs/queue/results.log'
groups = {}
for line in open(path, encoding='utf-8', errors='replace'):
    m = re.search(r'RESULT splits=(\d+) start=(\d+) (.*?) rollouts=', line)
    if not m:
        continue
    splits, start, cfg = int(m.group(1)), int(m.group(2)), m.group(3)
    kv = dict(x.split('=', 1) for x in cfg.split())
    key = 'w%s k%s cap%s' % (kv.get('width'), kv.get('k'), kv.get('rollout'))
    for name, default in (('stoch', '0'), ('maxkids', '0'), ('rankpen', '0'), ('wtwin', '0'), ('wstuck', '0'), ('wdebt', '0'), ('policy', 'greedy'), ('hparams', 'default')):
        if kv.get(name, default) != default:
            key += ' %s=%s' % (name, kv[name])
    if kv.get('noise') not in (None, '0.5'):
        key += ' noise=%s' % kv['noise']
    if start != '0' and int(start) > 0:
        key += ' from=%d' % start
    groups.setdefault(key, []).append(splits)
print('%-44s %3s %6s %6s %5s  %s' % ('setting', 'n', 'mean', 'sd', 'best', 'all (sorted)'))
for k, v in sorted(groups.items(), key=lambda kv: -st.mean(kv[1])):
    sd = st.stdev(v) if len(v) > 1 else 0
    print('%-44s %3d %6.0f %6.0f %5d  %s' % (k, len(v), st.mean(v), sd, max(v), ' '.join(map(str, sorted(v)))))
for logp in sorted(glob.glob('runs/ratchet/*/ratchet.log')):
    lines = [l for l in open(logp) if l.startswith('RATCHET')]
    nb = [l.strip() for l in open(logp) if l.startswith('NEWBEST')]
    if lines:
        last = lines[-1]
        best = re.search(r'\(best (\d+)\)', last).group(1)
        print('%s: %d iterations, best %s; last: %s' % (os.path.dirname(logp), len(lines), best, last.strip()[:110]))
    for l in nb[-5:]:
        print('   ' + l)
for logp in sorted(glob.glob('runs/ratchet/*/ils.log')):
    stat = [l for l in open(logp) if l.startswith('STAT')]
    nb = [l.strip() for l in open(logp) if l.startswith('NEWBEST')]
    gains = sum(1 for l in open(logp) if l.startswith('ILS') and 'GAIN' in l)
    if stat:
        m = re.search(r'STAT iter (\d+) .*best (\d+) pool=\[([^\]]*)\] total (\d+)s', stat[-1])
        print('%s: local search, %s iterations in %.1f h, %d gains, best %s, pool [%s]' % (os.path.dirname(logp), m.group(1), int(m.group(4)) / 3600, gains, m.group(2), m.group(3)))
    for l in nb[-5:]:
        print('   ' + l)
