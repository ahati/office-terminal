# Bundled fonts

These files are bundled so layout, rasterisation and SVG outlines resolve the
same faces in a minimal container as on a desktop with Office fonts installed.
All three families are licensed under the SIL Open Font License 1.1; the full
license text and copyright notices are in [`OFL.txt`](./OFL.txt).

| file | family | upstream | copyright |
|---|---|---|---|
| `Carlito-*.ttf` | Carlito (metric-compatible with Calibri) | https://github.com/googlefonts/carlito | © 2013 The Carlito Project Authors; Reserved Font Name “Carlito” |
| `Caladea-*.ttf` | Caladea (metric-compatible with Cambria) | https://github.com/huertatipografica/Caladea | © 2012 The Caladea Project Authors |
| `NunitoSans-*.ttf` | Nunito Sans | https://fonts.google.com/specimen/Nunito+Sans, static instances served by Google Fonts | © 2016 The Nunito Sans Project Authors |

Bundled weights: Carlito and Caladea regular/bold/italic/bold-italic; Nunito
Sans 300/400/600/700 upright plus 400/700 italic.

The reference viewer loads Nunito Sans through `@fontsource/nunito-sans` for its
demo deck; Office faces come from the host. Bundling here makes our headless
renders exercise the same metrics instead of silently falling back to DejaVu.

OFL 1.1 requires that the fonts not be sold by themselves and that derivative
font names not use the reserved names. Files are redistributed unmodified under
their original names. The complete SIL Open Font License 1.1 text and the
copyright notices above are reproduced in `OFL.txt`; that file is included in
the repository and in release archives.
