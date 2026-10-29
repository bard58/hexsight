"""Make the Hexsight icon files from one logo image.

Usage:
    py -3 tools/logo.py LOGO [--small SMALL] [--keep-bg] [--out DIR] [--preview FILE]

LOGO is a PNG, WebP or JPEG file, best 512 px or more. A transparent PNG works best.
When the image has no transparency, the script uses the color of the four corners as
the background and makes it transparent. Parts of the logo in that same color then
become transparent too: use a transparent PNG for such a logo.

    --small SMALL   a simpler cut of the logo for 16 and 24 px
    --keep-bg       keep the background (for a logo that is a full tile)
    --out DIR       the output folder (default: assets next to this tools folder)
    --preview FILE  also write a sheet with all sizes on dark and light backgrounds

Writes to the output folder:
    icon-16.png ... icon-256.png  the app icon at 7 sizes. The exe also uses icon-64.png
                                  for the window icon and the title mark.
    icon.ico                      the 7 sizes in one file (Explorer, taskbar, Start menu)
    social-preview.png            1280 x 640, for the GitHub repository card

Then build again: cargo build --release
Needs Pillow and numpy (pip install pillow numpy).
"""
import argparse
import struct
import sys
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFilter, ImageFont

SIZES = [16, 24, 32, 48, 64, 128, 256]
SMALL_UP_TO = 24  # the --small logo is used up to this size

NOISE = 8  # a pixel this close to the background color is background (JPEG and WebP noise)
SOLID = 40  # a logo color is at least this far from the background color
CORNERS_AGREE = 24  # the four corners differ by at most this much on a plain background

DARK = (17, 17, 19)  # the window background of the app
LIGHT = (242, 242, 247)
FONTS_BOLD = [r"C:\Windows\Fonts\seguisb.ttf", "DejaVuSans-Bold.ttf"]
FONTS_REGULAR = [r"C:\Windows\Fonts\segoeui.ttf", "DejaVuSans.ttf"]


def corner_color(im):
    """The background color: the color of the four corners, or None if they differ."""
    w, h = im.size
    corners = [im.getpixel(p)[:3] for p in [(0, 0), (w - 1, 0), (0, h - 1), (w - 1, h - 1)]]
    for c in corners[1:]:
        if max(abs(a - b) for a, b in zip(c, corners[0])) > CORNERS_AGREE:
            return None
    return tuple(sorted(c[i] for c in corners)[1] for i in range(3))


def drop_background(im):
    """Makes the background color transparent, with soft edges and no light halo."""
    bg = corner_color(im)
    if bg is None:
        sys.exit("The corners of the logo have different colors. Use a transparent PNG, or --keep-bg.")
    px = np.asarray(im.convert("RGB"), dtype=np.float32)
    dist = np.abs(px - np.array(bg, dtype=np.float32)).max(axis=2)
    # An edge pixel mixes the logo color and the background. The distance of the solid
    # logo color next to it tells how much of the pixel the logo covers.
    near = Image.fromarray(dist.astype(np.uint8)).filter(ImageFilter.MaxFilter(7))
    ratio = dist / np.maximum(np.asarray(near, dtype=np.float32), SOLID)
    alpha = np.clip((ratio - 0.04) / 0.92, 0.0, 1.0)
    alpha[dist <= NOISE] = 0.0
    # Take the background out of the edge colors, so a dark window shows no light ring.
    fg = bg + (px - bg) / np.maximum(alpha, 1e-3)[..., None]
    out = np.dstack([np.clip(fg, 0, 255), alpha * 255]).round().astype(np.uint8)
    return Image.fromarray(out, "RGBA")


def load(path, keep_bg):
    """The logo, cut to its content and made square, on a transparent background."""
    im = Image.open(path)
    im.load()
    im = im.convert("RGBA")
    if not keep_bg and im.getchannel("A").getextrema()[0] == 255:
        im = drop_background(im)
    box = im.getchannel("A").point(lambda a: 255 if a > 8 else 0).getbbox()
    if box is None:
        sys.exit(f"{path}: the logo is empty.")
    im = im.crop(box)
    side = max(im.size)
    square = Image.new("RGBA", (side, side))
    square.paste(im, ((side - im.width) // 2, (side - im.height) // 2))
    return square


def render(master, size, pad=None):
    """The logo at one size. Small sizes use every pixel, larger ones keep a thin margin."""
    if pad is None:
        pad = 0 if size <= SMALL_UP_TO else round(size / 32)
    inner = size - 2 * pad
    # Resize with premultiplied alpha, so transparent pixels add no dark or light fringe.
    logo = master.convert("RGBa").resize((inner, inner), Image.LANCZOS, reducing_gap=3.0).convert("RGBA")
    tile = Image.new("RGBA", (size, size))
    tile.paste(logo, (pad, pad))
    return tile


def pack_ico(pngs, path):
    """One .ico file with a PNG entry for each size (Windows Vista and later)."""
    header = struct.pack("<HHH", 0, 1, len(pngs))
    entries, blobs = [], []
    offset = 6 + 16 * len(pngs)
    for size, data in pngs:
        side = 0 if size >= 256 else size  # 0 means 256
        entries.append(struct.pack("<BBBBHHII", side, side, 0, 0, 1, 32, len(data), offset))
        blobs.append(data)
        offset += len(data)
    path.write_bytes(header + b"".join(entries) + b"".join(blobs))


def font(names, size):
    for name in names:
        try:
            return ImageFont.truetype(name, size)
        except OSError:
            pass
    return ImageFont.load_default(size)


def social(master, path):
    """The GitHub repository card: the logo, the name, and what the app is for."""
    im = Image.new("RGB", (1280, 640), DARK)
    logo = render(master, 300, pad=0)
    im.paste(logo, (130, 170), logo)
    draw = ImageDraw.Draw(im)
    draw.text((490, 318), "Hexsight", font=font(FONTS_BOLD, 128), fill=(245, 245, 247), anchor="ls")
    draw.text((494, 400), "ARAM Mayhem Helper", font=font(FONTS_REGULAR, 48), fill=(152, 152, 160), anchor="ls")
    im.save(path)


def preview(tiles, path):
    """All sizes on the dark and the light background, and the title bar mark at 1x and 1.5x."""
    gap = 24
    marks = [(tiles[64].resize((s, s), Image.LANCZOS), s) for s in (20, 30)]
    shown = [tiles[s] for s in reversed(SIZES)] + [m for m, _ in marks]
    width = gap + sum(t.width + gap for t in shown)
    row = 256 + 2 * gap
    sheet = Image.new("RGB", (width, 2 * row), DARK)
    sheet.paste(Image.new("RGB", (width, row), LIGHT), (0, row))
    small = font(FONTS_REGULAR, 13)
    draw = ImageDraw.Draw(sheet)
    for y, color in [(0, (152, 152, 160)), (row, (99, 99, 102))]:
        x = gap
        for t in shown:
            top = y + gap + (256 - t.height) // 2
            sheet.paste(t, (x, top), t)
            draw.text((x, y + row - 8), f"{t.width} px", font=small, fill=color, anchor="ls")
            x += t.width + gap
    sheet.save(path)


def main():
    here = Path(__file__).resolve().parent
    ap = argparse.ArgumentParser(description="Make the Hexsight icon files from one logo image.")
    ap.add_argument("logo", type=Path)
    ap.add_argument("--small", type=Path, help="a simpler cut of the logo for 16 and 24 px")
    ap.add_argument("--keep-bg", action="store_true", help="keep the background of the logo")
    ap.add_argument("--out", type=Path, default=here.parent / "assets")
    ap.add_argument("--preview", type=Path, help="also write a sheet with all sizes")
    args = ap.parse_args()

    master = load(args.logo, args.keep_bg)
    small = load(args.small, args.keep_bg) if args.small else master
    args.out.mkdir(parents=True, exist_ok=True)
    tiles, pngs = {}, []
    for size in SIZES:
        tile = render(small if size <= SMALL_UP_TO else master, size)
        path = args.out / f"icon-{size}.png"
        tile.save(path, optimize=True)
        tiles[size] = tile
        pngs.append((size, path.read_bytes()))
    pack_ico(pngs, args.out / "icon.ico")
    social(master, args.out / "social-preview.png")
    print(f"wrote icon-16.png ... icon-256.png, icon.ico and social-preview.png to {args.out}")
    if args.preview:
        preview(tiles, args.preview)
        print(f"wrote {args.preview}")


if __name__ == "__main__":
    main()
