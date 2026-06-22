//! Throwaway measurement helper: report the vertical pixel extent of the light
//! "digit" colour vs a coloured symbol in a rendered tile PNG, so symbol↔digit
//! sizing can be checked numerically rather than by eye.
//!   cargo run --example measure -- path.png

fn main() {
    let path = std::env::args().nth(1).expect("png path");
    let img = image::open(&path).expect("open").to_rgba8();
    let (w, h) = (img.width(), img.height());

    let extent = |pred: &dyn Fn(u8, u8, u8) -> bool| -> Option<(u32, u32, u32, u32)> {
        let (mut x0, mut x1, mut y0, mut y1) = (u32::MAX, 0u32, u32::MAX, 0u32);
        let mut any = false;
        for y in 0..h {
            for x in 0..w {
                let p = img.get_pixel(x, y);
                if p[3] > 40 && pred(p[0], p[1], p[2]) {
                    any = true;
                    x0 = x0.min(x);
                    x1 = x1.max(x);
                    y0 = y0.min(y);
                    y1 = y1.max(y);
                }
            }
        }
        any.then_some((x0, x1, y0, y1))
    };

    // digit "5": light lavender (#cdd6f4) ; dot: orange (#ff6a2c)
    let digit = extent(&|r, g, b| r > 150 && g > 160 && b > 195);
    let orange = extent(&|r, g, b| r > 150 && g < 150 && b < 120 && r as i32 > g as i32 + 40);
    println!("image {w}x{h}");
    if let Some((x0, x1, y0, y1)) = digit {
        println!("digit  x[{x0}..{x1}] y[{y0}..{y1}]  height={}", y1 - y0 + 1);
    }
    if let Some((x0, x1, y0, y1)) = orange {
        println!(
            "symbol x[{x0}..{x1}] y[{y0}..{y1}]  height={} width={}",
            y1 - y0 + 1,
            x1 - x0 + 1
        );
    }
}
