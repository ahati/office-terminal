//! Decodes the demo PNGs with the `image` crate and prints region stats so
//! the composites can be checked without an image viewer.
//!
//! Usage: cargo run -p kitty-pptx-cli --example verify_pngs -- <dir>

use image::GenericImageView;

fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| "/tmp/mixed_demo".into());
    /// (file, placement boxes) as (col0, col1, row0, row1) in cells.
    type BoxList = [(u16, u16, u16, u16)];
    let frames: &[(&str, &BoxList)] = &[
        ("frame_00_anchor00.png", &[(11, 79, 0, 44), (81, 149, 0, 44)]),
        ("frame_01_anchor02.png", &[(11, 79, 0, 26), (81, 149, 0, 44)]),
        ("frame_02_anchor04.png", &[(11, 79, 0, 26), (81, 149, 0, 44)]),
        ("frame_03_anchor06.png", &[(11, 79, 0, 44), (81, 149, 0, 26)]),
        ("frame_04_anchor08.png", &[(11, 79, 0, 44), (81, 149, 0, 26)]),
        ("frame_05_anchor10.png", &[(11, 79, 0, 26), (81, 149, 0, 44)]),
        ("frame_06_anchor11.png", &[(46, 114, 0, 44)]),
        // The flip moment: row [1,2] nearly gone, [3,4] peeking at the
        // same columns; then the post-flip primary [3,4].
        ("glide_07.png", &[(11, 79, 5, 31), (81, 149, 5, 44)]),
        ("glide_08.png", &[(11, 79, 0, 26), (81, 149, 0, 44)]),
    ];

    for (name, boxes) in frames {
        let img = image::open(format!("{dir}/{name}")).expect("open png");
        let (w, h) = img.dimensions();
        assert_eq!((w, h), (1600, 900), "{name} wrong size {w}x{h}");
        let px = |x: u32, y: u32| -> image::Rgba<u8> { img.get_pixel(x, y) };
        let dark = |x: u32, y: u32| -> bool {
            let p = px(x, y);
            p[0] == 20 && p[1] == 23 && p[2] == 33
        };
        let mut parts = Vec::new();
        for &(c0, c1, r0, r1) in *boxes {
            let (x0, y0, x1, y1) = (c0 as u32 * 10, r0 as u32 * 20, c1 as u32 * 10, r1 as u32 * 20);
            let mut white = 0usize;
            let mut tot = 0usize;
            let mut edge_dark = 0usize;
            for y in (y0..y1).step_by(6) {
                for x in (x0..x1).step_by(4) {
                    let p = px(x, y);
                    tot += 1;
                    if p[0] > 200 && p[1] > 200 && p[2] > 200 {
                        white += 1;
                    }
                }
            }
            // content must not bleed past the box edge (sample just outside)
            for y in (y0..y1).step_by(8) {
                if dark(x1 + 4, y) {
                    edge_dark += 1;
                }
            }
            parts.push(format!(
                "[{c0}..{c1}x{r0}..{r1}] paper={:2.0}% edge-clean={}",
                100.0 * white as f32 / tot as f32,
                edge_dark == (y1 - y0).div_ceil(8) as usize
            ));
        }
        println!("{name}: {}", parts.join(" | "));
    }
    println!("verified");
}
