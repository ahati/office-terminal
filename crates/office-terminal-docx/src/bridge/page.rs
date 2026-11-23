//! Page background mapping (PRD 15 §5.2, issue #40 §7.7.2).

use office_terminal_core::contracts::{
    VectorBackground, VectorGradient,
};
use oxml_layout::paint::Paint;

use super::paint::{average_color, gradient_angle_deg, map_gradient_stops, rgba};
use super::BridgeContext;

/// Map the page's optional background paint.
///
/// `None` keeps the renderer's normal transparent/white base. Approximations
/// (radial, grid, tile) warn once per document per the degradation policy.
pub(crate) fn map_background(paint: &Paint, ctx: &BridgeContext<'_>) -> Option<VectorBackground> {
    match paint {
        Paint::Solid(color) => Some(VectorBackground::Solid(rgba(color))),
        Paint::Linear {
            start, end, stops, ..
        } => Some(VectorBackground::Gradient(VectorGradient::new(
            gradient_angle_deg(start, end),
            map_gradient_stops(stops, 1.0),
        ))),
        Paint::Radial { stops, .. } => {
            ctx.warn_once("docx-background-radial", "page-background".to_string());
            // The core background model is linear-only; project the radial
            // stops onto a top-to-bottom gradient as the nearest honest
            // approximation (issue #40 §7.7.2).
            Some(VectorBackground::Gradient(VectorGradient::new(
                90.0,
                map_gradient_stops(stops, 1.0),
            )))
        }
        Paint::Grid { colors, .. } => {
            ctx.warn_once("docx-background-grid", "page-background".to_string());
            // An empty grid paints the paper colour rather than ink.
            Some(VectorBackground::Solid(
                average_color(colors, 1.0).unwrap_or_else(|| rgba(&oxml_layout::Color::WHITE)),
            ))
        }
        Paint::Tile { .. } => {
            // The engine's tile paint references a media id whose bytes do
            // not ride with the layout result; a background that cannot be
            // resolved degrades to the renderer's base, with the warning
            // keeping the loss visible.
            ctx.warn_once("docx-background-tile", "page-background".to_string());
            None
        }
    }
    // `Paint` is exhaustive: a future variant fails this match at compile
    // time and forces a conscious degradation arm here.
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{compile, FixtureEngine};
    use oxml_layout::paint::GradientStop;
    use oxml_layout::{Color, PageFrame, Point};

    #[test]
    fn background_helpers_are_shared_with_the_paint_module() {
        let start = Point { x: 0.0, y: 0.0 };
        let end = Point { x: 0.0, y: 100.0 };
        assert_eq!(gradient_angle_deg(&start, &end), 90.0);
        assert_eq!(
            average_color(&[Color::WHITE, Color::BLACK], 1.0),
            Some([128, 128, 128, 255])
        );
        assert_eq!(average_color(&[], 1.0), None);
    }

    fn page_with(background: Option<Paint>) -> PageFrame {
        let mut page = PageFrame::new(1, 612.0, 792.0, Vec::new());
        page.background = background;
        page
    }

    #[test]
    fn solid_backgrounds_map_directly() {
        let engine = FixtureEngine::new(Vec::new());
        let (slide, warnings) = compile(
            &page_with(Some(Paint::Solid(Color::WHITE))),
            &engine,
        );
        assert_eq!(slide.background, Some(VectorBackground::Solid([255, 255, 255, 255])));
        assert!(warnings.is_empty());
    }

    #[test]
    fn linear_backgrounds_keep_their_direction_and_stops() {
        let paint = Paint::linear(
            Point { x: 0.0, y: 0.0 },
            Point { x: 0.0, y: 100.0 },
            vec![
                GradientStop { offset: 0.0, color: Color::BLACK },
                GradientStop { offset: 1.0, color: Color::WHITE },
            ],
            (false, false),
        );
        let engine = FixtureEngine::new(Vec::new());
        let (slide, warnings) = compile(&page_with(Some(paint)), &engine);
        assert!(warnings.is_empty());
        let Some(VectorBackground::Gradient(gradient)) = slide.background else {
            panic!("expected a gradient background");
        };
        assert_eq!(gradient.angle_deg, 90.0);
        assert_eq!(gradient.stops.len(), 2);
        assert_eq!(gradient.stops[0].1, [0, 0, 0, 255]);
        assert_eq!(gradient.stops[1].1, [255, 255, 255, 255]);
    }

    #[test]
    fn radial_grid_and_tile_backgrounds_warn_and_approximate() {
        let radial = Paint::radial(
            Point { x: 50.0, y: 50.0 },
            50.0,
            Point { x: 50.0, y: 50.0 },
            vec![
                GradientStop { offset: 0.0, color: Color::BLACK },
                GradientStop { offset: 1.0, color: Color::WHITE },
            ],
            (false, false),
        );
        let grid = Paint::grid(
            oxml_layout::Rect {
                x: 0.0,
                y: 0.0,
                width: 10.0,
                height: 10.0,
            },
            2,
            2,
            vec![Color::WHITE, Color::BLACK, Color::BLACK, Color::WHITE],
        )
        .expect("valid grid");
        let tile = Paint::Tile {
            image: oxml_layout::MediaId(1),
            tile: oxml_layout::Rect {
                x: 0.0,
                y: 0.0,
                width: 10.0,
                height: 10.0,
            },
            transform: oxml_layout::Transform::IDENTITY,
        };

        let engine = FixtureEngine::new(Vec::new());
        let (slide, warnings) = compile(
            &page_with(Some(radial)),
            &engine,
        );
        assert!(matches!(slide.background, Some(VectorBackground::Gradient(_))));

        let (slide, more) = compile(&page_with(Some(grid)), &engine);
        assert_eq!(
            slide.background,
            Some(VectorBackground::Solid([128, 128, 128, 255]))
        );

        let (slide, tail) = compile(&page_with(Some(tile)), &engine);
        assert!(slide.background.is_none());

        assert_eq!(warnings.len(), 1);
        assert_eq!(more.len(), 1);
        assert_eq!(tail.len(), 1);
        for warning in [warnings, more, tail].concat() {
            let office_terminal_core::contracts::WarningEvent::FidelityDegraded {
                feature, ..
            } = warning
            else {
                panic!("unexpected warning {warning:?}");
            };
            assert!(feature.starts_with("docx-background-"));
        }
    }
}
