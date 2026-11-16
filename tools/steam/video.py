"""Finish a recording from `play.py --record` for upload.

usage: python video.py RUN.mp4 OUT.mp4 [--upscale N] [--pad916] [--crop W:H:X:Y] [--fit H]

The recording is SPL-T's screen at the screen's own pixels (3:4). Options, applied in this
order:
  --crop W:H:X:Y  cut a rectangle out first (for a --full-screen recording; at 3840 x 2160
                  SPL-T's screen is 1536:2048:1152:56)
  --upscale N     enlarge N times by repeating pixels, so one-pixel lines stay sharp and
                  become N pixels wide (for a recording made in a 1920 x 1080 window,
                  N = 2 gives 1536 x 2048 with two-pixel lines)
  --fit H         scale smoothly to height H (lines get soft; avoid for the master copy)
  --pad916        add black above and below to make a 9:16 frame (a phone's full screen)
Encoded on the GPU (NVENC) at high quality.
"""
import argparse
import subprocess

from play import ffmpeg

ap = argparse.ArgumentParser()
ap.add_argument('src')
ap.add_argument('dst')
ap.add_argument('--crop', default=None)
ap.add_argument('--upscale', type=int, default=1)
ap.add_argument('--fit', type=int, default=None)
ap.add_argument('--pad916', action='store_true')
a = ap.parse_args()
vf = []
if a.crop:
    vf.append('crop=' + a.crop)
if a.upscale > 1:
    vf.append(f'scale=iw*{a.upscale}:ih*{a.upscale}:flags=neighbor')
if a.fit:
    vf.append(f'scale=-2:{a.fit}:flags=lanczos')
if a.pad916:
    vf.append('pad=iw:ceil(iw*16/9/2)*2:0:(oh-ih)/2:black')
vf.append('format=yuv420p')
subprocess.run([ffmpeg(), '-y', '-hide_banner', '-loglevel', 'warning', '-stats', '-i', a.src, '-vf', ','.join(vf),
                '-c:v', 'h264_nvenc', '-preset', 'p6', '-tune', 'hq', '-rc', 'vbr', '-cq', '17', '-b:v', '0',
                '-profile:v', 'high', '-movflags', '+faststart', a.dst], check=True)
