pub mod core;

use after_effects as ae;
use core::{IRect, Mapping, OutputMode, Rgba, ScalerCfg, Scaled};

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Factor,
    Output,
    EqualColorTolerance,
    AdvancedStart,
    CenterDirectionBias,
    DominantDirectionThreshold,
    SteepDirectionThreshold,
    AdvancedEnd,
}

const FACTOR_OPTIONS: &[&str] = &["2x", "3x", "4x", "5x", "6x"];
const OUTPUT_OPTIONS: &[&str] = &["Expand", "Keep Size"];

#[derive(Default)]
struct Plugin {}

ae::define_effect!(Plugin, (), Params);

#[derive(Clone, Debug)]
struct RenderPlan {
    factor: usize,
    cfg: ScalerCfg,
    map: Mapping,
    output_rect: IRect,
}

fn float_slider(
    params: &mut ae::Parameters<Params>,
    id: Params,
    name: &str,
    min: f32,
    max: f32,
    default: f64,
    precision: i16,
) -> Result<(), Error> {
    params.add(id, name, ae::FloatSliderDef::setup(|f| {
        f.set_valid_min(min);
        f.set_valid_max(max);
        f.set_slider_min(min);
        f.set_slider_max(max);
        f.set_default(default);
        f.set_value(default);
        f.set_precision(precision);
    }))
}

// AE stores slider defaults as f32 (3.6 -> 3.5999999); undo that before passing to xBRZ.
fn slider_value(params: &ae::Parameters<Params>, id: Params) -> Result<f64, Error> {
    let v = params.get(id)?.as_float_slider()?.value();
    Ok((v as f32).to_string().parse().unwrap_or(v))
}

fn read_settings(params: &ae::Parameters<Params>) -> Result<(usize, OutputMode, ScalerCfg), Error> {
    let factor = core::FACTOR_MIN + (params.get(Params::Factor)?.as_popup()?.value() as usize).saturating_sub(1);
    let factor = factor.clamp(core::FACTOR_MIN, core::FACTOR_MAX);
    let mode = match params.get(Params::Output)?.as_popup()?.value() {
        2 => OutputMode::KeepSize,
        _ => OutputMode::Expand,
    };
    let cfg = ScalerCfg {
        equal_color_tolerance: slider_value(params, Params::EqualColorTolerance)?,
        center_direction_bias: slider_value(params, Params::CenterDirectionBias)?,
        dominant_direction_threshold: slider_value(params, Params::DominantDirectionThreshold)?,
        steep_direction_threshold: slider_value(params, Params::SteepDirectionThreshold)?,
    };
    Ok((factor, mode, cfg))
}

fn to_irect(r: ae::sys::PF_LRect) -> IRect {
    IRect { left: r.left, top: r.top, right: r.right, bottom: r.bottom }
}

fn to_lrect(r: IRect) -> ae::sys::PF_LRect {
    ae::sys::PF_LRect { left: r.left, top: r.top, right: r.right, bottom: r.bottom }
}

fn to_ae_rect(r: IRect) -> ae::Rect {
    ae::Rect { left: r.left, top: r.top, right: r.right, bottom: r.bottom }
}

impl AdobePluginGlobal for Plugin {
    fn params_setup(&self, params: &mut ae::Parameters<Params>, _: InData, _: OutData) -> Result<(), Error> {
        let d = ScalerCfg::default();

        params.add(Params::Factor, "Scale", ae::PopupDef::setup(|f| {
            f.set_options(FACTOR_OPTIONS);
            f.set_default(1);
            f.set_value(f.default());
        }))?;
        params.add(Params::Output, "Output", ae::PopupDef::setup(|f| {
            f.set_options(OUTPUT_OPTIONS);
            f.set_default(1);
            f.set_value(f.default());
        }))?;
        float_slider(params, Params::EqualColorTolerance, "Color Tolerance", 0.0, 255.0, d.equal_color_tolerance, 1)?;

        params.add_group(Params::AdvancedStart, Params::AdvancedEnd, "Advanced", true, |params| {
            float_slider(params, Params::CenterDirectionBias, "Center Direction Bias", 0.0, 20.0, d.center_direction_bias, 2)?;
            float_slider(params, Params::DominantDirectionThreshold, "Dominant Direction Threshold", 0.0, 20.0, d.dominant_direction_threshold, 2)?;
            float_slider(params, Params::SteepDirectionThreshold, "Steep Direction Threshold", 0.0, 20.0, d.steep_direction_threshold, 2)?;
            Ok(())
        })?;

        Ok(())
    }

    fn handle_command(&mut self, cmd: ae::Command, in_data: InData, mut out_data: OutData, params: &mut ae::Parameters<Params>) -> Result<(), ae::Error> {
        match cmd {
            ae::Command::About => {
                out_data.set_return_msg(&format!(
                    "xBRZ v{}\rPixel art upscaler using xBRZ 1.9 by Zenju.\rLicensed under GPL-3.0.",
                    env!("CARGO_PKG_VERSION")
                ));
            }
            ae::Command::SmartPreRender { mut extra } => {
                let (factor, mode, cfg) = read_settings(params)?;

                let ds_x: f64 = in_data.downsample_x().into();
                let ds_y: f64 = in_data.downsample_y().into();
                let center_x = in_data.width() as f64 * ds_x / 2.0;
                let center_y = in_data.height() as f64 * ds_y / 2.0;
                let map = Mapping::new(factor, mode, center_x, center_y);

                let req = extra.output_request();
                let req_rect = to_irect(req.rect);
                let mut in_req = req;
                in_req.rect = to_lrect(map.input_rect_for(req_rect));

                let in_result = extra.callbacks().checkout_layer(
                    0, 0, &in_req,
                    in_data.current_time(), in_data.time_step(), in_data.time_scale(),
                )?;
                let input_rect = to_irect(in_result.result_rect);
                let input_max = to_irect(in_result.max_result_rect);

                let output_rect = map.output_rect(input_rect).intersect(&req_rect);
                extra.set_result_rect(to_ae_rect(output_rect));
                extra.set_max_result_rect(to_ae_rect(map.output_rect(input_max)));
                extra.set_pre_render_data(RenderPlan { factor, cfg, map, output_rect });
            }
            ae::Command::SmartRender { extra } => {
                let Some(plan) = extra.pre_render_data::<RenderPlan>().cloned() else {
                    return Err(Error::BadCallbackParameter);
                };
                // Layer::bit_depth() trusts world_flags, which can misreport float worlds.
                let depth = Depth::from_bits(extra.bit_depth())?;
                let cb = extra.callbacks();
                let input = cb.checkout_layer_pixels(0)?;
                if let Some(mut output) = cb.checkout_output()? {
                    match input {
                        Some(input) => render(&plan, depth, &input, &mut output)?,
                        None => clear(depth, &mut output)?,
                    }
                }
                cb.checkin_layer_pixels(0)?;
            }
            _ => {}
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Depth {
    U8,
    U16,
    F32,
}

impl Depth {
    fn from_bits(bits: i16) -> Result<Self, Error> {
        match bits {
            8 => Ok(Depth::U8),
            16 => Ok(Depth::U16),
            32 => Ok(Depth::F32),
            _ => Err(Error::BadCallbackParameter),
        }
    }
    fn bytes_per_pixel(self) -> usize {
        match self {
            Depth::U8 => 4,
            Depth::U16 => 8,
            Depth::F32 => 16,
        }
    }
}

fn check_world(layer: &ae::Layer, depth: Depth) -> Result<(), Error> {
    if layer.width() > 0 && layer.height() > 0 && layer.buffer_stride() < layer.width() * depth.bytes_per_pixel() {
        return Err(Error::BadCallbackParameter);
    }
    Ok(())
}

fn read_input(layer: &ae::Layer, depth: Depth) -> Result<Vec<u32>, Error> {
    check_world(layer, depth)?;
    let (w, h) = (layer.width(), layer.height());
    let mut v = Vec::with_capacity(w * h);
    match depth {
        Depth::U8 => {
            for y in 0..h {
                for x in 0..w {
                    let p = layer.as_pixel8(x, y);
                    v.push(core::pack_u8(p.alpha, p.red, p.green, p.blue));
                }
            }
        }
        Depth::U16 => {
            let c = |v: u16| ((v.min(ae::MAX_CHANNEL16 as u16) as u32 * 255 + ae::MAX_CHANNEL16 / 2) / ae::MAX_CHANNEL16) as u8;
            for y in 0..h {
                for x in 0..w {
                    let p = layer.as_pixel16(x, y);
                    v.push(core::pack_u8(c(p.alpha), c(p.red), c(p.green), c(p.blue)));
                }
            }
        }
        Depth::F32 => {
            let c = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
            for y in 0..h {
                for x in 0..w {
                    let p = layer.as_pixel32(x, y);
                    v.push(core::pack_u8(c(p.alpha), c(p.red), c(p.green), c(p.blue)));
                }
            }
        }
    }
    Ok(v)
}

fn write_output(out: &mut ae::Layer, depth: Depth, mut color_at: impl FnMut(usize, usize) -> Rgba) -> Result<(), Error> {
    check_world(out, depth)?;
    let (w, h) = (out.width(), out.height());
    match depth {
        Depth::U8 => {
            let c = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
            for y in 0..h {
                for x in 0..w {
                    let s = color_at(x, y);
                    *out.as_pixel8_mut(x, y) = ae::Pixel8 { alpha: c(s.a), red: c(s.r), green: c(s.g), blue: c(s.b) };
                }
            }
        }
        Depth::U16 => {
            let c = |v: f32| (v.clamp(0.0, 1.0) * ae::MAX_CHANNEL16 as f32).round() as u16;
            for y in 0..h {
                for x in 0..w {
                    let s = color_at(x, y);
                    *out.as_pixel16_mut(x, y) = ae::Pixel16 { alpha: c(s.a), red: c(s.r), green: c(s.g), blue: c(s.b) };
                }
            }
        }
        Depth::F32 => {
            for y in 0..h {
                for x in 0..w {
                    let s = color_at(x, y);
                    *out.as_pixel32_mut(x, y) = ae::PixelF32 { alpha: s.a, red: s.r, green: s.g, blue: s.b };
                }
            }
        }
    }
    Ok(())
}

fn clear(depth: Depth, out: &mut ae::Layer) -> Result<(), Error> {
    write_output(out, depth, |_, _| Rgba::default())
}

fn render(plan: &RenderPlan, depth: Depth, input: &ae::Layer, output: &mut ae::Layer) -> Result<(), Error> {
    let (w, h) = (input.width(), input.height());
    if w == 0 || h == 0 || plan.output_rect.is_empty() {
        return clear(depth, output);
    }
    let src = read_input(input, depth)?;
    let pixels = core::xbrz_scale(plan.factor, &plan.cfg, &src, w, h);

    // Locate worlds by origin_x/y: the input world may be larger than the checkout result_rect.
    let in_origin = input.origin();
    let in_rect = IRect { left: in_origin.h, top: in_origin.v, right: in_origin.h + w as i32, bottom: in_origin.v + h as i32 };
    let scaled = Scaled::new(pixels, w, h, in_rect, &plan.map);

    let out_origin = output.origin();
    let (ox, oy) = (out_origin.h, out_origin.v);
    write_output(output, depth, |x, y| scaled.sample(ox + x as i32, oy + y as i32))
}
