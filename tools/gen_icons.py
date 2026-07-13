# Regenerate item/material icons: `uv run --with pillow python tools/gen_icons.py`
import os
import re
import glob
from PIL import Image

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
MAT_DIR = os.path.join(ROOT, "crates", "fallingsand_core", "content", "materials")
OUT = os.path.join(ROOT, "assets", "items")
OUT_MAT = os.path.join(OUT, "materials")
os.makedirs(OUT_MAT, exist_ok=True)

SKIP = {"AIR", "FLESH"}  # Empty phase / Player tag -> no inventory item


def parse_materials():
    result = {}
    for path in glob.glob(os.path.join(MAT_DIR, "*.material")):
        text = open(path, encoding="utf-8").read()
        for m in re.finditer(r"(\w+)\s*=\s*Material\s*\{", text):
            name = m.group(1)
            # bracket-match the first `colors: [ ... ]` after the name
            ci = text.find("colors:", m.end())
            if ci < 0:
                continue
            j = text.index("[", ci)
            depth = 0
            k = j
            while k < len(text):
                if text[k] == "[":
                    depth += 1
                elif text[k] == "]":
                    depth -= 1
                    if depth == 0:
                        break
                k += 1
            nums = [int(n) for n in re.findall(r"-?\d+", text[j:k + 1])]
            colors = [tuple(nums[i:i + 4]) for i in range(0, len(nums), 4)]
            result[name] = colors
    return result


def shade(c, f):
    return tuple(min(255, max(0, int(ch * f))) for ch in c[:3]) + (255,)


def material_tile(colors):
    img = Image.new("RGBA", (16, 16), (0, 0, 0, 0))
    n = len(colors)
    for y in range(16):
        for x in range(16):
            h = (x * 7 + y * 131 + x * y * 17) & 0xFF
            r, g, b = colors[h % n][:3]
            if x in (0, 15) or y in (0, 15):
                r, g, b = int(r * 0.55), int(g * 0.55), int(b * 0.55)
            elif x == 1 or y == 1:
                r, g, b = min(255, int(r * 1.18)), min(255, int(g * 1.18)), min(255, int(b * 1.18))
            img.putpixel((x, y), (r, g, b, 255))
    return img


def blank():
    return Image.new("RGBA", (16, 16), (0, 0, 0, 0))


def put(img, x, y, c):
    if 0 <= x < 16 and 0 <= y < 16:
        img.putpixel((x, y), c if len(c) == 4 else c + (255,))


def stick():
    img = blank()
    wood = (140, 96, 54)
    for t in range(10):
        x, y = 3 + t, 13 - t
        put(img, x, y - 1, shade(wood, 1.28))
        put(img, x, y, shade(wood, 1.0))
        put(img, x, y + 1, shade(wood, 0.6))
    put(img, 3, 13, shade(wood, 0.6))
    put(img, 12, 3, shade(wood, 1.28))
    return img


def ingot(base):
    img = blank()
    rows = {5: (6, 9), 6: (5, 10), 7: (4, 11), 8: (4, 11), 9: (4, 11)}
    for y, (a, b) in rows.items():
        for x in range(a, b + 1):
            if y == 5:
                c = shade(base, 1.3)
            elif y == 9:
                c = shade(base, 0.62)
            elif x in (a, b):
                c = shade(base, 0.5)
            else:
                c = shade(base, 1.0)
            put(img, x, y, c)
    put(img, 7, 6, (255, 255, 255, 255))
    put(img, 8, 6, shade(base, 1.45))
    return img


def pickaxe(head):
    img = blank()
    handle = (120, 80, 45)
    for (x, y) in [(8, 6), (8, 7), (9, 8), (9, 9), (9, 10), (10, 11), (10, 12), (10, 13)]:
        put(img, x, y, handle + (255,))
        put(img, x + 1, y, shade(handle, 0.62))
    head_px = [(3, 6), (4, 5), (5, 4), (6, 4), (7, 4), (8, 4),
               (9, 4), (10, 4), (11, 5), (12, 6), (13, 7)]
    for (x, y) in head_px:
        put(img, x, y - 1, shade(head, 1.3))
        put(img, x, y, shade(head, 1.0))
        put(img, x, y + 1, shade(head, 0.58))
    return img


def missing():
    img = blank()
    for y in range(16):
        for x in range(16):
            on = ((x // 4) + (y // 4)) % 2 == 0
            put(img, x, y, (214, 0, 214, 255) if on else (24, 24, 24, 255))
    return img


mats = parse_materials()
count = 0
for name, colors in mats.items():
    if name in SKIP or not colors:
        continue
    material_tile(colors).save(os.path.join(OUT_MAT, name.lower() + ".png"))
    count += 1

ingot((190, 198, 210)).save(os.path.join(OUT, "iron_ingot.png"))
ingot((236, 190, 48)).save(os.path.join(OUT, "gold_ingot.png"))
pickaxe((154, 105, 62)).save(os.path.join(OUT, "wooden_pickaxe.png"))
pickaxe((132, 134, 142)).save(os.path.join(OUT, "stone_pickaxe.png"))
pickaxe((190, 198, 210)).save(os.path.join(OUT, "iron_pickaxe.png"))
stick().save(os.path.join(OUT, "stick.png"))
missing().save(os.path.join(OUT, "missing.png"))

print(f"materials: {count}, plus 6 items + missing")
