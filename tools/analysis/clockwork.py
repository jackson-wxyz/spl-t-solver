"""Idealized SPL-T clockwork: P pipes, S slots each, 2x2 clusters of 1x1 blocks.

Every move costs one unit of split capacity K. Each explosion refills the top of its pipe
with a block worth `cap` splits (3 for a 2x2 refill) and the worker spends `cap` moves on
that refill (FIFO over pending refills); when they are done a new cluster worth the
current move number sits on top. Point blocks count down 1 per move; when the bottom one
reaches 0 the stack above falls one slot and is halved (floor, min 1). A cluster that
reaches 0 above the bottom explodes in place and the ones above it fall and halve.
The game dies when a move is needed and K == 0. Reports the death move.

usage: clockwork.py [pipes] [slots] [cap] [start_move] [K0]
"""
import sys

def run(P=4, S=8, cap=3, start=600, K0=12, init=None, verbose=False):
    n = start
    K = K0
    if init is None:
        # geometric chains, staggered phases
        pipes = []
        for p in range(P):
            base = (p * 3 * P // P) + 3  # bottom values 3,6,9,12
            chain = []
            v = base + p * 3
            for s in range(S - 1):
                chain.append(v)
                v = v * 2 + 3 * P
            pipes.append(chain)
    else:
        pipes = [list(c) for c in init]
    pending = []  # queue of (pipe, splits_left)
    expl_times = []
    cap_total = 0.0
    cap_given = 0
    while True:
        if K <= 0:
            return n, expl_times
        # the worker makes one move
        K -= 1
        n += 1
        if pending:
            p, left = pending[0]
            left -= 1
            if left == 0:
                pending.pop(0)
                pipes[p].append(n)  # new cluster on top, worth the move number
            else:
                pending[0] = (p, left)
        # countdown and explosions
        for p in range(P):
            chain = pipes[p]
            for i in range(len(chain)):
                chain[i] -= 1
            i = 0
            while i < len(chain):
                if chain[i] <= 0:
                    chain.pop(i)
                    for j in range(i, len(chain)):
                        chain[j] = max(1, chain[j] // 2)
                    cap_total += cap
                    give = int(cap_total) - cap_given
                    cap_given += give
                    K += give
                    pending.append((p, give))
                    expl_times.append(n)
                else:
                    i += 1
            if len(chain) > S:
                return n, expl_times  # overflow (cannot happen physically; treat as death)
        if verbose and n % 200 == 0:
            print(n, 'K', K, 'pipes', [list(reversed(c)) for c in pipes])

if __name__ == '__main__':
    a = sys.argv[1:]
    P = int(a[0]) if len(a) > 0 else 4
    S = int(a[1]) if len(a) > 1 else 8
    cap = float(a[2]) if len(a) > 2 else 3
    start = int(a[3]) if len(a) > 3 else 600
    K0 = int(a[4]) if len(a) > 4 else 12
    # The 2064 game's pipes at move 600 (bottom -> top), from the stats export.
    init600 = [[4,22,58,60,136,294],[1,56,133,132,596,590],[23,24,22,26,558,578,242],[24,23,24,252,558,570]]
    for label, init, st in [('geometric start', None, start), ('2064 game at move 600', init600, 600)]:
        for k0 in (K0,):
            for c in (3, 3.5, 3.75):
                cap_i = c
                # fractional caps: alternate 3/4 to emulate 4x2 or 8x2 refills on average
                death, ex = run(P, S, cap_i, st, k0, init)
                print('%-24s P=%d S=%d cap=%-4s K0=%d -> dies at move %d (%d explosions)' % (label, P, S, c, k0, death, len(ex)))
