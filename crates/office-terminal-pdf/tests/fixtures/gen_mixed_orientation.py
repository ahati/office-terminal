#!/usr/bin/env python3
"""Generate a 12-page PDF with a random-feeling mix of page orientations.

Every page prints its box size, /Rotate value and page number, so the
rendered result is self-explanatory. Orientation mix:

  1  box 612x792  rot 0   -> displays PORTRAIT
  2  box 612x792  rot 0   -> displays PORTRAIT
  3  box 612x792  rot 90  -> displays LANDSCAPE (classic rotated figure)
  4  box 612x792  rot 0   -> displays PORTRAIT
  5  box 792x612  rot 0   -> displays LANDSCAPE (native landscape)
  6  box 792x612  rot 90  -> displays PORTRAIT  (vice versa)
  7  box 612x792  rot 0   -> displays PORTRAIT
  8  box 612x792  rot 90  -> displays LANDSCAPE
  9  box 612x792  rot 0   -> displays PORTRAIT
 10  box 792x612  rot 0   -> displays LANDSCAPE (native)
 11  box 612x792  rot 90  -> displays LANDSCAPE
 12  box 612x792  rot 0   -> displays PORTRAIT
"""

P = (612.0, 792.0)   # letter portrait box
L = (792.0, 612.0)   # letter landscape box

PAGES = [
    (P, 0), (P, 0), (P, 90), (P, 0),
    (L, 0), (L, 90), (P, 0), (P, 90),
    (P, 0), (L, 0), (P, 90), (P, 0),
]

objs = []  # list of (obj_id, bytes body)


def add(id_, body):
    objs.append((id_, body))


def content(box, rot, num):
    w, h = box
    banner_rgb = (0.10, 0.34, 0.62) if box == P else (0.80, 0.49, 0.09)
    ops = []
    # page background
    ops.append("1.00 1.00 1.00 rg 0 0 %.1f %.1f re f" % (w, h))
    # thick border
    ops.append("0.20 0.25 0.33 RG 6 w 18 18 %.1f %.1f re S" % (w - 36, h - 36))
    # orientation banner (in box coordinates; /Rotate pages show it sideways,
    # exactly like a real scanned/rotated figure page)
    ops.append("%.2f %.2f %.2f rg 18 %.1f %.1f 64 re f" % (banner_rgb + (h - 118, w - 36)))
    ops.append("1 1 1 rg BT /F1 34 Tf 34 %.1f Td (BOX %dx%d ROT %d) Tj ET" % (h - 78, w, h, rot))
    # giant page number, centered-ish
    ops.append("0.15 0.18 0.25 rg BT /F1 220 Tf 170 220 Td (%d) Tj ET" % num)
    # footer caption
    ops.append("0.35 0.38 0.45 rg BT /F1 22 Tf 34 44 Td (Office Terminal mixed-orientation fixture) Tj ET")
    if rot:
        ops.append("0.72 0.10 0.10 rg BT /F1 26 Tf 34 96 Td (/Rotate %d) Tj ET" % rot)
    return ("\n".join(ops) + "\n").encode("latin-1")


# object 1: catalog; 2: pages tree; 3: font; then per page: page + contents
add(1, b"<< /Type /Catalog /Pages 2 0 R >>")
kids = " ".join("%d 0 R" % (4 + 2 * i) for i in range(len(PAGES)))
add(2, ("<< /Type /Pages /Kids [%s] /Count %d >>" % (kids, len(PAGES))).encode())
add(3, b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>")

for i, (box, rot) in enumerate(PAGES):
    w, h = box
    num = i + 1
    page_id = 4 + 2 * i
    cont_id = 5 + 2 * i
    rot_entry = " /Rotate %d" % rot if rot else ""
    add(page_id, (
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 %.1f %.1f]%s "
        "/Resources << /Font << /F1 3 0 R >> >> /Contents %d 0 R >>"
        % (w, h, rot_entry, cont_id)
    ).encode())
    body = content(box, rot, num)
    add(cont_id, b"<< /Length %d >>\nstream\n" % len(body) + body + b"endstream")

out = bytearray(b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n")
offsets = {}
maxid = max(o for o, _ in objs)
for id_, body in sorted(objs):
    offsets[id_] = len(out)
    out += b"%d 0 obj\n" % id_ + body + b"\nendobj\n"
xref_at = len(out)
out += b"xref\n0 %d\n" % (maxid + 1)
out += b"0000000000 65535 f \n"
for id_ in range(1, maxid + 1):
    out += b"%010d 00000 n \n" % offsets[id_]
out += b"trailer\n<< /Size %d /Root 1 0 R >>\nstartxref\n%d\n%%%%EOF\n" % (maxid + 1, xref_at)

import sys
path = sys.argv[1] if len(sys.argv) > 1 else "/tmp/mixed_orientation.pdf"
with open(path, "wb") as f:
    f.write(bytes(out))
print("wrote %s (%d bytes, %d pages)" % (path, len(out), len(PAGES)))
