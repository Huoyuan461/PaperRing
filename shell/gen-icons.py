#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
纸环 PaperRing —— 从品牌主图生成 Tauri 图标套图。

输入:../frontend/editor/src/core/assets/brand/modern-logo/logo512.png(512x512 RGBA)
输出:本目录/icons/ 下 Tauri v2 标准的全套图标:
    32x32.png / 128x128.png / 128x128@2x.png(256) / icon.png(512)
    icon.ico(Windows,多尺寸) / icon.icns(macOS,多尺寸)

依赖:Pillow(pip install pillow)。重跑即可覆盖生成,幂等。
注:也可以用官方 `cargo tauri icon logo512.png` 一键生成同套图标,本脚本是
    无 Rust 工具链环境下的等价替代,产物格式与官方一致。
"""
import os
import sys

from PIL import Image

HERE = os.path.dirname(os.path.abspath(__file__))
SRC = os.path.normpath(
    os.path.join(
        HERE,
        "..",
        "frontend",
        "editor",
        "src",
        "core",
        "assets",
        "brand",
        "modern-logo",
        "logo512.png",
    )
)
OUT = os.path.join(HERE, "icons")

PNG_SIZES = {
    "32x32.png": 32,
    "128x128.png": 128,
    "128x128@2x.png": 256,
    "icon.png": 512,
}
ICO_SIZES = [(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)]
ICNS_SIZES = [(16, 16), (32, 32), (64, 64), (128, 128), (256, 256), (512, 512)]


def main() -> int:
    if not os.path.exists(SRC):
        print(f"[gen-icons] 找不到品牌主图:{SRC}", file=sys.stderr)
        return 1
    os.makedirs(OUT, exist_ok=True)
    src = Image.open(SRC).convert("RGBA")
    if src.size != (512, 512):
        print(f"[gen-icons] 警告:主图尺寸为 {src.size},期望 512x512,仍按比例缩放处理")

    for name, size in PNG_SIZES.items():
        src.resize((size, size), Image.LANCZOS).save(os.path.join(OUT, name))
        print(f"[gen-icons] {name}")

    # ICO:Pillow 以最大尺寸为底图、sizes 指定内嵌的全部尺寸
    src.save(os.path.join(OUT, "icon.ico"), format="ICO", sizes=ICO_SIZES)
    print("[gen-icons] icon.ico")

    try:
        src.save(os.path.join(OUT, "icon.icns"), format="ICNS", sizes=ICNS_SIZES)
        print("[gen-icons] icon.icns")
    except Exception as exc:  # 个别 Pillow 版本写 ICNS 受限,降级提示不中断
        print(f"[gen-icons] 警告:icon.icns 生成失败({exc}),可用 `cargo tauri icon` 补生成")

    print("[gen-icons] 完成 ->", OUT)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
