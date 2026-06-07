#!/usr/bin/env python3
"""Compare two images and output similarity metrics for visual regression testing."""

import sys
import argparse
import json
from pathlib import Path


def compare_images(ref_path: Path, out_path: Path):
    from PIL import Image
    import numpy as np

    ref = Image.open(ref_path).convert("RGBA")
    out = Image.open(out_path).convert("RGBA")

    if ref.size != out.size:
        out = out.resize(ref.size, Image.Resampling.LANCZOS)

    ref_arr = np.array(ref).astype(np.float32)
    out_arr = np.array(out).astype(np.float32)

    mae = float(np.mean(np.abs(ref_arr - out_arr)))
    diff_mask = np.abs(ref_arr - out_arr) > 10
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
    args = parser.parse_args()

    if not args.reference.exists():
        print(f"Reference image not found: {args.reference}", file=sys.stderr)
        sys.exit(2)
    if not args.output.exists():
        print(f"Output image not found: {args.output}", file=sys.stderr)
        sys.exit(2)

    result = compare_images(args.reference, args.output)

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
        print(f"Diff: {result['diff_pct']:.2f}% pixels differ >10")

        if result["ssim"] is not None and result["ssim"] < args.threshold:
            sys.exit(1)


if __name__ == "__main__":
    main()
