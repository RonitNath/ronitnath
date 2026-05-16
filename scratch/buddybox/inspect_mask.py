"""Render Zapfino 'Ronit' at 17pt and emit ASCII map + zoomed mask for inspection."""
from PIL import Image, ImageDraw, ImageFont

W, H = 88, 31
PT = 17
img = Image.new("L", (W, H), 0)
d = ImageDraw.Draw(img)
f = ImageFont.truetype("/System/Library/Fonts/Supplemental/Zapfino.ttf", PT)
bbox = d.textbbox((0, 0), "Ronit", font=f)
tw, th = bbox[2] - bbox[0], bbox[3] - bbox[1]
x = (W - tw) // 2 - bbox[0]
y = (H - th) // 2 - bbox[1]
d.text((x, y), "Ronit", font=f, fill=255)
img.save("/Users/ronitnath/dev/personal/buddybox/mask17.png")
img.resize((W * 10, H * 10), Image.NEAREST).save("/Users/ronitnath/dev/personal/buddybox/mask17_10x.png")

px = img.load()
print("col index header:")
print("    " + "".join(str(c // 10 % 10) for c in range(W)))
print("    " + "".join(str(c % 10)        for c in range(W)))
for ry in range(H):
    line = ""
    for rx in range(W):
        v = px[rx, ry]
        if   v == 0:   line += "."
        elif v < 50:   line += "·"
        elif v < 120:  line += "+"
        elif v < 200:  line += "o"
        else:          line += "#"
    print(f"{ry:2d}  {line}")
