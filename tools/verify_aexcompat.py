"""Render the plugin through AEXCompat's headless SmartFX route and compare with examples/verify.rs.

Usage (from repo root, after `cargo build --release` and `cargo build --release --example verify`):
    python tools/verify_aexcompat.py <path-to-AEXCompat-repo>
"""
import json
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
WORK = ROOT / "target" / "aexcompat"
AEX = ROOT / "target" / "release" / "xbrz_ae.dll"
VERIFY = ROOT / "target" / "release" / "examples" / "verify.exe"

ROUTES = {8: "--render-experimental-smart-request", 16: "--render-experimental-smart-request-16", 32: "--render-experimental-smart-request-32-cpu"}


def find_key(o, key):
    if isinstance(o, dict):
        if key in o:
            return o[key]
        for v in o.values():
            r = find_key(v, key)
            if r is not None:
                return r
    elif isinstance(o, list):
        for v in o:
            r = find_key(v, key)
            if r is not None:
                return r
    return None


def run_case(harness, name, image, depth, factor, mode, spatial=None, extra_assign=()):
    w, h = map(int, image.stem.split("_")[0].removeprefix("in").split("x"))
    req = {
        "schema_version": 1,
        "assignments": [
            {"slot": 1, "value": factor - 1},
            {"slot": 2, "value": {"expand": 1, "keep": 2}.get(mode, 3)},
            {"slot": 3, "value": 2 if mode == "shrink-average" else 1},
            *extra_assign,
        ],
        "timing": {"frame": 0, "fps": 30},
    }
    full_w, full_h, ds = w, h, 1.0
    if spatial:
        num, den, full_w, full_h = spatial
        ds = num / den
        req["host_context"] = {
            "mask_scene": {"masks": []},
            "spatial": {
                "downsample_x": {"numerator": num, "denominator": den},
                "downsample_y": {"numerator": num, "denominator": den},
                "pixel_aspect_ratio": {"numerator": 1, "denominator": 1},
                "full_resolution_width": full_w,
                "full_resolution_height": full_h,
            },
        }
    case = WORK / name
    case.mkdir(parents=True, exist_ok=True)
    for p in case.iterdir():
        p.unlink()
    (case / "req.json").write_text(json.dumps(req))
    out_png = case / "out.png"
    r = subprocess.run(
        [harness, "--headless", ROUTES[depth], str(AEX), str(image), str(out_png), str(case / "req.json")],
        capture_output=True, text=True,
    )
    if r.returncode != 0:
        return f"FAIL harness exit {r.returncode}: {r.stderr.strip()[:300]}"
    rep = json.loads(r.stdout)
    (case / "report.json").write_text(r.stdout)
    origin = find_key(rep, "output_origin")
    result_rect = find_key(rep, "result_rect")
    ow, oh = find_key(rep, "output_world")["width"], find_key(rep, "output_world")["height"]
    violation = find_key(rep, "extra_pixels_contract_violation")

    cx, cy = full_w * ds / 2, full_h * ds / 2
    e = subprocess.run(
        [str(VERIFY), "expect", str(image), str(factor), mode, repr(cx), repr(cy), str(origin[0]), str(origin[1]), str(ow), str(oh), str(case / "expected.f32")],
        capture_output=True, text=True, check=True,
    )
    exp_rect = [int(v) for v in e.stdout.split()[1:5]]

    if depth == 8:
        actual, kind = out_png, "png"
    else:
        raw = sorted(p for p in case.iterdir() if p.suffix in (".rgba16le", ".rgba32f-le"))
        if not raw:
            return f"FAIL no raw sidecar in {case}"
        actual, kind = raw[0], raw[0].suffix[1:]
    c = subprocess.run([str(VERIFY), "cmp", str(case / "expected.f32"), str(actual), kind, str(ow), str(oh)], capture_output=True, text=True)
    if c.returncode != 0:
        return f"FAIL cmp: {c.stderr.strip()[:300]}"
    stats = c.stdout.split()
    max_diff, over_half = float(stats[1]), int(stats[3])
    # Expand at 8/16/32 bpc writes exact 8-bit values; Keep Size quantizes to the output depth.
    ok = result_rect == exp_rect and not violation and over_half == 0
    return f"{'OK  ' if ok else 'FAIL'} result_rect={result_rect} expected={exp_rect} world={ow}x{oh} max_diff_8bit={max_diff:.4f} over0.5={over_half}"


def main():
    aexcompat = Path(sys.argv[1])
    harness = str(aexcompat / "broker" / "target" / "debug" / "aexcompat-harness.exe")
    WORK.mkdir(parents=True, exist_ok=True)
    for size in ("64x48", "101x77"):
        subprocess.run([str(VERIFY), "gen", str(WORK / f"in{size}.png"), *size.split("x")], check=True)
    img = WORK / "in64x48.png"
    odd = WORK / "in101x77.png"
    subprocess.run([str(VERIFY), "gen", str(WORK / "in20x15.png"), "20", "15"], check=True)
    enlarged = {}
    for f in (2, 4):
        enlarged[f] = WORK / f"in{20 * f}x{15 * f}_nn{f}.png"
        subprocess.run([str(VERIFY), "enlarge", str(WORK / "in20x15.png"), str(f), str(enlarged[f])], check=True)
    cases = []
    for depth in (8, 16, 32):
        for f in (2, 3, 4, 6):
            cases.append((f"d{depth}_expand{f}", img, depth, f, "expand", None))
        for f in (2, 4):
            cases.append((f"d{depth}_keep{f}", img, depth, f, "keep", None))
        for f in (2, 3, 5):
            cases.append((f"d{depth}_shrink_nearest{f}", img, depth, f, "shrink-nearest", None))
        cases.append((f"d{depth}_shrink_average3", img, depth, 3, "shrink-average", None))
    cases += [
        ("d8_odd_expand3", odd, 8, 3, "expand", None),
        ("d8_odd_expand2", odd, 8, 2, "expand", None),
        ("d8_odd_keep3", odd, 8, 3, "keep", None),
        ("d8_odd_shrink_nearest4", odd, 8, 4, "shrink-nearest", None),
        ("d8_odd_shrink_average3", odd, 8, 3, "shrink-average", None),
        ("d8_nn2_shrink_nearest2", enlarged[2], 8, 2, "shrink-nearest", None),
        ("d8_nn4_shrink_average4", enlarged[4], 8, 4, "shrink-average", None),
        # Half-resolution preview of a 130x100 layer: center must come from full size * downsample.
        ("d8_ds_half_expand3", img, 8, 3, "expand", (1, 2, 130, 100)),
        ("d32_ds_quarter_expand2", img, 32, 2, "expand", (1, 4, 260, 190)),
    ]
    fails = 0
    for name, image, depth, f, mode, spatial in cases:
        res = run_case(harness, name, image, depth, f, mode, spatial)
        fails += res.startswith("FAIL")
        print(f"{name:26s} {res}", flush=True)
    print(f"{len(cases) - fails}/{len(cases)} passed")
    sys.exit(1 if fails else 0)


if __name__ == "__main__":
    main()
