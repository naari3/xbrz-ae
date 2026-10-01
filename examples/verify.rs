//! Offline helper for verifying the plugin output rendered by a host (e.g. AEXCompat).
//!
//!   verify gen <out.png> <w> <h>
//!       Deterministic pixel-art test image with transparency and semi-transparent pixels.
//!   verify expect <in.png> <factor> <expand|keep> <center_x> <center_y> <out_left> <out_top> <out_w> <out_h> <out.f32>
//!       Expected straight RGBA (f32, 0..1, little endian) for the output world whose top-left is at
//!       layer coordinate (out_left, out_top). Computed by scaling the *whole* input with xBRZ and then
//!       cropping / box filtering, independently of the plugin's request-rect logic.
//!       Prints the expected result_rect for a request of [0,w)x[0,h).
//!   verify cmp <expected.f32> <actual> <png|rgba16le|rgba32f-le> <w> <h>
//!       Compares and prints max abs difference in 8-bit units plus mismatch counts.

use std::fs;

use image::{Rgba, RgbaImage};
use xbrz_ae::core::{ScalerCfg, pack_u8, unpack, xbrz_scale};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("gen") => generate(&args[2], args[3].parse().unwrap(), args[4].parse().unwrap()),
        Some("enlarge") => enlarge(&args[2], args[3].parse().unwrap(), &args[4]),
        Some("expect") => expect(&args[2..]),
        Some("cmp") => compare(&args[2..]),
        _ => {
            eprintln!("usage: see source header");
            std::process::exit(2);
        }
    }
}

fn generate(path: &str, w: u32, h: u32) {
    let mut img = RgbaImage::new(w, h);
    let palette = [
        Rgba([20, 20, 30, 255]),
        Rgba([240, 200, 60, 255]),
        Rgba([200, 40, 40, 255]),
        Rgba([60, 120, 220, 255]),
        Rgba([90, 200, 90, 160]), // semi-transparent
    ];
    let (cx, cy) = (w as f64 * 0.4, h as f64 * 0.5);
    let r = w.min(h) as f64 * 0.3;
    for y in 0..h {
        for x in 0..w {
            let (fx, fy) = (x as f64 + 0.5, y as f64 + 0.5);
            let d = ((fx - cx).powi(2) + (fy - cy).powi(2)).sqrt();
            let p = if (d - r).abs() < 1.0 {
                palette[0] // outline
            } else if d < r {
                if (x / 3 + y / 3) % 2 == 0 { palette[1] } else { palette[2] }
            } else if (x as i64 - 2 * y as i64).rem_euclid(9) == 0 {
                palette[3] // steep diagonal lines
            } else if x > w * 3 / 4 && y < h / 3 {
                palette[4]
            } else {
                Rgba([0, 0, 0, 0])
            };
            img.put_pixel(x, y, p);
        }
    }
    // A few isolated single pixels.
    for (x, y) in [(1, 1), (w - 2, h - 2), (w / 2, 2)] {
        img.put_pixel(x, y, palette[1]);
    }
    img.save(path).unwrap();
}

fn enlarge(input: &str, factor: u32, output: &str) {
    let img = image::open(input).unwrap().to_rgba8();
    image::imageops::resize(&img, img.width() * factor, img.height() * factor, image::imageops::FilterType::Nearest)
        .save(output)
        .unwrap();
}

#[derive(PartialEq)]
enum Mode {
    Expand,
    Keep,
    ShrinkNearest,
    ShrinkAverage,
}

fn shrink_image(src: &[u32], w: i64, h: i64, f: i64, average: bool) -> (Vec<u32>, i64, i64) {
    let (bw, bh) = ((w + f - 1) / f, (h + f - 1) / f);
    let mut out = Vec::new();
    for by in 0..bh {
        for bx in 0..bw {
            let (x0, y0, x1, y1) = (bx * f, by * f, ((bx + 1) * f).min(w), ((by + 1) * f).min(h));
            if !average {
                let (x, y) = ((x0 + f / 2).min(x1 - 1), (y0 + f / 2).min(y1 - 1));
                out.push(src[(y * w + x) as usize]);
                continue;
            }
            let (mut a, mut r, mut g, mut b, mut n) = (0.0f32, 0.0f32, 0.0f32, 0.0f32, 0.0f32);
            for y in y0..y1 {
                for x in x0..x1 {
                    let c = unpack(src[(y * w + x) as usize]);
                    a += c.a;
                    r += c.r * c.a;
                    g += c.g * c.a;
                    b += c.b * c.a;
                    n += 1.0;
                }
            }
            let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
            out.push(if a <= 0.0 { 0 } else { pack_u8(q(a / n), q(r / a), q(g / a), q(b / a)) });
        }
    }
    (out, bw, bh)
}

fn expect(a: &[String]) {
    let img = image::open(&a[0]).unwrap().to_rgba8();
    let factor: usize = a[1].parse().unwrap();
    let mode = match a[2].as_str() {
        "expand" => Mode::Expand,
        "keep" => Mode::Keep,
        "shrink-nearest" => Mode::ShrinkNearest,
        "shrink-average" => Mode::ShrinkAverage,
        m => panic!("mode {m}"),
    };
    let keep = mode == Mode::Keep;
    let center_x: f64 = a[3].parse().unwrap();
    let center_y: f64 = a[4].parse().unwrap();
    let out_left: i64 = a[5].parse().unwrap();
    let out_top: i64 = a[6].parse().unwrap();
    let out_w: i64 = a[7].parse().unwrap();
    let out_h: i64 = a[8].parse().unwrap();
    let out_path = &a[9];

    let (w, h) = (img.width() as i64, img.height() as i64);
    let src: Vec<u32> = img.pixels().map(|p| pack_u8(p[3], p[0], p[1], p[2])).collect();
    let f = factor as i64;
    let (base, bw, bh) = match mode {
        Mode::ShrinkNearest => shrink_image(&src, w, h, f, false),
        Mode::ShrinkAverage => shrink_image(&src, w, h, f, true),
        _ => (src.clone(), w, h),
    };
    let scaled = xbrz_scale(factor, &ScalerCfg::default(), &base, bw as usize, bh as usize);
    let (sw, sh) = (bw * f, bh * f);

    let (off_x, off_y) = if mode != Mode::Expand {
        (0, 0)
    } else {
        ((center_x * (1.0 - f as f64)).round() as i64, (center_y * (1.0 - f as f64)).round() as i64)
    };
    let (l, t, r, b) = if keep { (0, 0, w, h) } else { (off_x, off_y, off_x + sw, off_y + sh) };
    println!(
        "expected_result_rect {} {} {} {}",
        l.max(0),
        t.max(0),
        r.min(w),
        b.min(h)
    );

    let mut out = Vec::with_capacity((out_w * out_h * 4) as usize);
    for oy in 0..out_h {
        for ox in 0..out_w {
            let (x, y) = (out_left + ox, out_top + oy);
            let px = if !keep {
                let (sx, sy) = (x - off_x, y - off_y);
                if sx < 0 || sy < 0 || sx >= sw || sy >= sh {
                    [0.0; 4]
                } else {
                    let c = unpack(scaled[(sy * sw + sx) as usize]);
                    [c.r, c.g, c.b, c.a]
                }
            } else if x < 0 || y < 0 || x >= w || y >= h {
                [0.0; 4]
            } else {
                let (mut sa, mut sr, mut sg, mut sb) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
                for yy in y * f..(y + 1) * f {
                    for xx in x * f..(x + 1) * f {
                        let c = unpack(scaled[(yy * sw + xx) as usize]);
                        let ca = c.a as f64;
                        sa += ca;
                        sr += c.r as f64 * ca;
                        sg += c.g as f64 * ca;
                        sb += c.b as f64 * ca;
                    }
                }
                if sa <= 0.0 {
                    [0.0; 4]
                } else {
                    [(sr / sa) as f32, (sg / sa) as f32, (sb / sa) as f32, (sa / (f * f) as f64) as f32]
                }
            };
            out.extend_from_slice(&px);
        }
    }
    let bytes: Vec<u8> = out.iter().flat_map(|v| v.to_le_bytes()).collect();
    fs::write(out_path, bytes).unwrap();
}

fn compare(a: &[String]) {
    let expected: Vec<f32> = fs::read(&a[0])
        .unwrap()
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
        .collect();
    let (w, h): (usize, usize) = (a[3].parse().unwrap(), a[4].parse().unwrap());
    let actual: Vec<f32> = match a[2].as_str() {
        "png" => {
            let img = image::open(&a[1]).unwrap().to_rgba8();
            assert_eq!((img.width() as usize, img.height() as usize), (w, h), "png size");
            img.into_raw().iter().map(|&v| v as f32 / 255.0).collect()
        }
        "rgba16le" => fs::read(&a[1])
            .unwrap()
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0)
            .collect(),
        "rgba32f-le" => fs::read(&a[1])
            .unwrap()
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
            .collect(),
        k => panic!("kind {k}"),
    };
    assert_eq!(expected.len(), w * h * 4, "expected size");
    assert_eq!(actual.len(), w * h * 4, "actual size");

    // Color of fully transparent pixels is irrelevant.
    let mut max_diff = 0.0f32;
    let mut over_half = 0usize;
    let mut over_one = 0usize;
    for (e, x) in expected.chunks_exact(4).zip(actual.chunks_exact(4)) {
        let channels = if e[3] == 0.0 && x[3] == 0.0 { 3..4 } else { 0..4 };
        for c in channels {
            let d = (e[c] - x[c]).abs() * 255.0;
            max_diff = max_diff.max(d);
            if d > 0.5 + 1e-3 {
                over_half += 1;
            }
            if d > 1.0 + 1e-3 {
                over_one += 1;
            }
        }
    }
    println!("max_diff_8bit {max_diff:.4} channels_over_0.5 {over_half} channels_over_1 {over_one}");
}
