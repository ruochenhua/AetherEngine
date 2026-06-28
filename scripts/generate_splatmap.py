#!/usr/bin/env python3
"""Generate a smooth terrain splat map for the textured terrain demo scene.

The splat map stores blend weights for four material layers in its RGBA
channels. The pattern uses large-scale, low-frequency variation so that sand,
grass, rock, and snow form broad contiguous zones rather than scattered
speckles.
"""

import math
import sys
from pathlib import Path

from PIL import Image


def smooth(t: float) -> float:
    return t * t * (3.0 - 2.0 * t)


def noise2d(x: float, y: float, seed: int = 42) -> float:
    """Simple deterministic 2D noise in [0, 1]."""
    sx = math.sin(x * 12.9898 + y * 78.233 + seed) * 43758.5453
    return sx - math.floor(sx)


def clamp(value: float, low: float, high: float) -> float:
    return max(low, min(value, high))


def fractal_noise(x: float, y: float, octaves: int = 3, seed: int = 42) -> float:
    value = 0.0
    amplitude = 0.5
    frequency = 1.0
    for _ in range(octaves):
        value += amplitude * noise2d(x * frequency, y * frequency, seed)
        amplitude *= 0.5
        frequency *= 2.0
    return value


def box_blur(pixels, size: int, radius: int = 2):
    """In-place box blur to soften material boundaries."""
    for _ in range(2):
        # Horizontal pass
        for y in range(size):
            row = [pixels[x, y] for x in range(size)]
            for x in range(size):
                r = g = b = a = 0
                count = 0
                for dx in range(-radius, radius + 1):
                    sx = (x + dx) % size
                    pr, pg, pb, pa = row[sx]
                    r += pr
                    g += pg
                    b += pb
                    a += pa
                    count += 1
                pixels[x, y] = (r // count, g // count, b // count, a // count)
        # Vertical pass
        for x in range(size):
            col = [pixels[x, y] for y in range(size)]
            for y in range(size):
                r = g = b = a = 0
                count = 0
                for dy in range(-radius, radius + 1):
                    sy = (y + dy) % size
                    pr, pg, pb, pa = col[sy]
                    r += pr
                    g += pg
                    b += pb
                    a += pa
                    count += 1
                pixels[x, y] = (r // count, g // count, b // count, a // count)


def generate_splatmap(size: int = 512) -> Image.Image:
    img = Image.new("RGBA", (size, size))
    pixels = img.load()

    for y in range(size):
        for x in range(size):
            u = x / size
            v = y / size

            # Low-frequency height-like signal so zones are broad and smooth.
            h = fractal_noise(u * 1.2, v * 1.2, octaves=2, seed=7)
            # A gentle diagonal slope so one corner is higher.
            h = h * 0.45 + (u + v) * 0.55

            # Wider smooth transitions reduce speckled patches.
            sand = 1.0 - smooth(clamp((h - 0.15) / 0.30, 0.0, 1.0))
            grass = smooth(clamp((h - 0.12) / 0.30, 0.0, 1.0)) * (
                1.0 - smooth(clamp((h - 0.42) / 0.35, 0.0, 1.0))
            )
            rock = smooth(clamp((h - 0.38) / 0.35, 0.0, 1.0)) * (
                1.0 - smooth(clamp((h - 0.68) / 0.28, 0.0, 1.0))
            )
            snow = smooth(clamp((h - 0.65) / 0.28, 0.0, 1.0))

            total = sand + grass + rock + snow
            if total > 0.0:
                sand /= total
                grass /= total
                rock /= total
                snow /= total

            pixels[x, y] = (
                int(sand * 255),
                int(grass * 255),
                int(rock * 255),
                int(snow * 255),
            )

    box_blur(pixels, size, radius=2)
    return img


def main() -> None:
    if len(sys.argv) > 1:
        output = Path(sys.argv[1])
    else:
        output = Path("assets/textures/terrain/splatmap.png")

    output.parent.mkdir(parents=True, exist_ok=True)
    img = generate_splatmap()
    img.save(output)
    print(f"Generated {output}")


if __name__ == "__main__":
    main()
