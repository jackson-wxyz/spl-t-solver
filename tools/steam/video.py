"""Make the 1080p copy of a run recorded by `play.py --record` (native 4K, 30 fps).

usage: python video.py RUN.mp4 OUT.mp4 [--board]

Default: the whole screen scaled to 1920 x 1080 (the tablet in the middle, the collection's
background around it). --board: only SPL-T's screen (score bar and board), 810 x 1080,
which also leaves out the controls help and any overlay in the top-left corner.
Encoded on the GPU (NVENC); a 30-minute run takes a few minutes.
"""
import argparse
import subprocess

from play import ffmpeg

ap = argparse.ArgumentParser()
ap.add_argument('src')
ap.add_argument('dst')
ap.add_argument('--board', action='store_true')
a = ap.parse_args()
# SPL-T's screen at 3840 x 2160: x 1152 to 2688, y 56 to 2104 (see board.py).
vf = 'crop=1536:2048:1152:56,scale=810:1080:flags=lanczos' if a.board else 'scale=1920:1080:flags=lanczos'
subprocess.run([ffmpeg(), '-y', '-hide_banner', '-loglevel', 'warning', '-stats', '-i', a.src, '-vf', vf,
                '-c:v', 'h264_nvenc', '-preset', 'p6', '-rc', 'vbr', '-cq', '19', '-b:v', '0',
                '-movflags', '+faststart', a.dst], check=True)
