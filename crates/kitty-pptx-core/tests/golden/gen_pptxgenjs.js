/* Generate pptxgenjs_sample.pptx — feature-rich generated deck for E2E
 * golden-image testing. Run normalize_pptx.py afterwards to make the archive
 * byte-stable. Slides:
 *   1. title text + core shapes (rect/ellipse/roundRect/star/arrow/triangle)
 *   2. solid background + extended presets (snip/callout/heart/cloud/teardrop)
 *   3. table + bullet list
 *   4. chart (bar) + chart (line)
 *   5. generated image (base64 PNG) + shape overlays
 *   6. text styling (sizes, colors, bold/italic, alignment, word-wrap paragraph)
 * Run: node gen_pptxgenjs.js <out.pptx>
 */
const fs = require("fs");
const PptxGenJS = require("pptxgenjs");

const pptx = new PptxGenJS();
pptx.defineLayout({ name: "W16x9", width: 13.333, height: 7.5 });
pptx.layout = "W16x9";
pptx.author = "golden";
pptx.title = "pptxgenjs golden fixture";

const EMU_IN = 914400;

// ---- slide 1: basics --------------------------------------------------
{
  const s = pptx.addSlide();
  s.background = { color: "FFFFFF" };
  s.addText("pptxgenjs Golden Fixture", {
    x: 0.9, y: 0.9, w: 11.5, h: 1.45,
    fontSize: 44, bold: true, color: "1F3864", fontFace: "Calibri",
  });
  s.addText("Solid background, core autoshapes, text", {
    x: 0.9, y: 2.35, w: 11.5, h: 0.5,
    fontSize: 20, color: "44546A", fontFace: "Calibri",
  });
  s.addShape(pptx.ShapeType.rect, { x: 0.9, y: 3.1, w: 2.5, h: 1.67, fill: { color: "C00000" } });
  s.addShape(pptx.ShapeType.ellipse, { x: 3.75, y: 3.1, w: 2.5, h: 1.67, fill: { color: "0070C0" } });
  s.addShape(pptx.ShapeType.roundRect, { x: 6.6, y: 3.1, w: 2.5, h: 1.67, fill: { color: "ED7D31" }, rectRadius: 0.12 });
  s.addShape(pptx.ShapeType.star5, { x: 9.45, y: 3.1, w: 2.5, h: 1.67, fill: { color: "70AD47" } });
  s.addShape(pptx.ShapeType.rightArrow, { x: 0.9, y: 5.3, w: 3.33, h: 1.25, fill: { color: "7030A0" } });
  s.addShape(pptx.ShapeType.triangle, { x: 4.67, y: 5.3, w: 1.67, h: 1.25, fill: { color: "00B0B0" } });
  s.addShape(pptx.ShapeType.rect, {
    x: 6.83, y: 5.3, w: 3.33, h: 1.25,
    fill: { color: "FFF2CC" }, line: { color: "BF8F00", width: 2.25 },
  });
}

// ---- slide 2: solid background + extended presets ---------------------
{
  const s = pptx.addSlide();
  s.background = { color: "0F2027" };
  s.addShape(pptx.ShapeType.rect, {
    x: 0, y: 0, w: 13.333, h: 7.5,
    fill: { type: "solid", color: "0F2027", transparency: 0 },
  });
  s.addText("Gradient + Extended Presets", {
    x: 0.9, y: 0.9, w: 11.5, h: 1.27,
    fontSize: 36, bold: true, color: "FFFFFF", fontFace: "Calibri",
  });
  s.addShape(pptx.ShapeType.snip1Rect, { x: 0.9, y: 2.5, w: 2.5, h: 1.67, fill: { color: "FFD966" } });
  s.addShape(pptx.ShapeType.wedgeRectCallout, {
    x: 3.75, y: 2.5, w: 3.0, h: 2.08, fill: { color: "F2F2F2" },
  });
  s.addText("Look here!", { x: 3.95, y: 2.8, w: 2.6, h: 0.8, fontSize: 16, color: "333333", fontFace: "Calibri" });
  s.addShape(pptx.ShapeType.heart, { x: 7.33, y: 2.5, w: 1.67, h: 1.67, fill: { color: "E91E63" } });
  s.addShape(pptx.ShapeType.cloud, { x: 9.5, y: 2.5, w: 2.5, h: 1.67, fill: { color: "D9E2F3" } });
  s.addShape(pptx.ShapeType.teardrop, { x: 0.9, y: 5.0, w: 1.25, h: 1.88, fill: { color: "00B0F0" } });
  s.addShape(pptx.ShapeType.sun, { x: 2.58, y: 5.0, w: 1.67, h: 1.67, fill: { color: "FFC000" } });
  s.addShape(pptx.ShapeType.gear6, { x: 4.67, y: 5.0, w: 1.67, h: 1.67, fill: { color: "808080" } });
  s.addShape(pptx.ShapeType.leftBrace, { x: 6.83, y: 4.58, w: 0.83, h: 2.5, fill: { color: "A6A6A6" } });
  s.addShape(pptx.ShapeType.chevron, { x: 8.33, y: 5.0, w: 2.5, h: 1.25, fill: { color: "FF6D00" } });
}

// ---- slide 3: table + bullets -----------------------------------------
{
  const s = pptx.addSlide();
  s.background = { color: "F7F9FC" };
  s.addText("Table and list rendering", {
    x: 0.9, y: 1.75, w: 7.0, h: 0.55, fontSize: 24, bold: true, color: "1F3864", fontFace: "Calibri",
  });
  s.addTable(
    [
      [
        { text: "Feature", options: { bold: true, color: "FFFFFF", fill: { color: "1F3864" } } },
        { text: "Status", options: { bold: true, color: "FFFFFF", fill: { color: "1F3864" } } },
      ],
      [
        { text: "Shapes", options: { color: "1F3864", fill: { color: "D6E4F0" } } },
        { text: "OK", options: { color: "1F3864", fill: { color: "D6E4F0" } } },
      ],
      [
        { text: "Tables", options: { color: "1F3864", fill: { color: "EDF2F9" } } },
        { text: "OK", options: { color: "1F3864", fill: { color: "EDF2F9" } } },
      ],
    ],
    { x: 0.9, y: 2.6, w: 7.0, h: 1.3, fontSize: 16, fontFace: "Calibri", border: { pt: 1, color: "FFFFFF" } }
  );
  s.addText(
    [
      { text: "First bullet point", options: { bullet: true, breakLine: true } },
      { text: "Second bullet point", options: { bullet: true, breakLine: true } },
      { text: "Third with more text to wrap around the box width", options: { bullet: true } },
    ],
    { x: 8.33, y: 2.25, w: 4.07, h: 4.0, fontSize: 18, color: "1F3864", fontFace: "Calibri" }
  );
}

// ---- slide 4: charts ---------------------------------------------------
{
  const s = pptx.addSlide();
  s.background = { color: "FFFFFF" };
  s.addText("Charts", {
    x: 0.9, y: 0.55, w: 11.5, h: 0.9, fontSize: 32, bold: true, color: "1F3864", fontFace: "Calibri",
  });
  s.addChart(pptx.ChartType.bar, [
    { name: "Series A", labels: ["Q1", "Q2", "Q3", "Q4"], values: [12, 28, 20, 35] },
    { name: "Series B", labels: ["Q1", "Q2", "Q3", "Q4"], values: [22, 15, 30, 18] },
  ], { x: 0.9, y: 1.8, w: 5.6, h: 4.8, barDir: "col", chartColors: ["4472C4", "ED7D31"], showLegend: true, legendPos: "b" });
  s.addChart(pptx.ChartType.line, [
    { name: "Trend", labels: ["Jan", "Feb", "Mar", "Apr", "May", "Jun"], values: [5, 12, 9, 17, 14, 21] },
  ], { x: 6.93, y: 1.8, w: 5.5, h: 4.8, chartColors: ["70AD47"], lineSize: 2.5, showLegend: false });
}

// ---- slide 5: image ----------------------------------------------------
{
  const s = pptx.addSlide();
  s.background = { color: "202124" };
  // deterministic 128x128 RGBA rainbow/checker PNG
  const W = 128, H = 128;
  const rows = [];
  for (let y = 0; y < H; y++) {
    const row = Buffer.alloc(1 + W * 4);
    row[0] = 0; // filter none
    for (let x = 0; x < W; x++) {
      let r = Math.round((x * 255) / (W - 1));
      let g = Math.round((y * 255) / (H - 1));
      let b = Math.round(((x + y) * 255) / (W + H - 2));
      if (((x >> 5) + (y >> 5)) % 2 === 0) {
        r = Math.min(255, r + 40); g = Math.min(255, g + 40); b = Math.min(255, b + 40);
      }
      if (x < 4 || y < 4 || x >= W - 4 || y >= H - 4) { r = 255; g = 255; b = 255; }
      const o = 1 + x * 4;
      row[o] = r; row[o + 1] = g; row[o + 2] = b; row[o + 3] = 255;
    }
    rows.push(row);
  }
  const zlib = require("zlib");
  function crc32(buf) {
    let c, crc = 0xffffffff;
    for (let n = 0; n < buf.length; n++) {
      c = (crc ^ buf[n]) & 0xff;
      for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
      crc = (crc >>> 8) ^ c;
    }
    return (crc ^ 0xffffffff) >>> 0;
  }
  function chunk(tag, data) {
    const len = Buffer.alloc(4); len.writeUInt32BE(data.length);
    const td = Buffer.concat([Buffer.from(tag), data]);
    const crc = Buffer.alloc(4); crc.writeUInt32BE(crc32(td));
    return Buffer.concat([len, td, crc]);
  }
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(W, 0); ihdr.writeUInt32BE(H, 4);
  ihdr[8] = 8; ihdr[9] = 6; ihdr[10] = 0; ihdr[11] = 0; ihdr[12] = 0;
  const png = Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", ihdr),
    chunk("IDAT", zlib.deflateSync(Buffer.concat(rows), { level: 9 })),
    chunk("IEND", Buffer.alloc(0)),
  ]);
  s.addImage({ data: "image/png;base64," + png.toString("base64"), x: 3.0, y: 1.5, w: 3.33, h: 3.33, sizing: { type: "crop", w: 3.33, h: 3.33 } });
  s.addShape(pptx.ShapeType.rect, { x: 6.8, y: 1.5, w: 3.33, h: 3.33, fill: { type: "none" }, line: { color: "FFD966", width: 3 } });
  s.addText("Embedded picture", { x: 3.0, y: 5.2, w: 6.67, h: 1.0, fontSize: 20, color: "FFD966", align: "center", fontFace: "Calibri" });
}

// ---- slide 6: text styling ----------------------------------------------
{
  const s = pptx.addSlide();
  s.background = { color: "FFFFFF" };
  s.addText("Text styling matrix", { x: 0.9, y: 0.55, w: 11.5, h: 0.9, fontSize: 32, bold: true, color: "1F3864" });
  const items = [
    { text: "Tiny 12", options: { fontSize: 12, color: "595959" } },
    { text: "Normal 18", options: { fontSize: 18, color: "404040" } },
    { text: "Big 28 bold", options: { fontSize: 28, bold: true, color: "C00000" } },
    { text: "Italic serif 20", options: { fontSize: 20, italic: true, color: "0070C0", fontFace: "Georgia" } },
    { text: "Underline 18", options: { fontSize: 18, underline: true, color: "7030A0" } },
    { text: "Left aligned", options: { fontSize: 16, align: "left", color: "333333" } },
    { text: "Center aligned", options: { fontSize: 16, align: "center", color: "333333" } },
    { text: "Right aligned", options: { fontSize: 16, align: "right", color: "333333" } },
    { text: "A fairly long paragraph that must word-wrap across several lines inside the fixed box so we can exercise the text layout engine end to end.", options: { fontSize: 16, color: "222222" } },
  ];
  s.addText(items, { x: 0.9, y: 1.8, w: 11.5, h: 5.0, fontFace: "Calibri", lineSpacingMultiple: 1.3 });
}

const out = process.argv[2] || "pptxgenjs_sample.pptx";
pptx.writeFile({ fileName: out }).then(() => console.log("wrote " + out));
