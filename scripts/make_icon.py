#!/usr/bin/env python3
"""Generate WanFlow app icon (1024x1024 PNG) without external deps: pure zlib/struct.
Design: blue→violet diagonal gradient rounded square + three floating "photo cards"."""
import zlib, struct, sys

W = H = 1024
R = 190  # corner radius

def lerp(a, b, t): return a + (b - a) * t

def in_round_rect(x, y, x0, y0, x1, y1, r):
    if x < x0 or x > x1 or y < y0 or y > y1: return False
    cx = min(max(x, x0 + r), x1 - r)
    cy = min(max(y, y0 + r), y1 - r)
    return (x - cx) ** 2 + (y - cy) ** 2 <= r * r or (x0 + r <= x <= x1 - r) or (y0 + r <= y <= y1 - r)

def px(x, y):
    m = 44  # margin
    if not in_round_rect(x, y, m, m, W - m, H - m, R):
        return (0, 0, 0, 0)
    t = (x + y) / (W + H)
    r = lerp(37, 124, t); g = lerp(99, 58, t); b = lerp(235, 237, t)
    # three photo cards ascending
    for i, (cx, cy, s, alpha) in enumerate([(400, 700, 230, 90), (560, 520, 270, 150), (520, 280, 320, 235)]):
        pad = s // 2
        if in_round_rect(x, y, cx - pad, cy - pad, cx + pad, cy + pad, 42):
            if alpha == 235:
                # top card: white with a gradient dot (sun) + hill silhouette
                inside_card = True
                lx, ly = x - (cx - pad), y - (cy - pad)
                # hill: circle below
                hill_c = (s * 0.62, s * 1.35)
                if (lx - hill_c[0]) ** 2 + (ly - hill_c[1]) ** 2 <= (s * 0.78) ** 2:
                    r, g, b = 96, 165, 250
                elif (lx - s * 0.30) ** 2 + (ly - s * 0.30) ** 2 <= (s * 0.115) ** 2:
                    r, g, b = 251, 191, 36
                else:
                    r, g, b = 255, 255, 255
                return (int(r), int(g), int(b), 255)
            else:
                # lower cards: translucent white
                r = lerp(r, 255, alpha / 255 * 0.55)
                g = lerp(g, 255, alpha / 255 * 0.55)
                b = lerp(b, 255, alpha / 255 * 0.55)
    return (int(r), int(g), int(b), 255)

rows = []
for y in range(H):
    row = bytearray([0])  # filter type 0
    for x in range(W):
        row.extend(px(x, y))
    rows.append(bytes(row))

def chunk(tag, data):
    c = struct.pack(">I", len(data)) + tag + data
    return c + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)

png = b"\x89PNG\r\n\x1a\n"
png += chunk(b"IHDR", struct.pack(">IIBBBBB", W, H, 8, 6, 0, 0, 0))
png += chunk(b"IDAT", zlib.compress(b"".join(rows), 9))
png += chunk(b"IEND", b"")

out = sys.argv[1] if len(sys.argv) > 1 else "app-icon.png"
with open(out, "wb") as f:
    f.write(png)
print(f"wrote {out} ({len(png)} bytes)")
