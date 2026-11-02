# -*- coding: utf-8 -*-
"""从一张方形原图生成 Copper 语言的图标（和 Float IDE 用同一套超椭圆圆角）。

用法：
    python scripts/make-copper-icon.py 原图.png

产物：
    src/assets/copper.png          界面用（关于页的 Copper logo，512）
    src-tauri/icons/copper.ico     Windows 文件关联用（16~256 多尺寸打包）

圆角规则与 Float IDE 完全一致：|x|^n + |y|^n = 1，n=5（苹果那种二阶贝塞尔手感），
圆角外面全透明——所以 .co 文件在资源管理器里看到的就是这只猫本身，没有白角。
"""

import io
import os
import sys
from PIL import Image, ImageDraw

N = 5.0   # 超椭圆指数，和 Float IDE 的 logo 相同
SS = 4    # 超采样倍数，边缘不锯齿


def squircle_mask(size: int) -> Image.Image:
    big = size * SS
    m = Image.new("L", (big, big), 0)
    d = ImageDraw.Draw(m)
    pts = []
    import math
    for i in range(720):
        t = 2.0 * math.pi * i / 720.0
        ct, st = math.cos(t), math.sin(t)
        x = big / 2.0 * (1 + (-1.0 if ct < 0 else 1.0) * abs(ct) ** (2.0 / N))
        y = big / 2.0 * (1 + (-1.0 if st < 0 else 1.0) * abs(st) ** (2.0 / N))
        pts.append((x, y))
    d.polygon(pts, fill=255)
    return m.resize((size, size), Image.LANCZOS)


def rounded(src: Image.Image, size: int) -> Image.Image:
    base = src.resize((size, size), Image.LANCZOS).convert("RGBA")
    out = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    out.paste(base, (0, 0), squircle_mask(size))
    return out


def ico_dib(img: Image.Image) -> bytes:
    """把一帧写成 ICO 里的 BMP 条目（32 位 BGRA + AND 掩码）。

    重要：Windows 只支持 256×256 用 PNG 压缩，小尺寸必须是 DIB，
    否则资源管理器会显示成一个白板——PIL 默认全写 PNG，所以这里自己打包。
    """
    import struct

    w, h = img.size
    px = img.convert("RGBA").load()
    xor = bytearray()
    for y in range(h - 1, -1, -1):
        for x in range(w):
            r, g, b, a = px[x, y]
            xor += bytes((b, g, r, a))
    row_bytes = ((w + 31) // 32) * 4
    and_mask = bytearray()
    for y in range(h - 1, -1, -1):
        row = bytearray(row_bytes)
        for x in range(w):
            if px[x, y][3] < 128:
                row[x >> 3] |= 0x80 >> (x & 7)
        and_mask += row
    bih = struct.pack(
        "<IiiHHIIiiII", 40, w, h * 2, 1, 32, 0, len(xor) + len(and_mask), 0, 0, 0, 0
    )
    return bih + bytes(xor) + bytes(and_mask)


def save_ico(path: str, master: Image.Image, sizes: list) -> None:
    import struct

    blobs = []
    for s in sizes:
        frame = master.resize((s, s), Image.LANCZOS)
        # 全部用 BMP 条目：Windows 对 PNG 压缩条目的支持看版本，BMP 全世界都认
        blobs.append((s, ico_dib(frame)))

    offset = 6 + 16 * len(blobs)
    out = bytearray(struct.pack("<HHH", 0, 1, len(blobs)))
    for s, blob in blobs:
        w = 0 if s >= 256 else s
        out += struct.pack("<BBBBHHII", w, w, 0, 0, 1, 32, len(blob), offset)
        offset += len(blob)
    for _, blob in blobs:
        out += blob
    with open(path, "wb") as f:
        f.write(bytes(out))


def main() -> None:
    if len(sys.argv) < 2:
        print("用法：python scripts/make-copper-icon.py 原图.png")
        raise SystemExit(2)
    src_path = sys.argv[1]
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    icons = os.path.join(root, "src-tauri", "icons")
    assets = os.path.join(root, "src", "assets")
    os.makedirs(icons, exist_ok=True)
    os.makedirs(assets, exist_ok=True)

    im = Image.open(src_path).convert("RGBA")
    w, h = im.size
    s = min(w, h)
    im = im.crop(((w - s) // 2, (h - s) // 2, (w - s) // 2 + s, (h - s) // 2 + s))
    print(f"原图 {w}×{h} → 裁成 {s}×{s}")

    master = rounded(im, 1024)
    master.resize((512, 512), Image.LANCZOS).save(os.path.join(assets, "copper.png"))
    master.resize((128, 128), Image.LANCZOS).save(os.path.join(icons, "copper-128.png"))
    save_ico(
        os.path.join(icons, "copper.ico"),
        master,
        [16, 24, 32, 48, 64, 128, 256],
    )
    print("已写出 src/assets/copper.png 与 src-tauri/icons/copper.ico")


if __name__ == "__main__":
    main()
