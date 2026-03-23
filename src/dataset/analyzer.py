"""
Dataset analyzer: computes image counts, class distribution, boxes-per-image stats.
"""
from __future__ import annotations

import json
import statistics
from dataclasses import dataclass, field
from pathlib import Path
from typing import Optional

from src.core.config_loader import PROJECT_ROOT
from src.core.logger import get_logger
from src.dataset.metadata import DatasetMetadata
from src.dataset.validator import IMAGE_EXTENSIONS

logger = get_logger()

_DEFAULT_CACHE = PROJECT_ROOT / "reports" / "evaluation" / "dataset_stats.json"


@dataclass
class SplitStats:
    split: str
    images: int = 0
    annotations: int = 0
    boxes_per_image: dict[str, float] = field(default_factory=dict)
    class_distribution: dict[str, int] = field(default_factory=dict)
    error: str | None = None

    def to_dict(self) -> dict:
        return {
            "split": self.split,
            "images": self.images,
            "annotations": self.annotations,
            "boxes_per_image": self.boxes_per_image,
            "class_distribution": self.class_distribution,
            "error": self.error,
        }


@dataclass
class DatasetStats:
    dataset_root: str
    class_names: list[str]
    num_classes: int
    splits: dict[str, SplitStats] = field(default_factory=dict)
    total_images: int = 0
    total_boxes: int = 0
    global_class_distribution: dict[str, int] = field(default_factory=dict)

    def to_dict(self) -> dict:
        return {
            "dataset_root": self.dataset_root,
            "class_names": self.class_names,
            "num_classes": self.num_classes,
            "total_images": self.total_images,
            "total_boxes": self.total_boxes,
            "global_class_distribution": self.global_class_distribution,
            "splits": {k: v.to_dict() for k, v in self.splits.items()},
        }

    @classmethod
    def from_dict(cls, d: dict) -> DatasetStats:
        splits = {
            k: SplitStats(
                split=v["split"],
                images=v.get("images", 0),
                annotations=v.get("annotations", 0),
                boxes_per_image=v.get("boxes_per_image", {}),
                class_distribution=v.get("class_distribution", {}),
                error=v.get("error"),
            )
            for k, v in d.get("splits", {}).items()
        }
        return cls(
            dataset_root=d.get("dataset_root", ""),
            class_names=d.get("class_names", []),
            num_classes=d.get("num_classes", 0),
            splits=splits,
            total_images=d.get("total_images", 0),
            total_boxes=d.get("total_boxes", 0),
            global_class_distribution=d.get("global_class_distribution", {}),
        )


def _analyze_split(
    split_name: str,
    split_root: Path | None,
    class_names: list[str],
) -> SplitStats:
    stats = SplitStats(split=split_name)

    if split_root is None or not split_root.exists():
        stats.error = f"Split root not found: {split_root}"
        return stats

    # data.yaml may point directly to the images/ folder (YOLOv8 standard)
    # or to the split root that contains images/ and labels/ subdirs.
    if split_root.name == "images" and (split_root.parent / "labels").exists():
        images_path = split_root
        labels_path = split_root.parent / "labels"
    else:
        images_path = split_root / "images"
        labels_path = split_root / "labels"

    if not images_path.exists() or not labels_path.exists():
        stats.error = f"images/ or labels/ missing under {split_root}"
        return stats

    image_files = [
        f for f in images_path.iterdir()
        if f.is_file() and f.suffix.lower() in IMAGE_EXTENSIONS
    ]
    stats.images = len(image_files)

    boxes_counts: list[int] = []
    class_counts: dict[int, int] = {}

    for img in image_files:
        lbl_path = labels_path / (img.stem + ".txt")
        if not lbl_path.exists():
            boxes_counts.append(0)
            continue

        try:
            lines = [
                line.strip()
                for line in lbl_path.read_text(encoding="utf-8").splitlines()
                if line.strip()
            ]
            boxes_counts.append(len(lines))
            stats.annotations += len(lines)
            for line in lines:
                parts = line.split()
                if parts:
                    cls_id = int(parts[0])
                    class_counts[cls_id] = class_counts.get(cls_id, 0) + 1
        except Exception as exc:
            logger.debug(f"Skipping label {lbl_path.name}: {exc}")
            boxes_counts.append(0)

    # Boxes-per-image stats
    if boxes_counts:
        stats.boxes_per_image = {
            "mean": round(statistics.mean(boxes_counts), 3),
            "median": round(statistics.median(boxes_counts), 3),
            "min": min(boxes_counts),
            "max": max(boxes_counts),
            "std": round(statistics.stdev(boxes_counts), 3) if len(boxes_counts) > 1 else 0.0,
        }

    # Map class id -> name
    for cls_id, count in sorted(class_counts.items()):
        name = class_names[cls_id] if cls_id < len(class_names) else str(cls_id)
        stats.class_distribution[name] = count

    logger.debug(
        f"[{split_name}] images={stats.images} boxes={stats.annotations} "
        f"classes={list(stats.class_distribution.keys())}"
    )
    return stats


def analyze(meta: DatasetMetadata, cache_path: Path | None = None) -> DatasetStats:
    """Compute full statistics for the dataset.

    Results are cached to *cache_path* (default: reports/evaluation/dataset_stats.json).
    On subsequent calls the cache is returned immediately if it is newer than data.yaml,
    skipping the expensive per-file label scan (~21 K files for D-Fire).
    """
    cache = cache_path or _DEFAULT_CACHE

    # ── Cache hit check ──────────────────────────────────────────────────────
    if cache.exists() and meta.data_yaml_path and meta.data_yaml_path.exists():
        if cache.stat().st_mtime > meta.data_yaml_path.stat().st_mtime:
            try:
                cached_dict = json.loads(cache.read_text(encoding="utf-8"))
                stats = DatasetStats.from_dict(cached_dict)
                logger.info(
                    "Dataset stats loaded from cache (%s) — skipping label scan.",
                    cache.name,
                )
                return stats
            except Exception as exc:
                logger.debug("Cache load failed (%s), re-analyzing.", exc)
    # ─────────────────────────────────────────────────────────────────────────

    ds_stats = DatasetStats(
        dataset_root=str(meta.dataset_root),
        class_names=meta.class_names,
        num_classes=meta.num_classes,
    )

    if not meta.loaded:
        logger.warning(f"Metadata not loaded, skipping analysis. Error: {meta.error}")
        return ds_stats

    for split_name, split_path in meta.splits.items():
        split_stats = _analyze_split(split_name, split_path, meta.class_names)
        ds_stats.splits[split_name] = split_stats
        ds_stats.total_images += split_stats.images
        ds_stats.total_boxes += split_stats.annotations

        for cls_name, cnt in split_stats.class_distribution.items():
            ds_stats.global_class_distribution[cls_name] = (
                ds_stats.global_class_distribution.get(cls_name, 0) + cnt
            )

    logger.info(
        f"Analysis complete: {ds_stats.total_images} images, "
        f"{ds_stats.total_boxes} boxes across {len(ds_stats.splits)} splits."
    )
    return ds_stats
