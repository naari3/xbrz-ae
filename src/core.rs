
use std::ffi::c_int;

unsafe extern "C" {
    fn xbrz_ae_scale_argb(
        factor: usize,
        src: *const u32,
        trg: *mut u32,
        src_width: c_int,
        src_height: c_int,
        equal_color_tolerance: f64,
        center_direction_bias: f64,
        dominant_direction_threshold: f64,
        steep_direction_threshold: f64,
        y_first: c_int,
        y_last: c_int,
    );
}

pub const FACTOR_MIN: usize = 2;
pub const FACTOR_MAX: usize = 6;

pub const SOURCE_MARGIN: i32 = 3;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScalerCfg {
    pub equal_color_tolerance: f64,
    pub center_direction_bias: f64,
    pub dominant_direction_threshold: f64,
    pub steep_direction_threshold: f64,
}

impl Default for ScalerCfg {
    fn default() -> Self {
        Self {
            equal_color_tolerance: 30.0,
            center_direction_bias: 4.0,
            dominant_direction_threshold: 3.6,
            steep_direction_threshold: 2.4,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl IRect {
    pub const EMPTY: IRect = IRect { left: 0, top: 0, right: 0, bottom: 0 };

    pub fn is_empty(&self) -> bool {
        self.right <= self.left || self.bottom <= self.top
    }
    pub fn intersect(&self, o: &IRect) -> IRect {
        let r = IRect {
            left: self.left.max(o.left),
            top: self.top.max(o.top),
            right: self.right.min(o.right),
            bottom: self.bottom.min(o.bottom),
        };
        if r.is_empty() { IRect::EMPTY } else { r }
    }
    pub fn inflate(&self, d: i32) -> IRect {
        IRect { left: self.left - d, top: self.top - d, right: self.right + d, bottom: self.bottom + d }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputMode {
    Expand,
    KeepSize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mapping {
    pub factor: i32,
    pub mode: OutputMode,
    pub offset_x: i32,
    pub offset_y: i32,
}

impl Mapping {
    pub fn new(factor: usize, mode: OutputMode, center_x: f64, center_y: f64) -> Self {
        let f = factor as i32;
        let (offset_x, offset_y) = match mode {
            OutputMode::Expand => (
                (center_x * (1.0 - f as f64)).round() as i32,
                (center_y * (1.0 - f as f64)).round() as i32,
            ),
            OutputMode::KeepSize => (0, 0),
        };
        Self { factor: f, mode, offset_x, offset_y }
    }

    pub fn output_rect(&self, input: IRect) -> IRect {
        if input.is_empty() {
            return IRect::EMPTY;
        }
        match self.mode {
            OutputMode::Expand => IRect {
                left: self.offset_x + self.factor * input.left,
                top: self.offset_y + self.factor * input.top,
                right: self.offset_x + self.factor * input.right,
                bottom: self.offset_y + self.factor * input.bottom,
            },
            OutputMode::KeepSize => input,
        }
    }

    pub fn input_rect_for(&self, output: IRect) -> IRect {
        if output.is_empty() {
            return IRect::EMPTY;
        }
        let needed = match self.mode {
            OutputMode::Expand => IRect {
                left: (output.left - self.offset_x).div_euclid(self.factor),
                top: (output.top - self.offset_y).div_euclid(self.factor),
                right: div_ceil(output.right - self.offset_x, self.factor),
                bottom: div_ceil(output.bottom - self.offset_y, self.factor),
            },
            OutputMode::KeepSize => output,
        };
        needed.inflate(SOURCE_MARGIN)
    }
}

fn div_ceil(a: i32, b: i32) -> i32 {
    -((-a).div_euclid(b))
}

pub fn xbrz_scale(factor: usize, cfg: &ScalerCfg, src: &[u32], width: usize, height: usize) -> Vec<u32> {
    assert!((FACTOR_MIN..=FACTOR_MAX).contains(&factor));
    assert_eq!(src.len(), width * height);
    let mut dst = vec![0u32; width * height * factor * factor];
    if width == 0 || height == 0 {
        return dst;
    }

    const MIN_ROWS: usize = 16;
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
    let chunks = threads.min(height.div_ceil(MIN_ROWS)).max(1);
    let rows_per_chunk = height.div_ceil(chunks);

    // Each slice [y_first, y_last) writes only to destination rows [factor*y_first, factor*y_last),
    // so the ranges are disjoint (documented THREAD-SAFETY contract of xbrz::scale).
    let dst_ptr = SendPtr(dst.as_mut_ptr());
    let call = |y_first: usize, y_last: usize| unsafe {
        xbrz_ae_scale_argb(
            factor,
            src.as_ptr(),
            dst_ptr.get(),
            width as c_int,
            height as c_int,
            cfg.equal_color_tolerance,
            cfg.center_direction_bias,
            cfg.dominant_direction_threshold,
            cfg.steep_direction_threshold,
            y_first as c_int,
            y_last as c_int,
        )
    };

    if chunks == 1 {
        call(0, height);
    } else {
        std::thread::scope(|s| {
            for i in 0..chunks {
                let y_first = i * rows_per_chunk;
                let y_last = ((i + 1) * rows_per_chunk).min(height);
                if y_first < y_last {
                    s.spawn(move || call(y_first, y_last));
                }
            }
        });
    }
    dst
}

#[derive(Clone, Copy)]
struct SendPtr(*mut u32);
unsafe impl Send for SendPtr {}
unsafe impl Sync for SendPtr {}
impl SendPtr {
    fn get(self) -> *mut u32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rgba {
    pub a: f32,
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

pub fn unpack(p: u32) -> Rgba {
    Rgba {
        a: ((p >> 24) & 0xff) as f32 / 255.0,
        r: ((p >> 16) & 0xff) as f32 / 255.0,
        g: ((p >> 8) & 0xff) as f32 / 255.0,
        b: (p & 0xff) as f32 / 255.0,
    }
}

pub fn pack_u8(a: u8, r: u8, g: u8, b: u8) -> u32 {
    (a as u32) << 24 | (r as u32) << 16 | (g as u32) << 8 | b as u32
}

pub struct Scaled {
    pub pixels: Vec<u32>,
    pub width: i32,
    pub height: i32,
    pub factor: i32,
    pub mode: OutputMode,
    pub origin_x: i32,
    pub origin_y: i32,
}

impl Scaled {
    pub fn new(pixels: Vec<u32>, src_w: usize, src_h: usize, input_rect: IRect, map: &Mapping) -> Self {
        let f = map.factor;
        let (origin_x, origin_y) = match map.mode {
            OutputMode::Expand => (map.offset_x + f * input_rect.left, map.offset_y + f * input_rect.top),
            OutputMode::KeepSize => (input_rect.left, input_rect.top),
        };
        Self {
            pixels,
            width: src_w as i32 * f,
            height: src_h as i32 * f,
            factor: f,
            mode: map.mode,
            origin_x,
            origin_y,
        }
    }

    pub fn sample(&self, x: i32, y: i32) -> Rgba {
        match self.mode {
            OutputMode::Expand => {
                let sx = x - self.origin_x;
                let sy = y - self.origin_y;
                if sx < 0 || sy < 0 || sx >= self.width || sy >= self.height {
                    return Rgba::default();
                }
                unpack(self.pixels[(sy * self.width + sx) as usize])
            }
            OutputMode::KeepSize => {
                let f = self.factor;
                let bx = (x - self.origin_x) * f;
                let by = (y - self.origin_y) * f;
                if bx < 0 || by < 0 || bx >= self.width || by >= self.height {
                    return Rgba::default();
                }
                let (mut a, mut r, mut g, mut b) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
                for yy in by..by + f {
                    let row = (yy * self.width) as usize;
                    for xx in bx..bx + f {
                        let c = unpack(self.pixels[row + xx as usize]);
                        a += c.a;
                        r += c.r * c.a;
                        g += c.g * c.a;
                        b += c.b * c.a;
                    }
                }
                if a <= 0.0 {
                    return Rgba::default();
                }
                Rgba { a: a / (f * f) as f32, r: r / a, g: g / a, b: b / a }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_mapping_keeps_center() {
        let m = Mapping::new(3, OutputMode::Expand, 5.0, 5.0);
        assert_eq!((m.offset_x, m.offset_y), (-10, -10));
        let out = m.output_rect(IRect { left: 0, top: 0, right: 10, bottom: 10 });
        assert_eq!(out, IRect { left: -10, top: -10, right: 20, bottom: 20 });
        assert_eq!((out.left + out.right) / 2, 5);
    }

    #[test]
    fn input_rect_covers_output() {
        let m = Mapping::new(4, OutputMode::Expand, 7.5, 3.0);
        for req in [
            IRect { left: -30, top: -9, right: 41, bottom: 17 },
            IRect { left: 0, top: 0, right: 1, bottom: 1 },
            IRect { left: -23, top: -10, right: -22, bottom: -9 },
        ] {
            let inp = m.input_rect_for(req);
            let core = inp.inflate(-SOURCE_MARGIN);
            let covered = m.output_rect(core);
            assert_eq!(covered.intersect(&req), req, "req {req:?} input {inp:?}");
        }
    }

    #[test]
    fn empty_request_gives_empty_input() {
        let m = Mapping::new(2, OutputMode::Expand, 1.0, 1.0);
        assert!(m.input_rect_for(IRect::EMPTY).is_empty());
        assert!(m.output_rect(IRect::EMPTY).is_empty());
    }

    #[test]
    fn xbrz_solid_color_interior_stays_solid() {
        let (w, h) = (40usize, 37usize);
        let c = pack_u8(255, 10, 200, 30);
        let src = vec![c; w * h];
        for f in FACTOR_MIN..=FACTOR_MAX {
            let dst = xbrz_scale(f, &ScalerCfg::default(), &src, w, h);
            assert_eq!(dst.len(), w * h * f * f);
            let (tw, th) = (w * f, h * f);
            for y in f..th - f {
                for x in f..tw - f {
                    assert_eq!(dst[y * tw + x], c, "factor {f} at ({x},{y})");
                }
            }
        }
    }

    #[test]
    fn threaded_matches_single_slice() {
        let (w, h) = (53usize, 97usize);
        let palette = [pack_u8(255, 0, 0, 0), pack_u8(255, 255, 255, 255), pack_u8(255, 200, 40, 40), pack_u8(0, 0, 0, 0)];
        let mut s = 12345u32;
        let src: Vec<u32> = (0..w * h)
            .map(|_| {
                s = s.wrapping_mul(1103515245).wrapping_add(12345);
                palette[((s >> 16) % 4) as usize]
            })
            .collect();
        let cfg = ScalerCfg::default();
        let threaded = xbrz_scale(3, &cfg, &src, w, h);
        let mut single = vec![0u32; w * h * 9];
        unsafe {
            xbrz_ae_scale_argb(3, src.as_ptr(), single.as_mut_ptr(), w as c_int, h as c_int, 30.0, 4.0, 3.6, 2.4, 0, h as c_int);
        }
        assert_eq!(threaded, single);
    }

    #[test]
    fn cropped_input_matches_full_input() {
        let (w, h) = (61i32, 45i32);
        let palette = [pack_u8(255, 30, 30, 30), pack_u8(255, 250, 220, 90), pack_u8(140, 60, 160, 240), 0];
        let mut s = 777u32;
        let layer: Vec<u32> = (0..w * h)
            .map(|_| {
                s = s.wrapping_mul(1103515245).wrapping_add(12345);
                palette[((s >> 16) % 4) as usize]
            })
            .collect();
        let layer_rect = IRect { left: 0, top: 0, right: w, bottom: h };
        let cfg = ScalerCfg::default();
        for (factor, mode) in [(2, OutputMode::Expand), (3, OutputMode::Expand), (5, OutputMode::Expand), (3, OutputMode::KeepSize)] {
            let map = Mapping::new(factor, mode, w as f64 / 2.0, h as f64 / 2.0);
            let full = Scaled::new(xbrz_scale(factor, &cfg, &layer, w as usize, h as usize), w as usize, h as usize, layer_rect, &map);
            for req in [
                IRect { left: 10, top: 7, right: 40, bottom: 30 },
                IRect { left: -50, top: -40, right: -20, bottom: -5 },
                IRect { left: 3, top: 3, right: 4, bottom: 4 },
            ] {
                let inp = map.input_rect_for(req).intersect(&layer_rect);
                if inp.is_empty() {
                    continue;
                }
                let (cw, ch) = ((inp.right - inp.left) as usize, (inp.bottom - inp.top) as usize);
                let crop: Vec<u32> = (inp.top..inp.bottom)
                    .flat_map(|y| (inp.left..inp.right).map(move |x| (y, x)))
                    .map(|(y, x)| layer[(y * w + x) as usize])
                    .collect();
                let part = Scaled::new(xbrz_scale(factor, &cfg, &crop, cw, ch), cw, ch, inp, &map);
                let out = map.output_rect(inp).intersect(&req);
                for y in out.top..out.bottom {
                    for x in out.left..out.right {
                        assert_eq!(part.sample(x, y), full.sample(x, y), "factor {factor} {mode:?} req {req:?} at ({x},{y})");
                    }
                }
            }
        }
    }

    #[test]
    fn keep_size_box_filter_of_solid_is_identity() {
        let c = pack_u8(255, 12, 34, 56);
        let src = vec![c; 8 * 8];
        let m = Mapping::new(4, OutputMode::KeepSize, 4.0, 4.0);
        let rect = IRect { left: -3, top: 2, right: 5, bottom: 10 };
        let scaled = Scaled::new(xbrz_scale(4, &ScalerCfg::default(), &src, 8, 8), 8, 8, rect, &m);
        let p = scaled.sample(0, 5);
        assert!((p.r - 12.0 / 255.0).abs() < 1e-6 && (p.a - 1.0).abs() < 1e-6);
        assert_eq!(scaled.sample(5, 5), Rgba::default());
        assert_eq!(scaled.sample(-4, 5), Rgba::default());
    }

    #[test]
    fn expand_sample_positions() {
        let m = Mapping::new(2, OutputMode::Expand, 3.0, 3.0);
        let rect = IRect { left: 1, top: 2, right: 7, bottom: 8 };
        let marker = pack_u8(255, 1, 2, 3);
        let mut pixels = vec![0u32; 12 * 12];
        pixels[5 * 12 + 7] = marker;
        let scaled = Scaled::new(pixels, 6, 6, rect, &m);
        assert_eq!(m.output_rect(rect), IRect { left: -1, top: 1, right: 11, bottom: 13 });
        assert_eq!(scaled.sample(-1 + 7, 1 + 5), unpack(marker));
        assert_eq!(scaled.sample(-1 + 6, 1 + 5), Rgba::default());
        assert_eq!(scaled.sample(-2, 1), Rgba::default());
        assert_eq!(scaled.sample(11, 1), Rgba::default());
    }
}
