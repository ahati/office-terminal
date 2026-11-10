#!/usr/bin/env python3
"""Deterministically generate the E2E golden-image fixture deck.

Produces e2e_deck.pptx (5 slides, 12192000x6858000 EMU / 16:9) plus the
embedded media image (image1.png). The output is byte-stable: fixed zip
timestamps, fixed part ordering, no clock reads.

Slides:
  1. basics      - solid white bg, title text, core autoshapes (rect,
                   ellipse, roundRect, star5, rightArrow, triangle)
  2. gradient    - linear gradient background + extended presets
                   (snip1Rect, wedgeRectCallout, heart, cloud, teardrop)
  3. table       - 3x2 filled table + bulleted text list
  4. picture     - embedded generated PNG with border
  5. math        - OMML equation (fraction + radical + superscript)

Usage: python3 gen_fixture.py [output.pptx]
"""

import os
import struct
import sys
import zipfile
import zlib

SLIDE_W = 12192000
SLIDE_H = 6858000

NS = (
    'xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" '
    'xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" '
    'xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" '
    'xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math"'
)

XML_DECL = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'


def png_bytes(width, height):
    """Deterministic RGBA test pattern: horizontal rainbow ramp blended with
    a 32px checkerboard, plus a white border frame."""
    rows = []
    for y in range(height):
        row = bytearray([0])  # filter type 0
        for x in range(width):
            r = (x * 255) // max(1, width - 1)
            g = (y * 255) // max(1, height - 1)
            b = ((x + y) * 255) // max(1, width + height - 2)
            if ((x // 32) + (y // 32)) % 2 == 0:
                r, g, b = min(255, r + 40), min(255, g + 40), min(255, b + 40)
            if x < 4 or y < 4 or x >= width - 4 or y >= height - 4:
                r, g, b = 255, 255, 255
            row += bytes((r, g, b, 255))
        rows.append(bytes(row))
    raw = b"".join(rows)

    def chunk(tag, data):
        return (
            struct.pack(">I", len(data))
            + tag
            + data
            + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
        )

    ihdr = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", ihdr)
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b"")
    )


def empty_sp_tree(shapes):
    return (
        "<p:spTree>"
        '<p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>'
        "<p:grpSpPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/>"
        '<a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr>'
        + "".join(shapes)
        + "</p:spTree>"
    )


def slide_xml(shapes, bg=""):
    return (
        XML_DECL
        + f"<p:sld {NS}>"
        + "<p:cSld>"
        + bg
        + f'<p:spTree>{empty_sp_tree(shapes)[len("<p:spTree>"):]}'
        + "</p:cSld>"
        + '<p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr>'
        + "</p:sld>"
    )


def shape(sid, name, geom, x, y, cx, cy, fill=None, line=None, adj=None, txbody=""):
    fill_xml = (
        f'<a:solidFill><a:srgbClr val="{fill}"/></a:solidFill>' if fill else "<a:noFill/>"
    )
    line_xml = (
        f'<a:ln w="{line[1]}"><a:solidFill><a:srgbClr val="{line[0]}"/></a:solidFill></a:ln>'
        if line
        else ""
    )
    av = ""
    if adj:
        av = "<a:avLst>" + "".join(
            f'<a:gd name="adj{n + 1}" fmla="val {v}"/>' for n, v in enumerate(adj)
        ) + "</a:avLst>"
    return (
        "<p:sp>"
        f'<p:nvSpPr><p:cNvPr id="{sid}" name="{name}"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr>'
        "<p:spPr>"
        f'<a:xfrm><a:off x="{x}" y="{y}"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm>'
        f'<a:prstGeom prst="{geom}">{av}</a:prstGeom>'
        f"{fill_xml}{line_xml}"
        "</p:spPr>"
        f"{txbody}"
        + "</p:sp>"
    )


def textbox(sid, x, y, cx, cy, paragraphs, anchor="t"):
    body = "".join(paragraphs)
    return (
        "<p:sp>"
        f'<p:nvSpPr><p:cNvPr id="{sid}" name="TextBox{sid}"/><p:cNvSpPr '
        'txBox="1"/><p:nvPr/></p:nvSpPr>'
        "<p:spPr>"
        f'<a:xfrm><a:off x="{x}" y="{y}"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm>'
        '<a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:noFill/>'
        "</p:spPr>"
        f'<p:txBody><a:bodyPr wrap="square" anchor="{anchor}"/><a:lstStyle/>{body}</p:txBody>'
        + "</p:sp>"
    )


def para(text, sz=1800, color="222222", bold=0, bullet=False, align=None, font="Calibri"):
    bu = '<a:buChar char="&#8226;"/>' if bullet else ""
    algn = f' algn="{align}"' if align else ""
    return (
        f'<a:p><a:pPr{algn}>'
        + ("<a:lnSpc><a:spcPct val=\"100000\"/></a:lnSpc>" if bullet else "")
        + bu
        + "</a:pPr>"
        + f'<a:r><a:rPr lang="en-US" sz="{sz}" b="{bold}" dirty="0">'
        + f'<a:latin typeface="{font}"/>'
        f'<a:solidFill><a:srgbClr val="{color}"/></a:solidFill></a:rPr>'
        f"<a:t>{text}</a:t></a:r></a:p>"
    )


# ---------------------------------------------------------------- slides --
def slide1():
    bg = (
        "<p:bg><p:bgPr><a:solidFill><a:srgbClr val=\"FFFFFF\"/></a:solidFill>"
        "<a:effectLst/></p:bgPr></p:bg>"
    )
    shapes = [
        textbox(
            2,
            838200,
            838200,
            10515600,
            1325563,
            [
                para("Golden Fixture Deck", sz=4400, color="1F3864", bold=1),
                para("Solid background, core autoshapes, text", sz=2000, color="44546A"),
            ],
        ),
        shape(3, "RedRect", "rect", 838200, 2286000, 2286000, 1524000, fill="C00000"),
        shape(
            4,
            "BlueEllipse",
            "ellipse",
            3429000,
            2286000,
            2286000,
            1524000,
            fill="0070C0",
        ),
        shape(
            5,
            "OrangeRoundRect",
            "roundRect",
            6019800,
            2286000,
            2286000,
            1524000,
            fill="ED7D31",
            adj=[16667],
        ),
        shape(
            6,
            "GreenStar",
            "star5",
            8610600 - 838200,
            2286000,
            2286000,
            1524000,
            fill="70AD47",
        ),
        shape(
            7,
            "PurpleArrow",
            "rightArrow",
            838200,
            4419600,
            3048000,
            1143000,
            fill="7030A0",
            adj=[50000, 50000],
        ),
        shape(
            8,
            "TealTriangle",
            "triangle",
            4267200,
            4419600,
            1524000,
            1143000,
            fill="00B0B0",
        ),
        shape(
            9,
            "OutlinedRect",
            "rect",
            6248400,
            4419600,
            3048000,
            1143000,
            fill="FFF2CC",
            line=("BF8F00", 28575),
        ),
    ]
    return slide_xml(shapes, bg)


def slide2():
    bg = (
        "<p:bg><p:bgPr>"
        "<a:gradFill><a:gsLst>"
        '<a:gs pos="0"><a:srgbClr val="0F2027"/></a:gs>'
        '<a:gs pos="100000"><a:srgbClr val="2C5364"/></a:gs>'
        '</a:gsLst><a:lin ang="5400000" scaled="1"/></a:gradFill>'
        "<a:effectLst/></p:bgPr></p:bg>"
    )
    shapes = [
        textbox(
            2,
            838200,
            838200,
            10515600,
            11591263,
            [
                para("Gradient + Extended Presets", sz=3600, color="FFFFFF", bold=1),
            ],
        ),
        shape(
            3, "Snip", "snip1Rect", 838200, 2286000, 2286000, 1524000,
            fill="FFD966", adj=[25000],
        ),
        shape(
            4, "Callout", "wedgeRectCallout", 3429000, 2286000, 2743200, 1905000,
            fill="F2F2F2", adj=[-20833, -20833],
            txbody=(
                "<p:txBody><a:bodyPr/><a:lstStyle/>"
                + para("Look here!", sz=1600, color="333333")
                + "</p:txBody>"
            ),
        ),
        shape(5, "Heart", "heart", 6705600, 2286000, 1524000, 1524000, fill="E91E63"),
        shape(6, "Cloud", "cloud", 8686800, 2286000, 2286000, 1524000, fill="D9E2F3"),
        shape(
            7, "Teardrop", "teardrop", 838200, 4572000, 1143000, 1714500,
            fill="00B0F0", adj=[50000],
        ),
        shape(8, "Sun", "sun", 2362200, 4572000, 1524000, 1524000, fill="FFC000"),
        shape(9, "Gear", "gear6", 4267200, 4572000, 1524000, 1524000, fill="808080"),
        shape(10, "Brace", "leftBrace", 6248400, 4191000, 762000, 2286000, fill="A6A6A6"),
        shape(11, "Chevron", "chevron", 7620000, 4572000, 2286000, 1143000, fill="FF6D00"),
    ]
    return slide_xml(shapes, bg)


def slide3():
    bg = (
        "<p:bg><p:bgPr><a:solidFill><a:srgbClr val=\"F7F9FC\"/></a:solidFill>"
        "<a:effectLst/></p:bgPr></p:bg>"
    )
    cell = lambda text, fill, color="FFFFFF": (
        f"<a:tc><a:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r>"
        f'<a:rPr lang="en-US" sz="1600" b="1"><a:solidFill><a:srgbClr val="{color}"/>'
        f"</a:solidFill></a:rPr><a:t>{text}</a:t></a:r></a:p></a:txBody>"
        f'<a:tcPr marL="91440" marR="91440" anchor="ctr">'
        f'<a:solidFill><a:srgbClr val="{fill}"/></a:solidFill>'
        f'<a:lnL w="12700"><a:solidFill><a:srgbClr val="FFFFFF"/></a:solidFill></a:lnL>'
        f'<a:lnR w="12700"><a:solidFill><a:srgbClr val="FFFFFF"/></a:solidFill></a:lnR>'
        f'<a:lnT w="12700"><a:solidFill><a:srgbClr val="FFFFFF"/></a:solidFill></a:lnT>'
        f'<a:lnB w="12700"><a:solidFill><a:srgbClr val="FFFFFF"/></a:solidFill></a:lnB>'
        "</a:tcPr></a:tc>"
    )
    table = (
        "<p:graphicFrame>"
        '<p:nvGraphicFramePr><p:cNvPr id="2" name="Table2"/>'
        '<p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr>'
        "<p:xfrm><a:off x=\"838200\"/><a:ext cx=\"6400800\" cy=\"1200000\"/></p:xfrm>"
        '<a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/table">'
        "<a:tbl><a:tblPr firstRow=\"1\" bandRow=\"1\">"
        '<a:solidFill><a:srgbClr val="1F3864"/></a:solidFill></a:tblPr>'
        '<a:tblGrid><a:gridCol w="3200400"/><a:gridCol w="3200400"/></a:tblGrid>'
        "<a:tr h=\"400000\">" + cell("Feature", "1F3864") + cell("Status", "1F3864") + "</a:tr>"
        "<a:tr h=\"400000\">" + cell("Shapes", "D6E4F0", "1F3864") + cell("OK", "D6E4F0", "1F3864") + "</a:tr>"
        "<a:tr h=\"400000\">" + cell("Tables", "EDF2F9", "1F3864") + cell("OK", "EDF2F9", "1F3864") + "</a:tr>"
        "</a:tbl></a:graphicData></a:graphic></p:graphicFrame>"
    )
    bullets = textbox(
        3,
        7620000,
        2057400,
        3718560,
        3657600,
        [
            para("First bullet point", sz=1800, color="1F3864", bullet=True),
            para("Second bullet point", sz=1800, color="1F3864", bullet=True),
            para("Third with more text to wrap around the box width", sz=1800, color="1F3864", bullet=True),
        ],
    )
    caption = textbox(
        4, 838200, 1600200, 6400800, 500000,
        [para("Table and list rendering", sz=2400, color="1F3864", bold=1)],
    )
    return slide_xml([caption, table, bullets], bg)


def slide4():
    bg = (
        "<p:bg><p:bgPr><a:solidFill><a:srgbClr val=\"202124\"/></a:solidFill>"
        "<a:effectLst/></p:bgPr></p:bg>"
    )
    pic = (
        "<p:pic>"
        '<p:nvPicPr><p:cNvPr id="2" name="Image1"/>'
        '<p:cNvPicPr/><p:nvPr/></p:nvPicPr>'
        "<p:blipFill><a:blip r:embed=\"rIdImg1\"/>"
        '<a:stretch><a:fillRect/></a:stretch></p:blipFill>'
        "<p:spPr>"
        '<a:xfrm><a:off x="2743200"/><a:ext cx="3048000" cy="3048000"/></a:xfrm>'
        '<a:prstGeom prst="rect"><a:avLst/></a:prstGeom>'
        '<a:ln w="38100"><a:solidFill><a:srgbClr val="FFD966"/></a:solidFill></a:ln>'
        "</p:spPr></p:pic>"
    )
    caption = textbox(
        3, 2743200, 6400800, 6096000, 914400,
        [para("Embedded picture", sz=2000, color="FFD966", align="ctr")],
    )
    return slide_xml([pic, caption], bg)


def slide5():
    bg = (
        "<p:bg><p:bgPr><a:solidFill><a:srgbClr val=\"FFFFFF\"/></a:solidFill>"
        "<a:effectLst/></p:bgPr></p:bg>"
    )
    m_run = lambda txt: (
        "<m:r><m:rPr><a:latin typeface=\"Cambria Math\"/></m:rPr>"
        f"<m:t>{txt}</m:t></m:r>"
    )
    math_para = (
        "<p:sp>"
        '<p:nvSpPr><p:cNvPr id="2" name="MathBox"/><p:cNvSpPr txBox="1"/><p:nvPr/></p:nvSpPr>'
        "<p:spPr>"
        '<a:xfrm><a:off x="1524000"/><a:ext cx="9144000" cy="2743200"/></a:xfrm>'
        '<a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:noFill/>'
        "</p:spPr>"
        "<p:txBody><a:bodyPr/><a:lstStyle/>"
        "<m:oMathPara><m:oMath>"
        "<m:sSup>"
        f"<m:e>{m_run('x')}</m:e>"
        "<m:sup><m:r><m:rPr><a:latin typeface=\"Cambria Math\"/></m:rPr><m:t>2</m:t></m:r></m:sup>"
        "</m:sSup>"
        f"{m_run('=')}"
        "<m:f>"
        f"<m:fPr><m:type m:val=\"bar\"/></m:fPr>"
        f"<m:num>{m_run('a')}</m:num>"
        f"<m:den><m:rad><m:radPr><m:degHide m:val=\"1\"/></m:radPr>"
        f"<m:deg/><m:e>{m_run('b')}</m:e></m:rad></m:den>"
        "</m:f>"
        f"{m_run('+')}"
        "<m:d><m:dPr><a:begChr m:val=\"[\"/><a:endChr m:val=\"]\"/></m:dPr>"
        f"<m:e>{m_run('c')}</m:e></m:d>"
        "</m:oMath></m:oMathPara>"
        "</p:txBody></p:sp>"
    )
    caption = textbox(
        3, 1524000, 1524000, 9144000, 914400,
        [para("OMML equation typesetting", sz=2400, color="1F3864", bold=1)],
    )
    return slide_xml([caption, math_para], bg)


# ------------------------------------------------------------ packaging --
CONTENT_TYPES = XML_DECL + (
    '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
    '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
    '<Default Extension="xml" ContentType="application/xml"/>'
    '<Default Extension="png" ContentType="image/png"/>'
    '<Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"/>'
    '<Override PartName="/ppt/slideMasters/slideMaster1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml"/>'
    '<Override PartName="/ppt/slideLayouts/slideLayout1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml"/>'
    + "".join(
        f'<Override PartName="/ppt/slides/slide{i}.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slide+xml"/>'
        for i in range(1, 6)
    )
    + '<Override PartName="/ppt/theme/theme1.xml" ContentType="application/vnd.openxmlformats-officedocument.theme+xml"/>'
    "</Types>"
)

ROOT_RELS = XML_DECL + (
    '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
    '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="ppt/presentation.xml"/>'
    "</Relationships>"
)

PRESENTATION = XML_DECL + (
    f"<p:presentation {NS}>"
    '<p:sldMasterIdLst><p:sldMasterId id="2147483648" r:id="rId1"/></p:sldMasterIdLst>'
    "<p:sldIdLst>"
    + "".join(f'<p:sldId id="{256 + i}" r:id="rId{2 + i}"/>' for i in range(5))
    + "</p:sldIdLst>"
    f'<p:sldSz cx="{SLIDE_W}" cy="{SLIDE_H}"/>'
    '<p:notesSz cx="6858000" cy="9144000"/>'
    "</p:presentation>"
)

PRES_RELS = XML_DECL + (
    '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
    '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster" Target="slideMasters/slideMaster1.xml"/>'
    + "".join(
        f'<Relationship Id="rId{2 + i}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide{i + 1}.xml"/>'
        for i in range(5)
    )
    + '<Relationship Id="rId7" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme" Target="theme/theme1.xml"/>'
    "</Relationships>"
)

MASTER = XML_DECL + (
    f"<p:sldMaster {NS}>"
    '<p:cSld name="Office">'
    + empty_sp_tree([])
    + "</p:cSld>"
    '<p:clrMap bg1="lt1" tx1="dk1" bg2="lt2" tx2="dk2" accent1="accent1" accent2="accent2" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" hlink="hlink" folHlink="folHlink"/>'
    "<p:sldLayoutIdLst>"
    '<p:sldLayoutId id="2147483649" r:id="rId1"/>'
    "</p:sldLayoutIdLst>"
    "<p:txStyles><p:titleStyle/><p:bodyStyle/><p:otherStyle/></p:txStyles>"
    "</p:sldMaster>"
)

MASTER_RELS = XML_DECL + (
    '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
    '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/>'
    '<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme" Target="../theme/theme1.xml"/>'
    "</Relationships>"
)

LAYOUT = XML_DECL + (
    f'<p:sldLayout {NS} type="blank">'
    '<p:cSld name="Blank">'
    + empty_sp_tree([])
    + "</p:cSld>"
    '<p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr>'
    "</p:sldLayout>"
)

LAYOUT_RELS = XML_DECL + (
    '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
    '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster" Target="../slideMasters/slideMaster1.xml"/>'
    "</Relationships>"
)

THEME = XML_DECL + (
    '<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" name="Office">'
    "<a:themeElements>"
    '<a:clrScheme name="Office">'
    '<a:dk1><a:sysClr val="windowText" lastClr="000000"/></a:dk1>'
    '<a:lt1><a:sysClr val="window" lastClr="FFFFFF"/></a:lt1>'
    '<a:dk2><a:srgbClr val="44546A"/></a:dk2>'
    '<a:lt2><a:srgbClr val="E7E6E6"/></a:lt2>'
    '<a:accent1><a:srgbClr val="4472C4"/></a:accent1>'
    '<a:accent2><a:srgbClr val="ED7D31"/></a:accent2>'
    '<a:accent3><a:srgbClr val="A5A5A5"/></a:accent3>'
    '<a:accent4><a:srgbClr val="FFC000"/></a:accent4>'
    '<a:accent5><a:srgbClr val="5B9BD5"/></a:accent5>'
    '<a:accent6><a:srgbClr val="70AD47"/></a:accent6>'
    '<a:hlink><a:srgbClr val="0563C1"/></a:hlink>'
    '<a:folHlink><a:srgbClr val="954F72"/></a:folHlink>'
    "</a:clrScheme>"
    '<a:fontScheme name="Office">'
    '<a:majorFont><a:latin typeface="Calibri Light"/><a:ea typeface=""/><a:cs typeface=""/></a:majorFont>'
    '<a:minorFont><a:latin typeface="Calibri"/><a:ea typeface=""/><a:cs typeface=""/></a:minorFont>'
    "</a:fontScheme>"
    '<a:fmtScheme name="Office">'
    "<a:fillStyleLst>"
    '<a:solidFill><a:schemeClr val="phClr"/></a:solidFill>'
    '<a:gradFill><a:gsLst><a:gs pos="0"><a:schemeClr val="phClr"/></a:gs><a:gs pos="100000"><a:schemeClr val="phClr"><a:lumMod val="60000"/></a:schemeClr></a:gs></a:gsLst><a:lin ang="5400000" scaled="0"/></a:gradFill>'
    "</a:fillStyleLst>"
    "<a:lnStyleLst>"
    '<a:ln w="6350"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln>'
    '<a:ln w="12700"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln>'
    '<a:ln w="19050"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln>'
    "</a:lnStyleLst>"
    "<a:effectStyleLst><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle></a:effectStyleLst>"
    '<a:bgFillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:bgFillStyleLst>'
    "</a:fmtScheme>"
    "</a:themeElements>"
    "</a:theme>"
)


def slide_rels(extra=""):
    return XML_DECL + (
        '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
        '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/>'
        + extra
        + "</Relationships>"
    )


def write_zip(path):
    slides = [slide1(), slide2(), slide3(), slide4(), slide5()]
    slide4_rels = slide_rels(
        '<Relationship Id="rIdImg1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="../media/image1.png"/>'
    )
    parts = {
        "[Content_Types].xml": CONTENT_TYPES,
        "_rels/.rels": ROOT_RELS,
        "ppt/presentation.xml": PRESENTATION,
        "ppt/_rels/presentation.xml.rels": PRES_RELS,
        "ppt/slideMasters/slideMaster1.xml": MASTER,
        "ppt/slideMasters/_rels/slideMaster1.xml.rels": MASTER_RELS,
        "ppt/slideLayouts/slideLayout1.xml": LAYOUT,
        "ppt/slideLayouts/_rels/slideLayout1.xml.rels": LAYOUT_RELS,
        "ppt/theme/theme1.xml": THEME,
        "ppt/media/image1.png": png_bytes(256, 256),
        "ppt/slides/_rels/slide4.xml.rels": slide4_rels,
    }
    for i, xml in enumerate(slides, 1):
        if i != 4:
            parts[f"ppt/slides/_rels/slide{i}.xml.rels"] = slide_rels()
        parts[f"ppt/slides/slide{i}.xml"] = xml

    with open(path, "wb") as fh:
        zf = zipfile.ZipFile(fh, "w", zipfile.ZIP_DEFLATED)
        for name in sorted(parts):
            info = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            zf.writestr(info, parts[name])
        zf.close()
    print(f"wrote {path} ({len(parts)} parts)")


if __name__ == "__main__":
    out = sys.argv[1] if len(sys.argv) > 1 else "e2e_deck.pptx"
    write_zip(out)
