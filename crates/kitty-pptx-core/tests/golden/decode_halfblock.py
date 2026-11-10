#!/usr/bin/env python3
"""Decode kitty-pptx halfblock ANSI output (▀ with 24-bit fg/bg) back to an RGB
grid and print a 4x4 average-color probe like probe_png.py.

Usage: decode_halfblock.py stream.bin
"""
import re
import sys


def decode(path):
    data = open(path, "rb").read()
    # SGR color state machine: 38;2;r;g;b (fg) and 48;2;r;g;b (bg)
    fg = (255, 255, 255)
    bg = (0, 0, 0)
    cells = []  # (top_rgb, bottom_rgb) per halfblock cell
    i = 0
    sgr = re.compile(rb"\x1b\[([0-9;]*)m")
    while i < len(data):
        m = sgr.match(data, i)
        if m:
            parts = m.group(1).split(b";") if m.group(1) else []
            j = 0
            while j < len(parts):
                if parts[j] == b"38" and j + 4 < len(parts) + 1 and parts[j + 1] == b"2":
                    fg = tuple(int(x) for x in parts[j + 2 : j + 5])
                    j += 5
                elif parts[j] == b"48" and parts[j + 1] == b"2":
                    bg = tuple(int(x) for x in parts[j + 2 : j + 5])
                    j += 5
                else:
                    j += 1
            i = m.end()
            continue
        ch = data[i : i + 3]
        if ch == "▀".encode():
            cells.append((fg, bg))
            i += 3
        elif ch == "\n".encode() or ch == b"\r":
            i += 1
        else:
            i += 1
    return cells


def main():
    cells = decode(sys.argv[1])
    # assume roughly square terminal: infer width from first row heuristics is
    # hard without newlines; instead treat as one long row and just probe by
    # fractional position over total cells.
    n = len(cells)
    print(f"cells: {n}")
    if not n:
        return
    side = int(n**0.5)
    # grid probe: split the cell list into 16 buckets and average both halves
    for row in range(4):
        out = []
        for col in range(4):
            lo = n * (row * 4 + col) // 16
            hi = n * (row * 4 + col + 1) // 16
            rs = gs = bs = cnt = 0
            for (top, bot) in cells[lo:hi]:
                for (r, g, b) in (top, bot):
                    rs += r; gs += g; bs += b; cnt += 1
            out.append(f"#{rs//cnt:02X}{gs//cnt:02X}{bs//cnt:02X}")
        print(" ".join(f"{c:9}" for c in out))


if __name__ == "__main__":
    main()
