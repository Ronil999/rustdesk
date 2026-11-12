"""Renders the OpenDesk icon (same shapes as icon.svg) and writes every icon file the
Windows and Linux builds embed. Run from the repo root: python3 res/opendesk/gen-icons.py"""
import shutil
from PIL import Image, ImageDraw

S = 4096  # supersampled canvas; 4x the 1024 design grid


def k(v):
    return int(v * S / 1024)


def render():
    top, bottom = (0x3B, 0x82, 0xF6), (0x1E, 0x3A, 0x8A)
    grad = Image.new("RGBA", (S, S))
    gd = ImageDraw.Draw(grad)
    for i in range(0, 2 * S, 8):
        t = i / (2 * S)
        c = tuple(int(a + (b - a) * t) for a, b in zip(top, bottom)) + (255,)
        gd.line([(i, 0), (0, i)], fill=c, width=12)
    mask = Image.new("L", (S, S), 0)
    ImageDraw.Draw(mask).rounded_rectangle([k(32), k(32), k(992), k(992)], k(220), fill=255)
    img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    img.paste(grad, (0, 0), mask)
    d = ImageDraw.Draw(img)
    w = (255, 255, 255, 255)
    d.rounded_rectangle([k(184), k(224), k(840), k(680)], k(84), fill=w)
    d.rounded_rectangle([k(240), k(280), k(784), k(624)], k(28), fill=(0, 0, 0, 0))
    # refill the screen area with the background so the frame reads as an outline
    inner = Image.new("L", (S, S), 0)
    ImageDraw.Draw(inner).rounded_rectangle([k(240), k(280), k(784), k(624)], k(28), fill=255)
    img.paste(grad, (0, 0), inner)
    d = ImageDraw.Draw(img)
    d.rectangle([k(472), k(676), k(552), k(760)], fill=w)
    d.rounded_rectangle([k(352), k(748), k(672), k(804)], k(28), fill=w)
    arrow = [(452, 344), (452, 588), (512, 532), (556, 624), (600, 604), (556, 512), (640, 512)]
    d.polygon([(k(x), k(y)) for x, y in arrow], fill=w)
    return img


big = render()


def size(n):
    return big.resize((n, n), Image.LANCZOS)


size(1024).save("res/icon.png")
size(1024).save("res/mac-icon.png")
size(256).save("res/128x128@2x.png")
size(128).save("res/128x128.png")
size(64).save("res/64x64.png")
size(32).save("res/32x32.png")
ico = [(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)]
size(256).save("res/icon.ico", sizes=ico)
size(256).save("flutter/windows/runner/resources/app_icon.ico", sizes=ico)
size(64).save("res/tray-icon.ico", sizes=[(16, 16), (24, 24), (32, 32), (48, 48)])
shutil.copy("res/opendesk/icon.svg", "res/scalable.svg")
shutil.copy("res/opendesk/icon.svg", "flutter/assets/icon.svg")
