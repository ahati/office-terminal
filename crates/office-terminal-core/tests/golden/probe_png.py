#!/usr/bin/env python3
"""Probe rendered slide PNGs: sample average color inside expected shape regions.

Usage: probe_png.py file.png [x_frac y_frac [x_frac y_frac ...]]
With no points: prints image size + a 4x4 grid of region average colors.
"""
import struct
import sys
import zlib


def decode_png(path):
    data = open(path, "rb").read()
    assert data[:8] == b"\x89PNG\r\n\x1a\n", "not a png"
    pos = 8
    width = height = None
    bit_depth = color_type = None
    idat = []
    palette = None
    while pos < len(data):
        (length,) = struct.unpack(">I", data[pos : pos + 4])
        tag = data[pos + 4 : pos + 8]
        payload = data[pos + 8 : pos + 8 + length]
        pos += 12 + length
        if tag == b"IHDR":
            width, height, bit_depth, color_type = struct.unpack(">IIBB", payload[:10])
        elif tag == b"PLTE":
            palette = [tuple(payload[i : i + 3]) for i in range(0, len(payload), 3)]
        elif tag == b"IDAT":
            idat.append(payload)
        elif tag == b"IEND":
            break
    raw = zlib.decompress(b"".join(idat))
    channels = {0: 1, 2: 3, 3: 1, 4: 2, 6: 4}[color_type]
    assert bit_depth == 8, f"unsupported depth {bit_depth}"
    stride = width * channels
    # defilter
    out = bytearray(width * height * channels)
    prev = bytearray(stride)
    src = 0
    for y in range(height):
        f = raw[src]
        src += 1
        line = bytearray(raw[src : src + stride])
        src += stride
        if f == 1:  # sub
            for i in range(channels, stride):
                line[i] = (line[i] + line[i - channels]) & 0xFF
        elif f == 2:  # up
            for i in range(stride):
                line[i] = (line[i] + prev[i]) & 0xFF
        elif f == 3:  # average
            for i in range(stride):
                a = line[i - channels] if i >= channels else 0
                line[i] = (line[i] + ((a + prev[i]) >> 1)) & 0xFF
        elif f == 4:  # paeth
            for i in range(stride):
                a = line[i - channels] if i >= channels else 0
                b = prev[i]
                c = prev[i - channels] if i >= channels else 0
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                pred = a if (pa <= pb and pa <= pc) else (b if pb <= pc else c)
                line[i] = (line[i] + pred) & 0xFF
        out[y * stride : (y + 1) * stride] = line
        prev = line
    return width, height, channels, bytes(out), palette


def avg_color(img, x0, y0, x1, y1):
    width, height, channels, px, _ = img
    x0, x1 = max(0, int(x0 * width)), min(width, int(x1 * width) or 1)
    y0, y1 = max(0, int(y0 * height)), min(height, int(y1 * height) or 1)
    if x1 <= x0 or y1 <= y0:
        return None
    r = g = b = a = n = 0
    for y in range(y0, y1):
        row = y * width * channels
        for x in range(x0, x1):
            o = row + x * channels
            if channels == 1:
                r += px[o]; g += px[o]; b += px[o]
                a += px[o + 1] if channels == 2 else 255
            else:
                r += px[o]; g += px[o + 1]; b += px[o + 2]
                a += px[o + 3] if channels == 4 else 255
            n += 1
    return (r // n, g // n, b // n, a // n)


def hexs(c):
    if not c:
        return "?"
    return f"#{c[0]:02X}{c[1]:02X}{c[2]:02X}"


def main():
    path = sys.argv[1]
    img = decode_png(path)
    w, h = img[0], img[1]
    print(f"{path}: {w}x{h}")
    pts = [(float(sys.argv[i]), float(sys.argv[i + 1])) for i in range(2, len(sys.argv), 2)]
    if not pts:
        for gy in range(4):
            row = []
            for gx in range(4):
                c = avg_color(img, gx / 4 + 0.01, gy / 4 + 0.01, (gx + 1) / 4 - 0.01, (gy + 1) / 4 - 0.01)
                row.append(f"{hexs(c):9}")
            print(" ".join(row))
    else:
        for (fx, fy) in pts:
            print(f"({fx:.3f},{fy:.3f}) -> {hexs(avg_color(img, fx - 0.01, fy - 0.01, fx + 0.01, fy + 0.01))}")


if __name__ == "__main__":
    main()
