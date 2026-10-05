#!/usr/bin/env python3
"""Write a deterministic 1,000-object T9.1 render fixture."""

from __future__ import annotations

import argparse
import math
import random
from pathlib import Path


ENTITY_COUNT = 1000
SEED = 9001


def scene_text(
    seed: int,
    entity_count: int = ENTITY_COUNT,
    extent: float | None = None,
    lod_zones: bool = False,
    scene_name: str = "T9.1 Baseline Report",
) -> str:
    side = math.ceil(math.sqrt(entity_count))
    spacing = 1.1 if extent is None else (2.0 * extent) / max(side - 1, 1)
    center = (side - 1) * spacing * 0.5
    spots = [
        (column * spacing - center, row * spacing - center)
        for row in range(side)
        for column in range(side)
    ]
    random.Random(seed).shuffle(spots)
    objects = []
    for index, (x, z) in enumerate(spots[:entity_count]):
        if lod_zones:
            high_lod_probe = 2.5 < x < 5.5 and -5.0 < z < -2.5
            scale = 3.0 if high_lod_probe else (1.5 if z > -18.0 else 0.45)
        else:
            scale = 0.45
        objects.append(
            "        (\n"
            f'            name: "BenchObject{index:04}",\n'
            '            mesh: Builtin("cube"),\n'
            "            transform: (\n"
            f"                translation: ({x:.4f}, 0.0, {z:.4f}),\n"
            f"                scale: ({scale:.4f}, {scale:.4f}, {scale:.4f}),\n"
            "            ),\n"
            "            material: (\n"
            "                albedo: (0.42, 0.56, 0.72, 1.0),\n"
            "                roughness: 0.62,\n"
            "                metallic: 0.05,\n"
            "            ),\n"
            "        )"
        )
    if lod_zones:
        camera = (
            "        position: (0.0, 6.0, 2.0),\n"
            "        yaw: -1.570796,\n"
            "        pitch: -0.6,\n"
        )
    else:
        camera = (
            "        position: (0.0, 28.0, 42.0),\n"
            "        yaw: -1.570796,\n"
            "        pitch: -0.56,\n"
        )
    return (
        "SceneDescription(\n"
        f'    name: "{scene_name}",\n'
        "    camera: (\n"
        + camera
        + "        speed: 4.0,\n"
        "        fov: 45.0,\n"
        "    ),\n"
        "    lights: [\n"
        "        (\n"
        "            light_type: Directional,\n"
        "            direction: (-0.4, -1.0, -0.3),\n"
        "            color: (1.0, 0.96, 0.9),\n"
        "            intensity: 1.0,\n"
        "        ),\n"
        "    ],\n"
        "    ambient: 0.08,\n"
        "    objects: [\n"
        + ",\n".join(objects)
        + "\n    ],\n)\n"
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--seed", type=int, default=SEED)
    parser.add_argument("--entity-count", type=int, default=ENTITY_COUNT)
    parser.add_argument("--extent", type=float)
    parser.add_argument("--lod-zones", action="store_true")
    parser.add_argument("--scene-name", default="T9.1 Baseline Report")
    args = parser.parse_args()
    if args.entity_count <= 0:
        parser.error("--entity-count must be positive")
    if args.extent is not None and args.extent <= 0.0:
        parser.error("--extent must be positive")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(
        scene_text(args.seed, args.entity_count, args.extent, args.lod_zones, args.scene_name),
        encoding="utf-8",
        newline="\n",
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
