"""Find connected components in the Zapfino 'Ronit' mask to identify glyph parts."""
from PIL import Image, ImageDraw, ImageFont
from collections import deque

W, H = 88, 31
img = Image.new("L", (W, H), 0)
d = ImageDraw.Draw(img)
f = ImageFont.truetype("/System/Library/Fonts/Supplemental/Zapfino.ttf", 17)
bbox = d.textbbox((0, 0), "Ronit", font=f)
tw, th = bbox[2] - bbox[0], bbox[3] - bbox[1]
x = (W - tw) // 2 - bbox[0]
y = (H - th) // 2 - bbox[1]
d.text((x, y), "Ronit", font=f, fill=255)
px = img.load()

# Threshold and find connected components
THRESH = 25
visited = [[False]*W for _ in range(H)]
components = []
for sy in range(H):
    for sx in range(W):
        if visited[sy][sx] or px[sx, sy] <= THRESH:
            continue
        # BFS
        comp = []
        q = deque([(sx, sy)])
        visited[sy][sx] = True
        while q:
            cx, cy = q.popleft()
            comp.append((cx, cy))
            for ddx, ddy in [(-1,0),(1,0),(0,-1),(0,1),(-1,-1),(1,-1),(-1,1),(1,1)]:
                nx, ny = cx+ddx, cy+ddy
                if 0 <= nx < W and 0 <= ny < H and not visited[ny][nx] and px[nx, ny] > THRESH:
                    visited[ny][nx] = True
                    q.append((nx, ny))
        components.append(comp)

# Sort components by leftmost x, then topmost y
components.sort(key=lambda c: (min(p[0] for p in c), min(p[1] for p in c)))

for i, comp in enumerate(components):
    xs = [p[0] for p in comp]
    ys = [p[1] for p in comp]
    print(f"comp {i}: size={len(comp):3d}  x={min(xs)}-{max(xs)}  y={min(ys)}-{max(ys)}  "
          f"centroid=({sum(xs)//len(xs)},{sum(ys)//len(ys)})")
