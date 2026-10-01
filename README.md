# xbrz-ae

After Effects effect plugin that upscales pixel art with [xBRZ](https://sourceforge.net/projects/xbrz/) (by Zenju).

- Plugin side: Rust, using the [`after-effects`](https://crates.io/crates/after-effects) crate (SmartFX, 8/16/32 bpc, multi-frame rendering).
- Algorithm: upstream xBRZ 1.9 C++ (`third_party/xbrz`, unmodified), compiled with `cc` and called through a small C shim (`src/xbrz_shim.cpp`).

## Parameters

| Name | Description |
|---|---|
| Scale | 2x – 6x |
| Output | **Expand**: output buffer becomes N× larger, scaled around the layer center. **Keep Size**: upscale with xBRZ, then box-filter back to the original size (smooths jaggies without changing size). |
| Color Tolerance | `equalColorTolerance` (default 30) |
| Advanced | `centerDirectionBias` (4), `dominantDirectionThreshold` (3.6), `steepDirectionThreshold` (2.4) |

Notes:
- xBRZ works on 8-bit ARGB. In 16/32 bpc projects the input is quantized to 8 bit (values outside 0–1 in 32 bpc are clamped). Keep Size mode writes the box-filtered result at full output precision.
- xBRZ treats outside of the image as transparent, so corners of an opaque full-frame layer get slightly rounded. This is upstream behavior.
- At reduced preview resolution the downsampled image is fed to xBRZ, so the preview differs from Full.

## Build

Requirements: Rust (MSVC), Visual Studio C++ toolchain, LLVM (`LIBCLANG_PATH`) for bindgen, After Effects SDK **25.6 or newer**.

Set `AESDK_ROOT` to the SDK root, either as an environment variable or in an untracked `.cargo/config.toml`:

```toml
[env]
AESDK_ROOT = { value = 'C:\path\to\AfterEffectsSDK', force = true }
```

`after-effects-sys` does not rerun bindgen when `AESDK_ROOT` changes, so run `cargo clean -p after-effects-sys` after changing it.

```
cargo build --release
cargo test
```

## Verification

- `cargo test`: mapping math, xBRZ threading, cropped-input equivalence.
- `python tools/verify_aexcompat.py <AEXCompat repo>`: renders through [AEXCompat](https://github.com/onmokoworks/AEXCompat)'s headless SmartFX route at 8/16/32 bpc and compares with `examples/verify.rs`. Needs `cargo build --release --example verify` and a built AEXCompat harness/worker.

See `NOTES.md` for what has and has not been verified.

## Install

Copy `target/release/xbrz_ae.dll` as `xBRZ.aex` into
`C:\Program Files\Adobe\Common\Plug-ins\7.0\MediaCore\` (requires admin), then restart After Effects.
The effect appears under **Effect > Stylize > xBRZ**.

## License

GPL-3.0-only, because xBRZ is GPL-3.0. See `LICENSE`.
