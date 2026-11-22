"""The keeper beam's family tree, drawn like a chart of competing virus variants.

usage: python tools/analysis/beam_lineages.py OUT.png TRACE[:UPTO][@LAG] [TRACE[:UPTO][@LAG] ...]
           [--theta 0.05] [--lag 100] [--size 12.8x7.2] [--light] [--nested] [--title TEXT] [--sub TEXT]

TRACE is a file written by `keeper ... --trace FILE`: for every step of the search, which
member of the previous step's beam each member came from. Several traces are drawn one
after another on one axis (a later search that started from an earlier one's game); UPTO
cuts a trace off at that split count, and @LAG sets LAG for that trace alone.

What is drawn
- Top: share of the beam held by each *lineage*. A lineage is founded where the tree
  forks into two or more branches that each go on to hold at least THETA of the beam LAG
  or more moves later. A beam member belongs to the youngest such lineage among its
  ancestors, so a lineage's band shrinks as its own sub-lineages are founded inside it (they
  are drawn nested in it), and it ends when its last descendant drops out of the beam.
  Bright bands are lineages that the game finally returned descends from; dim bands died out.
- Middle: how many separate lineages the beam holds, counted as distinct ancestors 10, 25,
  50, 100 and 200 moves back.
- Bottom: how far back the common ancestor of the whole beam lies.

The record game's searches (see docs/RESEARCH_LOG.md, "Session 8"):
  keeper runs/best/game_3103.txt --from 700 --width 300 --seed 2 --wcap 30 --trace A.ktr
  keeper runs/best/game_3588.txt --from 2600 --width 1000 --seed 2 --wsync 60 --trace B.ktr
  python tools/analysis/beam_lineages.py OUT.png A.ktr:2600@30 B.ktr
"""
import colorsys
import sys

import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
import numpy as np

REC = np.dtype([('parent', '<u2'), ('burnt', '<u2'), ('lv', '<u2'), ('phi', 'u1'), ('matched', 'u1'), ('quad', 'u1'), ('doomed', 'u1')])
LAGS = (10, 25, 50, 100, 200)


def opt(name, default, cast=str):
    return cast(sys.argv[sys.argv.index(name) + 1]) if name in sys.argv else default


def load(path):
    """(list of (split count, records), the returned game's (split count, beam index) list)."""
    raw = np.fromfile(path, dtype=np.uint8)
    pos, steps = 0, []
    while True:
        mv, n = (int(v) for v in np.frombuffer(raw, dtype='<u2', count=2, offset=pos))
        pos += 4
        if mv == 0xFFFF:
            return steps, np.frombuffer(raw, dtype='<u2', count=2 * n, offset=pos).reshape(-1, 2).astype(int)
        steps.append((mv, np.frombuffer(raw, dtype=REC, count=n, offset=pos)))
        pos += 10 * n


def analyse(steps, line, theta, lag, upto=None):
    """Lineages of one search. Returns a dict of arrays over the steps kept."""
    T = len(steps)
    par = [s[1]['parent'].astype(np.int64) for s in steps]
    n = np.array([len(p) for p in par])
    marked = [np.zeros(k, dtype=bool) for k in n]
    marked[0][:] = True
    nlin = np.ones((T, len(LAGS)), dtype=int)
    mrca = np.zeros(T, dtype=int)
    for t in range(1, T):
        idx = np.arange(n[t])
        cnt = np.ones(n[t])
        a = 0
        while t - a > 0:
            p = par[t - a][idx]
            idx, inv = np.unique(p, return_inverse=True)
            cnt = np.bincount(inv, weights=cnt)
            a += 1
            if a in LAGS:
                nlin[t, LAGS.index(a)] = len(idx)
            if a >= lag:
                marked[t - a][idx[cnt >= theta * n[t]]] = True
            if len(idx) == 1 and a >= lag:
                break
            if len(idx) == 1 and mrca[t] == 0:
                mrca[t] = a
        if mrca[t] == 0:
            mrca[t] = a
    # A marked node's ancestors are marked.
    for t in range(T - 1, 0, -1):
        marked[t - 1][par[t][marked[t]]] = True
    # Name the lineages: a new one for every marked child of a node with two or more.
    parent_of, born = [-1], [0]
    node_cl = [np.full(k, -1, dtype=int) for k in n]
    node_cl[0][:] = 0
    member_cl = [np.zeros(k, dtype=int) for k in n]
    for t in range(1, T):
        m = np.flatnonzero(marked[t])
        pm = par[t][m]
        kids = np.bincount(pm, minlength=n[t - 1])
        for i, p in zip(m, pm):
            if kids[p] == 1:
                node_cl[t][i] = node_cl[t - 1][p]
            else:
                parent_of.append(int(node_cl[t - 1][p]))
                born.append(t)
                node_cl[t][i] = len(parent_of) - 1
        member_cl[t] = np.where(marked[t], node_cl[t], member_cl[t - 1][par[t]])
    C = len(parent_of)
    share = np.zeros((T, C))
    for t in range(T):
        share[t] = np.bincount(member_cl[t], minlength=C) / n[t]
    # Lineages the returned game descends from.
    on_line = np.zeros(C, dtype=bool)
    first = steps[0][0]
    line_cl = np.full(T, -1)
    for mv, i in line:
        line_cl[mv - first] = member_cl[mv - first][i]
        c = node_cl[mv - first][i]
        if c >= 0:
            on_line[c] = True
    keep = T if upto is None else min(T, upto - first + 1)
    return {'moves': np.array([s[0] for s in steps[:keep]]), 'share': share[:keep], 'parent_of': np.array(parent_of),
            'born': np.array(born), 'on_line': on_line, 'line_cl': line_cl[:keep], 'nlin': nlin[:keep], 'mrca': mrca[:keep], 'n': n[:keep]}


def bands(parent_of, born, share, nested):
    """Drawing order from the bottom, as a list of (lineage, fraction of its share).
    Default: the youngest lineage at the bottom, so that a new one comes up from below and
    pushes the older ones out at the top. Nested: a lineage's band is split in two with its
    sub-lineages between the halves."""
    if not nested:
        return [(c, 1.0) for c in sorted(range(len(born)), key=lambda c: -born[c])]
    kids = {}
    for c, p in enumerate(parent_of):
        if p >= 0:
            kids.setdefault(int(p), []).append(c)
    out, stack = [], [(0, False)]
    # Iterative depth-first walk (the tree can be thousands deep along the main line).
    while stack:
        c, closing = stack.pop()
        ch = [k for k in kids.get(c, []) if share[:, k].max() > 0]
        if closing or not ch:
            out.append((c, 0.5 if (closing or ch) else 1.0))
            continue
        out.append((c, 0.5))
        stack.append((c, True))
        for k in sorted(ch, key=lambda k: born[k]):
            stack.append((k, False))
    return out


def colours(parent_of, on_line, dark):
    """A colour per lineage, stepping round the colour wheel in order of founding so that
    neighbours differ. Lineages the returned game does not descend from are dimmer."""
    cols = []
    for c in range(len(parent_of)):
        hue = (0.58 + 0.381966 * c) % 1.0
        if on_line[c]:
            l, s = (0.62, 0.85) if dark else (0.50, 0.80)
        else:
            l, s = ((0.40, 0.34, 0.46)[c % 3], 0.50) if dark else ((0.74, 0.80, 0.68)[c % 3], 0.50)
        cols.append(colorsys.hls_to_rgb(hue, l, s))
    return cols


def main():
    out = sys.argv[1]
    traces = [a for a in sys.argv[2:] if not a.startswith('--') and sys.argv[sys.argv.index(a) - 1] not in ('--theta', '--lag', '--size', '--title', '--sub')]
    theta, lag = opt('--theta', 0.05, float), opt('--lag', 100, int)
    w, h = (float(v) for v in opt('--size', '12.8x7.2').split('x'))
    dark = '--light' not in sys.argv
    if dark:
        INK, INK2, MUTED, GRID, SURF = '#f1e7e7', '#D8CBCC', '#A59090', '#3d1a1a', '#1E0000'
    else:
        INK, INK2, MUTED, GRID, SURF = '#0b0b0b', '#52514e', '#8a8985', '#e6e5e1', '#fcfcfb'
    plt.rcParams.update({'font.family': 'DejaVu Sans', 'font.size': 11, 'text.color': INK, 'axes.labelcolor': INK2,
                         'xtick.color': INK2, 'ytick.color': INK2, 'axes.edgecolor': MUTED})
    fig, (ax, ax2, ax3) = plt.subplots(3, 1, figsize=(w, h), dpi=200, sharex=True, gridspec_kw={'height_ratios': [5, 1.6, 1.6], 'hspace': 0.09})
    fig.patch.set_facecolor(SURF)
    seams = []
    for spec in traces:
        path, _, own_lag = spec.partition('@')
        upto = ''
        if path.count(':') and path.rsplit(':', 1)[1].isdigit():
            path, upto = path.rsplit(':', 1)
        steps, line = load(path)
        r = analyse(steps, line, theta, int(own_lag) if own_lag else lag, int(upto) if upto else None)
        x = r['moves']
        seams.append((x[0], x[-1], r))
        nested = '--nested' in sys.argv
        order = bands(r['parent_of'], r['born'], r['share'], nested)
        cols = colours(r['parent_of'], r['on_line'], dark)
        base = np.zeros(len(x))
        lo, hi = np.full(len(x), np.nan), np.full(len(x), np.nan)
        for c, f in order:
            top = base + r['share'][:, c] * f
            if (top - base).max() > 0:
                ax.fill_between(x, base * 100, top * 100, color=cols[c], linewidth=0.3, edgecolor=cols[c], antialiased=True)
            here = r['line_cl'] == c
            lo[here], hi[here] = base[here] * 100, top[here] * 100
            base = top
        if not nested:
            # The band that holds the returned game's own ancestor at each move.
            change = np.flatnonzero(np.diff(r['line_cl']) != 0) + 1
            lo[change], hi[change] = np.nan, np.nan
            for edge in (lo, hi):
                ax.plot(x, edge, color=INK, lw=0.9, solid_joinstyle='round')
        live = int((r['share'].max(axis=0) > 0).sum())
        print(f'{path}: moves {x[0]}..{x[-1]}, beam {r["n"].max()}, {live} lineages drawn ({int(r["on_line"].sum())} on the final line), '
              f'common ancestor back: median {int(np.median(r["mrca"]))}, max {int(r["mrca"].max())} (at move {int(x[np.argmax(r["mrca"])])})')
        shades = [0.30, 0.45, 0.60, 0.80, 1.0]
        for k, L in enumerate(LAGS):
            ax2.plot(x, np.maximum(r['nlin'][:, k], 1), color=INK, alpha=shades[k], lw=0.9, label=f'{L} moves back' if spec == traces[0] else None)
        ax3.fill_between(x, 0, r['mrca'], color=MUTED, linewidth=0, alpha=0.55)
        ax3.plot(x, r['mrca'], color=INK, lw=0.9)
    for a in (ax, ax2, ax3):
        a.set_facecolor(SURF)
        for s in ('top', 'right'):
            a.spines[s].set_visible(False)
        for x0, _, _ in seams[1:]:
            a.axvline(x0, color=INK, lw=1, ls=(0, (3, 3)))
    ax.set_ylim(0, 100)
    ax.set_xlim(seams[0][0], seams[-1][1])
    ax.set_ylabel('share of the beam (%)')
    ax.set_title(opt('--title', "The search's competing lineages"), loc='left', color=INK, fontsize=14, pad=24)
    ax.text(0, 1.022, opt('--sub', 'Each colour: the positions descended from one long-lived fork of the search. '
                          'Bright, outlined: the family the final game came from.'),
            transform=ax.transAxes, color=INK2, fontsize=9.5, va='bottom')
    ax2.set_yscale('log')
    ax2.set_ylim(0.8, 1200)
    ax2.set_ylabel('separate\nlineages')
    ax2.legend(loc='upper left', ncol=5, frameon=False, fontsize=8.5, labelcolor=INK2, handlelength=1.4, columnspacing=1.2, bbox_to_anchor=(0, 1.22))
    ax2.grid(axis='y', color=GRID, lw=0.6)
    ax3.set_ylabel('common ancestor\n(moves back)')
    ax3.set_ylim(0, None)
    ax3.grid(axis='y', color=GRID, lw=0.6)
    ax3.set_xlabel('split count (move number)')
    fig.savefig(out, facecolor=SURF, bbox_inches='tight', pad_inches=0.25)
    print('wrote', out)


if __name__ == '__main__':
    main()
