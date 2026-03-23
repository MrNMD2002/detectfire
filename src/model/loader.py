"""
Load YOLOv10 / YOLOv8 model from a checkpoint file via Ultralytics.
"""
from __future__ import annotations

from pathlib import Path

from src.core.logger import get_logger

logger = get_logger()


def load_model(weights_path: Path | str):
    """Return an Ultralytics YOLO model from *weights_path*.

    Raises FileNotFoundError if the file is missing.
    """
    from ultralytics import YOLO

    path = Path(weights_path)
    if not path.exists():
        raise FileNotFoundError(f"Model weights not found: {path}")

    model = YOLO(str(path))
    logger.info(f"Model loaded from {path} ({path.stat().st_size / 1e6:.1f} MB)")
    return model
