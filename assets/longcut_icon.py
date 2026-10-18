import sys
from PIL import Image, ImageDraw, ImageFilter
S = 1024
out = sys.argv[1]

def lerp(a, b, t): return tuple(round(a[i] + (b[i] - a[i]) * t) for i in range(3))
top, mid, bot = (38, 24, 92), (112, 56, 240), (255, 92, 122)
grad = Image.new("RGB", (S, S))
px = grad.load()
for y in range(S):
    for x in range(S):
        t = (x * 0.35 + y * 0.65) / S
        px[x, y] = lerp(top, mid, t / 0.55) if t < 0.55 else lerp(mid, bot, (t - 0.55) / 0.45)

mask = Image.new("L", (S, S), 0)
ImageDraw.Draw(mask).rounded_rectangle([40, 40, S - 40, S - 40], radius=220, fill=255)
icon = Image.new("RGBA", (S, S), (0, 0, 0, 0))
icon.paste(grad, (0, 0), mask)

# The mark: an L whose foot is the timeline, and above it three clips on
# their lanes, each one longer than the one above: a long cut.
d = ImageDraw.Draw(icon)
white = (255, 255, 255, 255)
d.rounded_rectangle([240, 220, 360, 800], radius=60, fill=white)   # the L's stem
d.rounded_rectangle([240, 680, 800, 800], radius=60, fill=white)   # its foot, the timeline
clip = (255, 255, 255, 200)
d.rounded_rectangle([420, 240, 610, 320], radius=40, fill=clip)
d.rounded_rectangle([420, 380, 700, 460], radius=40, fill=clip)
d.rounded_rectangle([420, 520, 800, 600], radius=40, fill=clip)

icon.save(out)
