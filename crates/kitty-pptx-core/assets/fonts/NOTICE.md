# Bundled fonts

These files are bundled so layout, rasterisation and SVG outlines resolve the
same faces in a minimal container as on a desktop with Office fonts installed.
All three families are licensed under the SIL Open Font License 1.1.

| file | family | upstream |
|---|---|---|
| `Carlito-*.ttf` | Carlito (metric-compatible with Calibri) | https://github.com/googlefonts/carlito (OFL 1.1) |
| `Caladea-*.ttf` | Caladea (metric-compatible with Cambria) | https://github.com/googlefonts/caladea (OFL 1.1) |
| `NunitoSans-*.ttf` | Nunito Sans | https://fonts.google.com/specimen/Nunito+Sans (OFL 1.1), static instances served by Google Fonts |

Bundled weights: Carlito and Caladea regular/bold/italic/bold-italic; Nunito
Sans 300/400/600/700 upright plus 400/700 italic.

The reference viewer loads Nunito Sans through `@fontsource/nunito-sans` for its
demo deck; Office faces come from the host. Bundling here makes our headless
renders exercise the same metrics instead of silently falling back to DejaVu.

OFL 1.1 requires that the fonts not be sold by themselves and that derivative
font names not use the reserved names. Files are redistributed unmodified under
their original names; see the upstream repositories for the full license text.
