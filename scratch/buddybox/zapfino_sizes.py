"""Preview Zapfino 'Ronit' at several sizes to find the largest that fits 88x31."""
from PIL import Image, ImageDraw, ImageFont

W, H = 88, 31
PATH = "/System/Library/Fonts/Supplemental/Zapfino.ttf"

sizes = [14, 15, 16, 17, 18, 19, 20, 22]
SCALE = 4
LABEL_W = 100

sheet = Image.new("RGB", (LABEL_W + W * SCALE + 16, (H * SCALE + 8) * len(sizes) + 8), (24, 24, 28))
d_sheet = ImageDraw.Draw(sheet)
label_font = ImageFont.truetype("/System/Library/Fonts/Supplemental/Arial.ttf", 16)

for i, sz in enumerate(sizes):
    img = Image.new("L", (W, H), 0)
    d = ImageDraw.Draw(img)
    f = ImageFont.truetype(PATH, sz)
    bbox = d.textbbox((0, 0), "Ronit", font=f)
    tw = bbox[2] - bbox[0]
    th = bbox[3] - bbox[1]
    x = (W - tw) // 2 - bbox[0]
    y = (H - th) // 2 - bbox[1]
    d.text((x, y), "Ronit", font=f, fill=240)
    rgb = Image.merge("RGB", (img, img, img))
    big = rgb.resize((W * SCALE, H * SCALE), Image.NEAREST)
    y0 = 8 + i * (H * SCALE + 8)
    sheet.paste(big, (LABEL_W, y0))
    d_sheet.text((10, y0 + (H * SCALE) // 2 - 8), f"{sz}pt  ({tw}x{th})", font=label_font, fill=(220, 220, 220))

sheet.save("/Users/ronitnath/dev/personal/buddybox/zapfino_sizes.png")
print("done")
