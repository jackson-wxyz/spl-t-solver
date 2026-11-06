import json, sys, statistics as st
frames = json.load(open(sys.argv[1]))
N = len(frames) - 1
W = 100
print('window  clusters/pipe  free  legal  cluster-moves%  roll  rank1%  median-ratio  twins  maxval')
for s in range(0, N, W):
    fs = frames[s:min(s + W, N)]
    cl = st.mean(len(p) for f in fs for p in f['pipes'])
    free = st.mean(f['free'] for f in fs)
    legal = st.mean(f['legal'] for f in fs)
    cm = 100 * sum(1 for f in fs if f['cluster'] not in ('-', 'end')) / len(fs)
    roll = st.mean(f['roll'] for f in fs if f['roll'] >= 0)
    r1 = 100 * sum(1 for f in fs if f['rank'] == 1) / max(1, sum(1 for f in fs if f['rank']))
    ratios = []
    twins = 0
    for f in fs:
        for p in f['pipes']:
            for a, b in zip(p, p[1:]):
                if a >= 20:
                    ratios.append(b / a)
                    if b < 1.5 * a:
                        twins += 1
    mr = st.median(ratios) if ratios else 0
    mx = max((max(p) if p else 0) for f in fs for p in f['pipes'])
    print('%5d  %12.1f  %5.1f  %5.1f  %13.0f  %5.0f  %6.0f  %12.2f  %5.1f  %6d' % (s, cl, free, legal, cm, roll, r1, mr, twins / len(fs), mx))
# first move with all four pipes holding >= 3 clusters, and >= 5
for need in (2, 3, 5, 7):
    k = next((f['move'] for f in frames if all(len(p) >= need for p in f['pipes'])), None)
    print('first move with every pipe holding >=', need, 'clusters:', k)
# cluster sizes created
from collections import Counter
print('cluster creations by size:', Counter(f['cluster'] for f in frames if f['cluster'] not in ('-', 'end')))
# share of moves that create a cluster overall and expected 1 per 3
tot = sum(1 for f in frames if f['cluster'] not in ('-', 'end'))
print('cluster-creating moves:', tot, 'of', N, '= 1 per %.2f moves' % (N / tot))
# H/V direction of cluster-creating moves
print('cluster moves by direction:', Counter(f['dir'] for f in frames if f['cluster'] not in ('-', 'end')))
# How often did the game take a move whose rollout was NOT the longest among the 4?
nb = sum(1 for f in frames if f['rank'] and f['alts'] and f['alts'][f['rank'] - 1] < max(f['alts']))
print('moves where the taken move was not the longest-rollout alternative:', nb, 'of', N)
