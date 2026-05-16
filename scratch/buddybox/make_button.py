"""88x31 animated GIF: 'Ronit' in Zapfino 17pt over a starfield with twinkles."""
from PIL import Image, ImageDraw, ImageFont
from collections import deque
import math
import random

W, H = 88, 31
N_FRAMES = 60
FRAME_MS = 110
ZAPFINO = "/System/Library/Fonts/Supplemental/Zapfino.ttf"
TEXT_PT = 17
TEXT_COLOR = (240, 240, 240)

# ---------- 1. Render text mask ----------
mask = Image.new("L", (W, H), 0)
md = ImageDraw.Draw(mask)
font = ImageFont.truetype(ZAPFINO, TEXT_PT)
bbox = md.textbbox((0, 0), "Ronit", font=font)
tw, th = bbox[2] - bbox[0], bbox[3] - bbox[1]
text_x = (W - tw) // 2 - bbox[0]
text_y = (H - th) // 2 - bbox[1]
md.text((text_x, text_y), "Ronit", font=font, fill=255)
mpx = mask.load()

# ---------- 2. Find connected components, identify i-dot ----------
THRESH = 25
visited = [[False] * W for _ in range(H)]
components = []
for sy in range(H):
    for sx in range(W):
        if visited[sy][sx] or mpx[sx, sy] <= THRESH:
            continue
        comp = []
        q = deque([(sx, sy)])
        visited[sy][sx] = True
        while q:
            cx, cy = q.popleft()
            comp.append((cx, cy))
            for ddx in (-1, 0, 1):
                for ddy in (-1, 0, 1):
                    if ddx == 0 and ddy == 0:
                        continue
                    nx, ny = cx + ddx, cy + ddy
                    if (0 <= nx < W and 0 <= ny < H
                            and not visited[ny][nx]
                            and mpx[nx, ny] > THRESH):
                        visited[ny][nx] = True
                        q.append((nx, ny))
        components.append(comp)

# i-dot = smallest component (4 pixels, upper region)
components.sort(key=lambda c: len(c))
idot_pixels = set(components[0])
xs = [p[0] for p in idot_pixels]
ys = [p[1] for p in idot_pixels]
idot_center = (sum(xs) // len(xs), sum(ys) // len(ys))
print(f"i-dot component: {len(idot_pixels)} px, center {idot_center}")

# ---------- 3. Build RGBA text layer EXCLUDING the i-dot ----------
# Quantize antialiased alpha to 4 levels so the text edges produce fewer
# unique palette colors → smaller GIF.
def quantize_alpha(a):
    if a < 32:   return 0
    if a < 96:   return 90
    if a < 176:  return 170
    return 255

text_rgba = Image.new("RGBA", (W, H), (0, 0, 0, 0))
trgba = text_rgba.load()
for y in range(H):
    for x in range(W):
        if (x, y) in idot_pixels:
            continue
        a = quantize_alpha(mpx[x, y])
        if a > 0:
            trgba[x, y] = (TEXT_COLOR[0], TEXT_COLOR[1], TEXT_COLOR[2], a)

# Star keep-out: every text pixel above THRESH (incl. i-dot) + 1px buffer
keepout = set()
for y in range(H):
    for x in range(W):
        if mpx[x, y] > THRESH:
            for dx in range(-1, 2):
                for dy in range(-1, 2):
                    keepout.add((x + dx, y + dy))

# Allow placement on/around the i-dot so the twinkle arms can draw cleanly
icx, icy = idot_center
for dx in range(-2, 3):
    for dy in range(-2, 3):
        keepout.discard((icx + dx, icy + dy))

def safe_for_arms(x, y, arm_len):
    """Check center + cross arms up to arm_len fall in canvas and avoid text."""
    pts = [(x, y)]
    for r in range(1, arm_len + 1):
        pts += [(x - r, y), (x + r, y), (x, y - r), (x, y + r)]
    for px_, py_ in pts:
        if not (0 <= px_ < W and 0 <= py_ < H):
            return False
    return True

# ---------- 4. Twinkle stars: positions, randomized timings ----------
# Hand-placed positions in margins + interior gaps. (i-dot is one of them.)
positions = [
    # left margin (halved: 4 -> 2)
    (5,  3,  True),
    (4,  28, False),
    # right margin (halved: 3 -> 2)
    (85, 17, True),
    (83, 28, False),
    # interior gaps above lowercase letters
    (38, 5,  False),   # above between R and o
    (44, 8,  True),    # above the o
    (53, 4,  False),   # above the n
    (60, 3,  True),    # above the i (left of dot)
    idot_center + (False,),  # ON the i dot — long slow shine
    (68, 3,  False),   # right of i dot
    # below text
    (52, 26, True),    # below n area
    (60, 28, False),   # below i/t
    (74, 26, True),    # below t right
]

# Verify positions don't overlap text after exempting the i-dot
unsafe = []
for (sx, sy, _is_blue) in positions:
    if (sx, sy) in keepout:
        unsafe.append((sx, sy))
for u in unsafe:
    print(f"WARN: position {u} overlaps text — adjust")

# Assign each star its own randomized timing parameters.
# All periods = N_FRAMES (60) so loop is seamless. Variation comes from:
# - independently sampled phase (0..59)
# - independently sampled duration (10..22 frames, ~1.1s..2.4s)
# - independently sampled peak brightness (170..255)
# - independently sampled ramp/fade asymmetry
# - independently sampled sharpness exponent
random.seed(2026)

def make_star(x, y, is_blue):
    return {
        "x": x, "y": y, "is_blue": is_blue,
        "phase": random.randint(0, N_FRAMES - 1),
        "dur":   random.randint(10, 22),
        "peak":  random.choice([180, 195, 210, 225, 240, 255]),
        "ramp_frac": random.uniform(0.25, 0.55),
        "ease_exp":  random.uniform(1.2, 2.0),
        "baseline":  22,
    }

stars = [make_star(*p) for p in positions]

# Special: i-dot star — long slow shine, always faintly visible
for s in stars:
    if (s["x"], s["y"]) == idot_center:
        s["baseline"]  = 80
        s["peak"]      = 255
        s["dur"]       = 42         # ~4.6s of brightening/fading per 6.6s loop
        s["ramp_frac"] = 0.4        # 17 frames up, 25 frames down
        s["ease_exp"]  = 1.1        # gentle, broad shine
        s["is_blue"]   = False
        break

def pulse_brightness(frame, s):
    t = (frame - s["phase"]) % N_FRAMES
    if t >= s["dur"]:
        return s["baseline"]
    ramp = max(1.0, s["dur"] * s["ramp_frac"])
    fade = max(1.0, s["dur"] - ramp)
    if t < ramp:
        f = t / ramp
    else:
        f = 1 - (t - ramp) / fade
    f = math.sin(max(0.0, min(1.0, f)) * math.pi / 2) ** s["ease_exp"]
    return int(s["baseline"] + (s["peak"] - s["baseline"]) * f)

def color_for(is_blue, b):
    if is_blue:
        return (int(b * 0.55), int(b * 0.78), b)
    return (b, b, int(b * 0.96))

# ---------- 5. Field stars (static dim, with subtle slow shimmer on a few) ----------
random.seed(7)
twinkle_keepout = set()
for s in stars:
    for dx in range(-2, 3):
        for dy in range(-2, 3):
            twinkle_keepout.add((s["x"] + dx, s["y"] + dy))

field_stars = []
attempts = 0
placed = set()
while len(field_stars) < 38 and attempts < 8000:
    attempts += 1
    x = random.randint(0, W - 1)
    y = random.randint(0, H - 1)
    if (x, y) in keepout: continue
    if (x, y) in twinkle_keepout: continue
    if (x, y) in placed: continue
    r = random.random()
    if   r < 0.55: b = random.randint(28, 55)
    elif r < 0.85: b = random.randint(55, 90)
    elif r < 0.97: b = random.randint(90, 130)
    else:          b = random.randint(130, 170)
    if random.random() < 0.18:
        col = (int(b * 0.55), int(b * 0.78), b)
    else:
        col = (b, b, int(b * 0.95))
    field_stars.append((x, y, col, b))
    placed.add((x, y))

# Each shimmering field star also gets independent timing
shimmer_ids = random.sample(range(len(field_stars)), k=min(6, len(field_stars)))
shimmer_phases = {i: random.randint(0, N_FRAMES - 1) for i in shimmer_ids}
shimmer_periods = {i: random.choice([30, 40, 60]) for i in shimmer_ids}
shimmer_amp = {i: random.uniform(0.15, 0.30) for i in shimmer_ids}

# ---------- 6. Render ----------
frames = []
for f_idx in range(N_FRAMES):
    img = Image.new("RGB", (W, H), (0, 0, 0))
    px = img.load()

    for i, (x, y, col, base_b) in enumerate(field_stars):
        if i in shimmer_ids:
            t = ((f_idx - shimmer_phases[i]) % shimmer_periods[i]) / shimmer_periods[i]
            mod = (1 - shimmer_amp[i]) + shimmer_amp[i] * 2 * (0.5 + 0.5 * math.sin(t * 2 * math.pi))
            b = max(15, min(220, int(base_b * mod)))
            is_blue = col[2] > col[0] + 10
            px[x, y] = color_for(is_blue, b)
        else:
            px[x, y] = col

    for s in stars:
        b = pulse_brightness(f_idx, s)
        sx, sy = s["x"], s["y"]
        is_blue = s["is_blue"]
        c_center = color_for(is_blue, b)
        if 0 <= sx < W and 0 <= sy < H:
            px[sx, sy] = c_center
        if b > 90:
            arm_b = int(b * 0.55)
            ac = color_for(is_blue, arm_b)
            for dx, dy in ((-1, 0), (1, 0), (0, -1), (0, 1)):
                ax, ay = sx + dx, sy + dy
                if (0 <= ax < W and 0 <= ay < H
                        and (ax, ay) not in keepout):
                    cur = px[ax, ay]
                    if sum(cur) < sum(ac):
                        px[ax, ay] = ac
        if b > 175:
            arm_b = int(b * 0.28)
            ac = color_for(is_blue, arm_b)
            for dx, dy in ((-2, 0), (2, 0), (0, -2), (0, 2)):
                ax, ay = sx + dx, sy + dy
                if (0 <= ax < W and 0 <= ay < H
                        and (ax, ay) not in keepout):
                    cur = px[ax, ay]
                    if sum(cur) < sum(ac):
                        px[ax, ay] = ac

    base_rgba = img.convert("RGBA")
    out_rgba = Image.alpha_composite(base_rgba, text_rgba)
    frames.append(out_rgba.convert("RGB"))

out_path = "/Users/ronitnath/dev/personal/buddybox/ronit.gif"
frames[0].save(
    out_path,
    save_all=True,
    append_images=frames[1:],
    duration=FRAME_MS,
    loop=0,
    optimize=True,
    disposal=2,
)
for k in (0, 15, 30, 45):
    frames[k].save(f"/Users/ronitnath/dev/personal/buddybox/ronit_frame{k:02d}.png")

import os
print("wrote", out_path, "size", os.path.getsize(out_path), "bytes")
print(f"loop = {N_FRAMES * FRAME_MS / 1000:.2f}s, {len(stars)} twinklers, {len(field_stars)} field stars")
