"""SPL-T world-record history chart for the blog post (updates ryederek's Bluesky plot).

Dates before 2026 are approximate: read off ryederek's chart (2026-03-20 version), except Josh Holder's
1196, dated by his write-up (published 2022-09-16), which also fixes Becker's 1258 / 1277 as later.
usage: python tools/analysis/record_history_chart.py OUT.png  (the blog uses jwxyz/static/graphs/spl-t-record-history.png)
"""
import sys
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt

INK, INK2, MUTED, GRID, SURF = '#0b0b0b', '#52514e', '#8a8985', '#e6e5e1', '#fcfcfb'
COMP, HAND, UNK = '#2a78d6', '#eb6834', '#8a8985'   # computer search / by hand / not stated

# (decimal year, splits, who, method)
recs = [
    (2015.80, 973, 'baronvonholbach', UNK),
    (2018.30, 1027, 'Craig Polley', COMP),
    (2022.02, 1133, 'Josh Holder', COMP),
    (2022.30, 1170, 'Jonathon Becker', HAND),
    (2022.70, 1196, 'Josh Holder', COMP),
    (2022.80, 1258, 'Jonathon Becker', HAND),
    (2022.90, 1277, 'Jonathon Becker', HAND),
    (2026.19, 1396, 'ryederek', UNK),
    (2026.22, 1910, 'ryederek', UNK),
]
SOLVER = (2026.86, 3103)

plt.rcParams.update({'font.family': 'DejaVu Sans', 'font.size': 11})
fig, ax = plt.subplots(figsize=(9, 5.4), dpi=220)
fig.patch.set_facecolor(SURF)
ax.set_facecolor(SURF)

# Context lines: original release and the Legacy Collection re-release.
for x, y, ha, txt in [(2015.80, 3270, 'left', 'SPL-T released (iOS)'),
                      (2025.93, 1200, 'right', 'Legacy Collection\nreleased (Steam, Switch)')]:
    ax.axvline(x, ymax=1 if ha == 'left' else 0.8, color=MUTED, lw=1, ls=(0, (3, 3)), zorder=1)
    ax.text(x + (0.1 if ha == 'left' else -0.1), y, txt, ha=ha, va='top', color=INK2, fontsize=9.5)

# The record as a step function: each record stands until it is beaten.
xs, ys = [], []
for x, y, *_ in recs:
    if xs:
        xs.append(x)
        ys.append(ys[-1])
    xs.append(x)
    ys.append(y)
ax.plot(xs, ys, color=INK2, lw=2, zorder=2, solid_joinstyle='miter')
# The solver's game: dashed, because it has not been played on the real game.
ax.plot([recs[-1][0], SOLVER[0], SOLVER[0]], [1910, 1910, SOLVER[1]], color=COMP, lw=2, ls=(0, (4, 3)), zorder=2)

for x, y, who, c in recs:
    ax.scatter([x], [y], s=70, color=c, edgecolors=SURF, linewidths=2, zorder=4)
ax.scatter([SOLVER[0]], [SOLVER[1]], s=80, facecolors=SURF, edgecolors=COMP, linewidths=2.2, zorder=4)

def lab(x, y, text, ha='left', dx=0.18, dy=0, **kw):
    ax.text(x + (dx if ha == 'left' else -dx), y + dy, text, ha=ha, va='center', color=INK, fontsize=10.5, **kw)

lab(2015.80, 973, '973  baronvonholbach (2015)', dy=-70)
lab(2018.30, 1027, '1,027  Craig Polley (2018)\nrandom play on his emulator', dy=95)
# 2022: five records in one year, labelled as one block.
ax.text(2021.75, 1560, "2022, traded back and forth:\n"
        "1,133  Josh Holder (Jan)\n"
        "1,170  Jonathon Becker\n"
        "1,196  Josh Holder (Sep)\n"
        "1,258, then 1,277  Jonathon Becker",
        ha='right', va='center', color=INK, fontsize=10.5, linespacing=1.45)
ax.annotate('', xy=(2022.35, 1240), xytext=(2021.85, 1480),
            arrowprops=dict(arrowstyle='-', color=MUTED, lw=1))
lab(2026.19, 1396, '1,396', ha='right', dx=0.15)
lab(2026.22, 1910, '1,910  ryederek (Mar 2026)', ha='right', dx=0.2)
lab(SOLVER[0], SOLVER[1], f'{SOLVER[1]:,}  this solver (Nov 2026)\ncomputed; not yet played', ha='right', dx=0.22)

ax.set_xlim(2015.2, 2027.3)
ax.set_ylim(800, 3300)
ax.set_xticks(range(2016, 2028, 2))
ax.set_yticks([1000, 1500, 2000, 2500, 3000])
ax.set_yticklabels(['1,000', '1,500', '2,000', '2,500', '3,000'])
ax.grid(axis='y', color=GRID, lw=1)
ax.set_axisbelow(True)
for s in ('top', 'right', 'left'):
    ax.spines[s].set_visible(False)
ax.spines['bottom'].set_color(MUTED)
ax.tick_params(colors=INK2, length=0, pad=6)
ax.set_ylabel('splits', color=INK2)
ax.set_title('The SPL-T world record, 2015-2026', loc='left', color=INK, fontsize=13, pad=12)

# Legend: what made each record.
from matplotlib.lines import Line2D
h = [Line2D([], [], marker='o', ls='', ms=8, color=c, markeredgecolor=SURF, markeredgewidth=1.5, label=l)
     for c, l in [(COMP, 'computer search'), (HAND, 'by hand'), (UNK, 'method not stated')]]
h.append(Line2D([], [], marker='o', ls='', ms=8, markerfacecolor=SURF, markeredgecolor=COMP, markeredgewidth=2, label='computed, not yet played on the game'))
leg = ax.legend(handles=h, loc='upper left', bbox_to_anchor=(0.07, 0.86), frameon=False, fontsize=10, labelcolor=INK2, handletextpad=0.4)

fig.tight_layout()
fig.savefig(sys.argv[1], facecolor=SURF)
print('wrote', sys.argv[1])
