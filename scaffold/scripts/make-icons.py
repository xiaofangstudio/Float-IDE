# -*- coding: utf-8 -*-
"""从一张方形原图生成 Float IDE 的全套图标（苹果式超椭圆圆角 + 外部透明）。

用法：
    python scripts/make-icons.py 原图.png

做的事：
1. 居中裁成正方形
2. 用超椭圆（squircle，|x|^n + |y|^n = 1，n=5）抠出苹果那种"二阶贝塞尔圆角"，
   圆角外面全部透明 —— 原图本身是 PNG，抠完就能直接用
3. 输出 Tauri 需要的全套：icon.png / 32 / 64 / 128 / 128@2x / Windows Store 各尺寸 / icon.ico
4. 同时给界面用的 src/assets/logo.png（内部图标，同样带圆角，不要直接用直角原图）

原理说明（为什么不用普通的 border-radius）：
  普通圆角是"直线 + 一段圆弧"，放大看拐角是突然开始的；
  苹果的图标是超椭圆，拐角从边上就缓慢过渡，视觉上更"饱满"。
  n=5 是最接近 iOS 图标手感的取值。
"""

import sys
import os
from PIL import Image, ImageDraw

N = 5.0          # 超椭圆指数（iOS 大约 4.5~5）
SS = 4           # 超采样倍数：先画 4 倍大再缩回去，边缘不会锯齿


def squircle_mask(size: int) -> Image.Image:
    """生成一张 size×size 的超椭圆蒙版（白=保留，黑=透明）"""
    big = size * SS
    m = Image.new("L", (big, big), 0)
    d = ImageDraw.Draw(m)
    # 用 720 个点逼近超椭圆，4 倍超采样后缩回去足够平滑
    pts = []
    for i in range(720):
        t = 2.0 * 3.141592653589793 * i / 720.0
        ct, st = os_cos(t), os_sin(t)
        x = big / 2.0 * (1 + sign(ct) * abs(ct) ** (2.0 / N))
        y = big / 2.0 * (1 + sign(st) * abs(st) ** (2.0 / N))
        pts.append((x, y))
    d.polygon(pts, fill=255)
    return m.resize((size, size), Image.LANCZOS)


def os_cos(t):
    import math
    return math.cos(t)


def os_sin(t):
    import math
    return math.sin(t)


def sign(v):
    return -1.0 if v < 0 else 1.0


def rounded(src: Image.Image, size: int) -> Image.Image:
    base = src.resize((size, size), Image.LANCZOS).convert("RGBA")
    mask = squircle_mask(size)
    out = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    out.paste(base, (0, 0), mask)
    return out


def main() -> None:
    if len(sys.argv) < 2:
        print("用法：python scripts/make-icons.py 原图.png")
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

    # 先做一张 1024 的基准，其余都从它缩（比从 778 反复重采样干净）
    master = rounded(im, 1024)
    master.save(os.path.join(icons, "icon.png"))
    master.resize((512, 512), Image.LANCZOS).save(os.path.join(assets, "logo.png"))

    for name, size in [
        ("32x32.png", 32),
        ("64x64.png", 64),
        ("128x128.png", 128),
        ("128x128@2x.png", 256),
        ("StoreLogo.png", 50),
        ("Square30x30Logo.png", 30),
        ("Square44x44Logo.png", 44),
        ("Square71x71Logo.png", 71),
        ("Square89x89Logo.png", 89),
        ("Square107x107Logo.png", 107),
        ("Square142x142Logo.png", 142),
        ("Square150x150Logo.png", 150),
        ("Square284x284Logo.png", 284),
        ("Square310x310Logo.png", 310),
    ]:
        master.resize((size, size), Image.LANCZOS).save(os.path.join(icons, name))

    # Windows 的 .ico：多尺寸打包，全部由带圆角的基准缩下来
    ico = master.resize((256, 256), Image.LANCZOS)
    ico.save(
        os.path.join(icons, "icon.ico"),
        format="ICO",
        sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)],
    )
    print("已写出 icons/ 全套 + src/assets/logo.png")


if __name__ == "__main__":
    main()
