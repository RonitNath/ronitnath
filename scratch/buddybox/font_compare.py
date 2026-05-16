"""Render 'Ronit' in several elegant fonts at 88x31 so the user can pick one."""
from PIL import Image, ImageDraw, ImageFont

W, H = 88, 31

# (display_name, path, point_size, italic_marker_only_for_label, y_nudge)
candidates = [
    ("Didot",                "/System/Library/Fonts/Supplemental/Didot.ttc",                28, 0),
    ("Didot Italic",         "/System/Library/Fonts/Supplemental/Didot.ttc",                28, 0),  # italic face index handled below
    ("Hoefler Text Italic",  "/System/Library/Fonts/Supplemental/Hoefler Text.ttc",         26, 0),
    ("Baskerville Italic",   "/System/Library/Fonts/Supplemental/Baskerville.ttc",          26, 0),
    ("Big Caslon",           "/System/Library/Fonts/Supplemental/BigCaslon.ttf",            24, 0),
    ("Apple Chancery",       "/System/Library/Fonts/Supplemental/Apple Chancery.ttf",       26, 0),
    ("Snell Roundhand",      "/System/Library/Fonts/Supplemental/SnellRoundhand.ttc",       28, 0),
    ("Zapfino",              "/System/Library/Fonts/Supplemental/Zapfino.ttf",              16, 0),
    ("New York Italic",      "/System/Library/Fonts/NewYorkItalic.ttf",                     24, 0),
    ("Times New Roman It.",  "/System/Library/Fonts/Supplemental/Times New Roman Italic.ttf", 26, 0),
    ("Copperplate",          "/System/Library/Fonts/Supplemental/Copperplate.ttc",          22, 0),
]

# Map display names to font face indices for .ttc files where we want italic
face_index = {
    "Didot Italic": 1,         # 0=Regular, 1=Italic in Didot.ttc usually
    "Hoefler Text Italic": 1,
    "Baskerville Italic": 1,
    "Snell Roundhand": 0,       # already script
}

def render_button(name, path, size):
    img = Image.new("RGB", (W, H), (0, 0, 0))
    d = ImageDraw.Draw(img)
    idx = face_index.get(name, 0)
    try:
        f = ImageFont.truetype(path, size, index=idx)
    except Exception:
        f = ImageFont.truetype(path, size)
    bbox = d.textbbox((0, 0), "Ronit", font=f)
    tw = bbox[2] - bbox[0]
    th = bbox[3] - bbox[1]
    x = (W - tw) // 2 - bbox[0]
    y = (H - th) // 2 - bbox[1]
    d.text((x, y), "Ronit", font=f, fill=(240, 240, 240))
    return img

# Stack into a comparison sheet at 4x scale for legibility
SCALE = 4
LABEL_W = 200
ROW_H = H * SCALE + 8
sheet = Image.new("RGB", (LABEL_W + W * SCALE + 16, ROW_H * len(candidates) + 8), (24, 24, 28))
d = ImageDraw.Draw(sheet)
try:
    label_font = ImageFont.truetype("/System/Library/Fonts/Supplemental/Arial.ttf", 16)
except Exception:
    label_font = ImageFont.load_default()

for i, (name, path, size, _) in enumerate(candidates):
    btn = render_button(name, path, size)
    btn_big = btn.resize((W * SCALE, H * SCALE), Image.NEAREST)
    y0 = 8 + i * ROW_H
    sheet.paste(btn_big, (LABEL_W, y0))
    d.text((10, y0 + ROW_H // 2 - 10), f"{name} ({size}pt)", font=label_font, fill=(220, 220, 220))

sheet.save("/Users/ronitnath/dev/personal/buddybox/font_compare.png")
print("wrote font_compare.png", sheet.size)

# Also save each at native 88x31 for direct viewing
for (name, path, size, _) in candidates:
    btn = render_button(name, path, size)
    safe = name.replace(" ", "_").replace(".", "")
    btn.save(f"/Users/ronitnath/dev/personal/buddybox/sample_{safe}.png")
