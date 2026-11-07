"""Print the path of the longest game among runs/ratchet/*/best*.txt and runs/best/*.txt
(sequence files only). usage: python -I tools/queue/best_of.py"""
import glob, re
def n(p):
    t = open(p).read()
    return len([x for x in re.split(r'[^0-9]+', t) if x]) if t[:1] == '[' else -1
c = glob.glob('runs/ratchet/*/best*.txt') + glob.glob('runs/best/*.txt')
print(max(c, key=n).replace('\', '/'))
