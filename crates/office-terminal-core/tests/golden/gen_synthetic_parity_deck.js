#!/usr/bin/env node
// Generates the synthetic fidelity deck used by the visual-regression tests.
//
// The deck contains only generic text, shapes, a table and a chart. It does not
// embed third-party images, fonts or templates, so the generated `.pptx` and
// its rendered reference PNGs can be redistributed under the project licence.
//
// Usage (from this directory):
//   npm ci
//   node gen_synthetic_parity_deck.js
//
// The script writes `../fixtures/parity/synthetic-1.pptx`.

const path = require("path");
const PptxGenJS = require("pptxgenjs");

const OUT = path.resolve(__dirname, "..", "fixtures", "parity", "synthetic-1.pptx");

const pptx = new PptxGenJS();
pptx.layout = "LAYOUT_16x9";
pptx.author = "Office Terminal contributors";
pptx.company = "Office Terminal";
pptx.subject = "Synthetic visual-regression deck";
pptx.title = "Synthetic Fidelity Deck";
pptx.lang = "en-US";
pptx.theme = {
  headFontFace: "Calibri",
  bodyFontFace: "Calibri",
  lang: "en-US",
};

const INK = "111827";
const MUTED = "475569";
const ACCENT = "0F766E";
const ACCENT_2 = "EA580C";
const PAPER = "F8FAFC";

// Slide 1: title and geometric accents.
{
  const slide = pptx.addSlide();
  slide.background = { color: PAPER };
  slide.addShape(pptx.ShapeType.rect, {
    x: 0.6, y: 0.7, w: 1.4, h: 0.12,
    fill: { color: ACCENT }, line: { color: ACCENT, width: 0 },
  });
  slide.addShape(pptx.ShapeType.rect, {
    x: 2.0, y: 0.7, w: 0.55, h: 0.12,
    fill: { color: ACCENT_2 }, line: { color: ACCENT_2, width: 0 },
  });
  slide.addText("Synthetic Fidelity Deck", {
    x: 0.6, y: 1.25, w: 8.8, h: 1.0,
    fontFace: "Calibri", fontSize: 40, bold: true, color: INK,
    margin: 0,
  });
  slide.addText("Generic shapes, text, tables and charts — no third-party media", {
    x: 0.6, y: 2.35, w: 8.8, h: 0.5,
    fontFace: "Calibri", fontSize: 18, color: MUTED,
    margin: 0,
  });
  slide.addShape(pptx.ShapeType.ellipse, {
    x: 7.4, y: 3.35, w: 1.6, h: 1.6,
    fill: { color: "CCFBF1" }, line: { color: ACCENT, width: 2 },
  });
  slide.addShape(pptx.ShapeType.roundRect, {
    x: 6.4, y: 3.95, w: 1.35, h: 0.8,
    fill: { color: "FFEDD5" }, line: { color: ACCENT_2, width: 2 },
    rectRadius: 0.12,
  });
  slide.addText("office-terminal / ot", {
    x: 0.6, y: 4.85, w: 8.8, h: 0.35,
    fontFace: "Calibri", fontSize: 11, color: MUTED,
    margin: 0,
  });
}

// Slide 2: shape coverage and mixed text styling.
{
  const slide = pptx.addSlide();
  slide.background = { color: "FFFFFF" };
  slide.addText("Shape and text coverage", {
    x: 0.6, y: 0.45, w: 8.8, h: 0.65,
    fontFace: "Calibri", fontSize: 28, bold: true, color: INK,
    margin: 0,
  });
  slide.addText(
    [
      { text: "Vector shapes", options: { bullet: true } },
      { text: "Solid, gradient and outline fills", options: { bullet: true } },
      { text: "Rounded corners and rotated geometry", options: { bullet: true } },
      { text: "Bold, italic and mixed-run text", options: { bullet: true } },
    ],
    {
      x: 0.6, y: 1.45, w: 4.4, h: 2.9,
      fontFace: "Calibri", fontSize: 18, color: INK,
      paraSpaceAfter: 10, margin: 0,
    }
  );
  slide.addText(
    [
      { text: "Bold ", options: { bold: true, color: INK } },
      { text: "italic ", options: { italic: true, color: ACCENT } },
      { text: "regular", options: { color: MUTED } },
    ],
    {
      x: 0.6, y: 4.45, w: 4.4, h: 0.4,
      fontFace: "Calibri", fontSize: 16, margin: 0,
    }
  );
  slide.addShape(pptx.ShapeType.rect, {
    x: 5.4, y: 1.45, w: 1.5, h: 1.1,
    fill: { color: ACCENT }, line: { color: "0F172A", width: 1.5 },
  });
  slide.addShape(pptx.ShapeType.roundRect, {
    x: 7.15, y: 1.45, w: 1.7, h: 1.1,
    fill: { color: "CCFBF1" }, line: { color: ACCENT, width: 2 },
    rectRadius: 0.18,
  });
  slide.addShape(pptx.ShapeType.triangle, {
    x: 5.4, y: 2.95, w: 1.5, h: 1.25,
    fill: { color: ACCENT_2 }, line: { color: "7C2D12", width: 1.5 },
  });
  slide.addShape(pptx.ShapeType.ellipse, {
    x: 7.15, y: 2.95, w: 1.7, h: 1.25,
    fill: { color: "FFEDD5" }, line: { color: ACCENT_2, width: 2 },
  });
  slide.addShape(pptx.ShapeType.line, {
    x: 5.4, y: 4.55, w: 3.4, h: 0,
    line: { color: INK, width: 2 },
  });
  slide.addShape(pptx.ShapeType.line, {
    x: 5.4, y: 4.75, w: 3.4, h: 0,
    line: { color: ACCENT, width: 1.5, dashType: "dash" },
  });
}

// Slide 3: table and chart coverage.
{
  const slide = pptx.addSlide();
  slide.background = { color: PAPER };
  slide.addText("Table and chart coverage", {
    x: 0.6, y: 0.45, w: 8.8, h: 0.65,
    fontFace: "Calibri", fontSize: 28, bold: true, color: INK,
    margin: 0,
  });
  slide.addTable(
    [
      [
        { text: "Region", options: { bold: true, color: "FFFFFF", fill: { color: ACCENT } } },
        { text: "Q1", options: { bold: true, color: "FFFFFF", fill: { color: ACCENT } } },
        { text: "Q2", options: { bold: true, color: "FFFFFF", fill: { color: ACCENT } } },
      ],
      ["North", "12", "18"],
      ["South", "9", "14"],
      ["West", "7", "11"],
    ],
    {
      x: 0.6, y: 1.45, w: 4.4, h: 2.6,
      colW: [2.0, 1.2, 1.2],
      border: { pt: 1, color: "CBD5E1" },
      fill: "FFFFFF", color: INK, fontFace: "Calibri", fontSize: 12,
      align: "center", valign: "middle",
    }
  );
  slide.addChart(
    pptx.ChartType.bar,
    [{ name: "Q2", labels: ["North", "South", "West"], values: [18, 14, 11] }],
    {
      x: 5.3, y: 1.45, w: 4.1, h: 3.2,
      showLegend: false,
      showTitle: true,
      title: "Q2 by region",
      chartColors: [ACCENT],
      catAxisLabelColor: MUTED,
      valAxisLabelColor: MUTED,
      catAxisLabelFontFace: "Calibri",
      valAxisLabelFontFace: "Calibri",
      catAxisLabelFontSize: 11,
      valAxisLabelFontSize: 11,
      titleFontFace: "Calibri",
      titleFontSize: 14,
      titleColor: INK,
    }
  );
  slide.addText("Generated by tests/golden/gen_synthetic_parity_deck.js", {
    x: 0.6, y: 4.85, w: 8.8, h: 0.35,
    fontFace: "Calibri", fontSize: 10, color: MUTED,
    margin: 0,
  });
}

pptx.writeFile({ fileName: OUT })
  .then(() => console.log(`wrote ${OUT}`))
  .catch((err) => {
    console.error(err);
    process.exitCode = 1;
  });
