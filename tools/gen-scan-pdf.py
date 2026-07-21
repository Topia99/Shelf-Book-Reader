#!/usr/bin/env python3
"""P4-9 内存压测素材：生成 ~300MB 扫描版 PDF（每页一整张高分辨率位图、无文字层，
最吃内存的场景，用于验证 D9 渲染预算下峰值 <600MB、无 jetsam 终止）。

依赖（建议隔离 venv）：  python3 -m venv venv && ./venv/bin/pip install Pillow img2pdf numpy
用法：  ./venv/bin/python tools/gen-scan-pdf.py <输出.pdf> [页数=100]
注入模拟器：  SAMPLE_PDF=<输出.pdf> TITLE=压测 bash tools/seed-ios-test-book.sh <UDID>
观测：真机用 Instruments(Allocations/VM Tracker)；模拟器可 footprint -p <各 Shelf 相关进程> 求和。
"""
import io, os, sys
import numpy as np
from PIL import Image, ImageDraw
import img2pdf

OUT = sys.argv[1] if len(sys.argv) > 1 else "scan300.pdf"
PAGES = int(sys.argv[2]) if len(sys.argv) > 2 else 100
W, H = 2480, 3508  # A4 @ 300dpi

def make_page(n: int) -> bytes:
    # 浅色底 + 细噪点纹理（模拟扫描噪声，让 JPEG 体积到 ~3MB/页）
    noise = (np.random.default_rng(n).integers(205, 255, (H, W, 3))).astype("uint8")
    img = Image.fromarray(noise, "RGB")
    draw = ImageDraw.Draw(img)
    # 段落黑条模拟正文行
    y = 300
    for _ in range(46):
        x = 240
        for _ in range(np.random.randint(6, 11)):
            wlen = np.random.randint(120, 420)
            draw.rectangle([x, y, x + wlen, y + 34], fill=(20, 20, 20))
            x += wlen + 40
            if x > W - 300:
                break
        y += 64
    draw.text((W // 2 - 120, 120), f"SCAN STRESS PAGE {n+1}/{PAGES}", fill=(0, 0, 0))
    buf = io.BytesIO()
    img.save(buf, format="JPEG", quality=88)
    return buf.getvalue()

jpegs = []
for i in range(PAGES):
    jpegs.append(make_page(i))
    if (i + 1) % 20 == 0:
        print(f"  {i+1}/{PAGES} pages, cumulative ~{sum(len(j) for j in jpegs)/1e6:.0f}MB")

with open(OUT, "wb") as f:
    f.write(img2pdf.convert(jpegs))
print(f"done: {OUT}  {os.path.getsize(OUT)/1e6:.1f}MB  {PAGES} pages")
