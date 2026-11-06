"""One pipe, S slots, clusters created `delay` moves after each explosion (queued, so
bunched explosions each get their cluster). Reports mean explosion interval per window."""
import sys, json
S=int(sys.argv[1]); delay=int(sys.argv[2]); t_end=int(sys.argv[3]); chain=json.loads(sys.argv[4]); t=int(sys.argv[5])
pending=[]; last=t; intervals=[]; t0=t
while t<t_end:
    t+=1
    chain=[v-1 for v in chain]
    while pending and t>=pending[0]:
        pending.pop(0)
        if len(chain)<S: chain.append(t)
    zero=[i for i,v in enumerate(chain) if v<=0]
    if zero:
        i=zero[0]; chain.pop(i); chain=chain[:i]+[max(1,v//2) for v in chain[i:]]
        intervals.append((t,t-last,i)); last=t; pending.append(t+delay)
win=400
for s in range(t0,t_end,win):
    xs=[iv for (tt,iv,i) in intervals if s<=tt<s+win]
    inplace=sum(1 for (tt,iv,i) in intervals if s<=tt<s+win and i>0)
    if xs: print('moves %5d-%5d: %3d explosions (%d in place), mean interval %5.1f, max %3d, n/interval = %.0f'%(s,s+win,len(xs),inplace,sum(xs)/len(xs),max(xs),(s+win/2)/(sum(xs)/len(xs))))
print('final chain:',chain)
