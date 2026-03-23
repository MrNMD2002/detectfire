"""
Dataset metadata: reads data.yaml and returns class names, split paths, etc.
"""
from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path
from typing import Optional

import yaml

from src.core.logger import get_logger

logger = get_logger()


@dataclass
class DatasetMetadata:
    data_yaml_path: Path
    dataset_root: Path
    class_names: list[str] = field(default_factory=list)
    num_classes: int = 0
    splits: dict[str, Path | None] = field(default_factory=dict)
    raw: dict = field(default_factory=dict)
    loaded: bool = False
    error: str | None = None


def load_metadata(dataset_path: Path) -> DatasetMetadata:
    """Parse data.yaml from a YOLOv8 dataset folder.

    Returns a DatasetMetadata even if data.yaml is missing or malformed;
    the `error` field will describe the problem.
    """
    data_yaml = dataset_path / "data.yaml"
    meta = DatasetMetadata(data_yaml_path=data_yaml, dataset_root=dataset_path)

    if not dataset_path.exists():
        meta.error = f"Dataset path does not exist: {dataset_path}"
        logger.warning(meta.error)
        return meta

    if not data_yaml.exists():
        meta.error = f"data.yaml not found at {data_yaml}"
        logger.warning(meta.error)
        return meta

    try:
        with open(data_yaml, encoding="utf-8") as fh:
            raw = yaml.safe_load(fh) or {}
        meta.raw = raw

        # Class names
        names = raw.get("names", [])
        if isinstance(names, dict):
            names = [names[k] for k in sorted(names.keys())]
        meta.class_names = names
        meta.num_classes = raw.get("nc", len(names))

        # Split paths (relative entries in data.yaml resolved against dataset root)
        for split in ("train", "val", "valid", "test"):
            val = raw.get(split)
            if val:
                p = Path(val)
                if not p.is_absolute():
                    p = dataset_path / p
                meta.splits[split] = p
            else:
                meta.splits[split] = None

        meta.loaded = True
        logger.info(
            f"data.yaml loaded: {meta.num_classes} classes, "
            f"splits={list(k for k, v in meta.splits.items() if v)}"
        )
    except Exception as exc:
        meta.error = f"Failed to parse data.yaml: {exc}"
        logger.error(meta.error)

    return meta
