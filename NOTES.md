# Development notes (chronological)

## 2026-10-02 initial implementation

### Decisions
- xBRZ implementation: upstream C++ 1.9 linked via `cc`. Considered `xbrz-rs` (bell345/xbrz-rs 0.1.0) but it is a port of 1.8, exposes only `scale_rgba` with fixed default config, no slice API (no multithreading), and uses a 5-bit compressed color-distance LUT by default (upstream uses a full 8-bit table).
- `after-effects` crate 0.4.0 (virtualritz/after-effects). AdrianEddy/after-effects clone is older (0.2.0).
- SmartFX only (no legacy Render). Expand mode maps `out = offset + N * in`, offset = round(center * (1 - N)), center = `in_data.width/height * downsample / 2` (SDK guide "The Size Of A Layer" recommends in_data width/height as center reference).
- Input request = inverse-mapped output request inflated by 3 px (xBRZ reads 2 px around each source pixel).

### Observations
- xBRZ 1.9 needs C++20 (`[[likely]]`, lambda syntax at xbrz.cpp:419 fails on /std:c++17).
- after-effects 0.4.0 fails to compile against SDK 25.2 headers (`name_do_not_use_directly` exists only in 25.6 `AE_Effect.h`). `after-effects-sys` build.rs only reruns on `wrapper.hpp` change, so changing `AESDK_ROOT` requires `cargo clean -p after-effects-sys`.
- `AdobePluginGlobal::can_load` no longer exists in 0.4.0.
- define_effect! catches panics only in debug unless cfg `catch_panics` is set; build.rs sets it.
- xBRZ (ARGB) rounds corners against the transparent outside, including corners of a solid image and isolated single pixels (alpha 0.788 at 2x). Tests were adjusted to this upstream behavior.
- Release DLL contains the `PIPL` resource and exports `EffectMain`.

### Hypotheses (not yet verified in After Effects)
- AE effect worlds are straight (non-premultiplied) alpha. Basis: SDK guide describes `PF_OutFlag2_REVEALS_ZERO_ALPHA` as revealing RGB of zero-alpha pixels, which implies RGB is stored independently of alpha; xBRZ's `alphaGrad` also assumes straight alpha.
- In SmartRender the output world's (0,0) is the `result_rect` top-left set in PreRender, and the input world's (0,0) is the checkout `result_rect` top-left. Basis: SDK Resizer sample's SmartFX path is consistent with this; not stated explicitly in the guide.
- `in_data.downsample_x/y` is valid during SMART_PRE_RENDER. Basis: SDK Resizer.cpp reads it there.

## 2026-10-02 verification with AEXCompat (onmokoworks/AEXCompat @ 61487e32, debug harness)

Route: `aexcompat-harness --headless --render-experimental-smart-request{,-16,-32-cpu}`, driven by `tools/verify_aexcompat.py`,
expected values from `examples/verify.rs` (whole-image xBRZ, then crop / box filter).

### Observations
- Param slots: 1 Scale, 2 Output, 3 Color Tolerance, 4 group start, 5-7 advanced sliders, 8 group end.
- Float slider defaults come back as f32 (3.6 -> 3.5999999046). Fixed: `slider_value` round-trips through the shortest f32 decimal.
- First run: Keep Size OK, Expand wrong at all depths, 32 bpc rejected by host output validation.
  - Expand cause: plugin placed the input world at the checkout `result_rect` ([13,9,51,39] for 2x on 64x48), but the host handed the whole 64x48 layer with `origin` (0,0). Fixed: locate input/output worlds by `PF_LayerDef.origin_x/y` (SDK header: "origin of buffer in layer coords; smart effect checkouts only").
  - 32 bpc cause: `after_effects::Layer::bit_depth()` returned 16 for an ARGB32F world (row_bytes 1024 for width 64), so float buffers were written as u16. Fixed: depth from `PF_SmartRenderInput.bitdepth` (`extra.bit_depth()`), plus a row_bytes sanity check.
- After the fixes: 23/23 cases pass (8/16/32 bpc x Expand 2/3/4/6x x Keep 2/4x, odd size 101x77, downsample 1/2 and 1/4 with non-matching full resolution). Expand is exact; Keep Size max diff is 0.5 (8-bit rounding).
- `max_result_rect` reported for 2x on 64x48: [-32,-24,96,72] (2x around center (32,24)).
- Retraction of the earlier hypothesis "the input world's (0,0) is the checkout `result_rect` top-left": false on AEXCompat (see the Expand cause above). The plugin now reads `origin_x/y` from each world instead of assuming either rect.

### Not covered by AEXCompat
- AEXCompat always requests `[0,0,w,h]`, so the expanded area outside the original layer bounds and a host that returns a cropped input world are not exercised by the host run. Covered only by unit tests (`input_rect_covers_output`, `cropped_input_matches_full_input`).
- Not yet run in real After Effects. The straight-alpha and `origin_x/y` hypotheses above remain unverified against AE itself.
- Whether real AE sets `PF_WorldFlag_DEEP` on float worlds (the reason `Layer::bit_depth` would misreport) is unknown; the plugin no longer depends on it.
- Filed onmokoworks/AEXCompat#1727 (2026-10-02): host cannot request output outside the layer bounds, so the expanded area is never rendered there; also notes the whole-layer input world vs checkout result_rect observation.
- Filed onmokoworks/AEXCompat#1728 for float worlds stamped with PF_WorldFlag_DEEP instead of RESERVED1 (2026-10-02).
