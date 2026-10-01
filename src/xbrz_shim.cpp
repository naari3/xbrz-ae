#include <cstddef>
#include <cstdint>
#include "xbrz.h"

extern "C" void xbrz_ae_scale_argb(size_t factor,
                                   const uint32_t* src, uint32_t* trg,
                                   int src_width, int src_height,
                                   double equal_color_tolerance,
                                   double center_direction_bias,
                                   double dominant_direction_threshold,
                                   double steep_direction_threshold,
                                   int y_first, int y_last)
{
    xbrz::ScalerCfg cfg;
    cfg.equalColorTolerance        = equal_color_tolerance;
    cfg.centerDirectionBias        = center_direction_bias;
    cfg.dominantDirectionThreshold = dominant_direction_threshold;
    cfg.steepDirectionThreshold    = steep_direction_threshold;
    xbrz::scale(factor, src, trg, src_width, src_height, xbrz::ColorFormat::argb, cfg, y_first, y_last);
}
