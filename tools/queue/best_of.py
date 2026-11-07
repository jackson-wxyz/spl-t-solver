"""Print the path of the longest game among runs/ratchet/*/best*.txt and runs/best/*.txt
(sequence files only). usage: python -I tools/queue/best_of.py"""
import glob
import os
import re


def length(path):
    text = open(path).read()
    if text[:1] != '[':
        return -1
    return len([x for x in re.split(r'[^0-9]+', text) if x])


cands = glob.glob('runs/ratchet/*/best*.txt') + glob.glob('runs/best/*.txt')
print(max(cands, key=length).replace(os.sep, '/'))
