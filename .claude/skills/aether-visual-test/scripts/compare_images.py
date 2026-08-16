#!/usr/bin/env python3
"""Compare two images and output similarity metrics for visual regression testing.

Supports optional diff and side-by-side visual artifacts to make regressions easier
to inspect in reports.
"""

import sys
import argparse
import json
from pathlib import Path

from PIL import Image, ImageDraw
import numpy as np

DIFF_THRESHOLD = 10.0


def load_images(ref_path: Path, out_path: Path):
    ref = Image.open(ref_path).convert("RGBA")
    out = Image.open(out_path).convert("RGBA")

    if ref.size != out.size:
        out = out.resize(ref.size, Image.Resampling.LANCZOS)

    return ref, out


def compare_images(ref: Image.Image, out: Image.Image):
    ref_arr = np.array(ref).astype(np.float32)
    out_arr = np.array(out).astype(np.float32)

    mae = float(np.mean(np.abs(ref_arr - out_arr)))
    diff_mask = np.abs(ref_arr - out_arr) > DIFF_THRESHOLD
    diff_pct = float(np.mean(diff_mask) * 100)

    # Try SSIM if scikit-image is available
    ssim_val = None
    try:
        from skimage.metrics import structural_similarity as ssim
        ssim_val = float(
            ssim(
                ref_arr[:, :, :3],
                out_arr[:, :, :3],
                channel_axis=2,
                data_range=255,
            )
        )
    except ImportError:
        pass

    return {
        "ssim": ssim_val,
        "mae": mae,
        "diff_pct": diff_pct,
        "width": ref.width,
        "height": ref.height,
    }


def save_diff_image(ref: Image.Image, out: Image.Image, path: Path):
    """Save an image highlighting changed pixels in red over the output image."""
    ref_arr = np.array(ref).astype(np.float32)
    out_arr = np.array(out).astype(np.float32)
    diff_mask = np.any(np.abs(ref_arr - out_arr) > DIFF_THRESHOLD, axis=2)

    diff = out.copy()
    diff_arr = np.array(diff)
    # Mark changed pixels as red with a bit of original color preserved.
    diff_arr[diff_mask] = [255, 0, 0, 255]
    Image.fromarray(diff_arr, "RGBA").save(path)
    return path


def save_side_by_side(ref: Image.Image, out: Image.Image, path: Path):
    """Save reference and output images side-by-side with a divider."""
    gap = 4
    combined = Image.new(
        "RGBA",
        (ref.width + out.width + gap, max(ref.height, out.height)),
        (0, 0, 0, 255),
    )
    combined.paste(ref, (0, 0))
    combined.paste(out, (ref.width + gap, 0))
    draw = ImageDraw.Draw(combined)
    draw.rectangle(
        [ref.width, 0, ref.width + gap - 1, combined.height - 1],
        fill=(255, 0, 0, 255),
    )
    combined.save(path)
    return path


def main():
    parser = argparse.ArgumentParser(
        description="Compare reference and output images for visual regression"
    )
    parser.add_argument("reference", type=Path, help="Path to reference image")
    parser.add_argument("output", type=Path, help="Path to output image")
    parser.add_argument(
        "--threshold", type=float, default=0.95, help="SSIM threshold for PASS"
    )
    parser.add_argument("--json", action="store_true", help="Output as JSON")
    parser.add_argument(
        "--diff",
        type=Path,
        default=None,
        help="Save a diff visualization to this path (PNG)",
    )
    parser.add_argument(
        "--side-by-side",
        type=Path,
        default=None,
        help="Save a side-by-side reference/output image to this path (PNG)",
    )
    args = parser.parse_args()

    if not args.reference.exists():
        print(f"Reference image not found: {args.reference}", file=sys.stderr)
        sys.exit(2)
    if not args.output.exists():
        print(f"Output image not found: {args.output}", file=sys.stderr)
        sys.exit(2)

    ref, out = load_images(args.reference, args.output)
    result = compare_images(ref, out)

    if args.diff:
        save_diff_image(ref, out, args.diff)
        result["diff_image"] = str(args.diff)

    if args.side_by_side:
        save_side_by_side(ref, out, args.side_by_side)
        result["side_by_side"] = str(args.side_by_side)

    if args.json:
        print(json.dumps(result, indent=2))
    else:
        print(f"Resolution: {result['width']}x{result['height']}")
        if result["ssim"] is not None:
            status = "PASS" if result["ssim"] >= args.threshold else "FAIL"
            print(
                f"SSIM: {result['ssim']:.4f} (threshold: {args.threshold}) [{status}]"
            )
        else:
            print("SSIM: N/A (install scikit-image for SSIM)")
        print(f"MAE:  {result['mae']:.2f}")
        print(f"Diff: {result['diff_pct']:.2f}% pixels differ >{DIFF_THRESHOLD:g}")

        if args.diff:
            print(f"Diff image: {args.diff}")
        if args.side_by_side:
            print(f"Side-by-side: {args.side_by_side}")

        if result["ssim"] is not None and result["ssim"] < args.threshold:
            sys.exit(1)


if __name__ == "__main__":
    main()
