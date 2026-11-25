# README screenshots

## PPTX previews

`presentation-view.png`, `presenter-view.png`, and `slide-sorter.png` are
documentation captures of the Office Terminal TUI running a demo presentation.

Source deck: *Intro to Neural Networks* (120 slides, 16:9), CERN Indico:
<https://indico.cern.ch/event/1477462/contributions/6515096/attachments/3090571/5473644/Intro%20to%20Neural%20Networks.pptx.pdf>

The project author states that the slide content is public domain or used under
fair use for documentation; all rights remain with the original authors.
Third-party images visible in the slides are credited in-slide (for example, the
machine-learning figures are credited to `scikit-learn.org`). If these
screenshots are reused outside this repository, verify the rights for the
underlying slide content independently.

## PDF reader

`pdf-reader.png` and `reader-help.png` show *The Cathedral and the Bazaar* by
Eric S. Raymond, version 3.0, copyright © 2000 Eric S. Raymond. The document is
distributed under the Open Publication License, version 2.0, as stated on its
copyright page. The PDF itself is not part of this repository.

`reader-help.png` overlays the reader-mode help modal on that spread.

## DOCX reader

`docx-reader.png` uses a synthetic Word document generated for this README. It
contains no third-party text or media. The document exercises styled headings,
mixed runs, lists, a shaded table, headers, footers and page fields.

## Capture note

The PDF and DOCX reader images are composited from the actual TUI frame and the
exact page rasters produced by the same build. The help-modal image uses a
literal capture of the modal over that reader frame. The headless Xvfb/Kitty
capture used for these screenshots does not preserve direct Kitty graphics
placements across redraws, so a literal frame grab shows the reader chrome and
page backgrounds but not the page bitmaps.
