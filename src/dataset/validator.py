"""
Dataset validator: checks YOLOv8 layout, image/label pairing, corruption.
"""
from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path
from typing import Optional

from src.core.logger import get_logger
from src.dataset.metadata import DatasetMetadata

logger = get_logger()

IMAGE_EXTENSIONS = {".jpg", ".jpeg", ".png", ".bmp", ".tiff", ".webp"}


@dataclass
class SplitValidationResult:
    split: str
    images_path: Path | None
    labels_path: Path | None
    images_found: int = 0
    labels_found: int = 0
    missing_labels: list[str] = field(default_factory=list)
    missing_images: list[str] = field(default_factory=list)
    corrupted_images: list[str] = field(default_factory=list)
    empty_annotations: list[str] = field(default_factory=list)
    errors: list[str] = field(default_factory=list)

    @property
    def empty_annotation_rate(self) -> float:
        if self.images_found == 0:
            return 0.0
        return len(self.empty_annotations) / self.images_found


@dataclass
class ValidationReport:
    dataset_root: Path
    data_yaml_present: bool = False
    splits: dict[str, SplitValidationResult] = field(default_factory=dict)
    global_errors: list[str] = field(default_factory=list)
    passed: bool = False

    def summary(self) -> str:
        lines = [f"Dataset: {self.dataset_root}", f"data.yaml present: {self.data_yaml_present}"]
        for split, r in self.splits.items():
            lines.append(
                f"  [{split}] images={r.images_found} labels={r.labels_found} "
                f"missing_labels={len(r.missing_labels)} "
                f"corrupted={len(r.corrupted_images)} "
                f"empty_ann_rate={r.empty_annotation_rate:.2%}"
            )
        if self.global_errors:
            lines.append("Global errors:")
            for e in self.global_errors:
                lines.append(f"  - {e}")
        lines.append(f"Passed: {self.passed}")
        return "\n".join(lines)


def _check_image(path: Path) -> bool:
    """Return True if the image file can be opened without error."""
    try:
        from PIL import Image  # type: ignore
        with Image.open(path) as img:
            img.verify()
        return True
    except ImportError:
        # PIL not available; do a basic binary read check
        try:
            data = path.read_bytes()
            return len(data) > 0
        except Exception:
            return False
    except Exception:
        return False


def _validate_split(
    split_name: str,
    split_root: Path | None,
) -> SplitValidationResult:
    images_path = split_root / "images" if split_root else None
    labels_path = split_root / "labels" if split_root else None

    result = SplitValidationResult(
        split=split_name,
        images_path=images_path,
        labels_path=labels_path,
    )

    if split_root is None:
        result.errors.append(f"No path configured for split '{split_name}'")
        return result

    if not split_root.exists():
        result.errors.append(f"Split directory not found: {split_root}")
        return result

    if not images_path or not images_path.exists():
        result.errors.append(f"images/ directory not found under {split_root}")
        return result

    if not labels_path or not labels_path.exists():
        result.errors.append(f"labels/ directory not found under {split_root}")
        return result

    image_files = [
        f for f in images_path.iterdir()
        if f.is_file() and f.suffix.lower() in IMAGE_EXTENSIONS
    ]
    label_files = {f.stem: f for f in labels_path.iterdir() if f.suffix == ".txt"}

    result.images_found = len(image_files)
    result.labels_found = len(label_files)

    for img in image_files:
        # Check paired label
        if img.stem not in label_files:
            result.missing_labels.append(img.name)
        else:
            # Check for empty annotation
            lbl_path = label_files[img.stem]
            try:
                content = lbl_path.read_text(encoding="utf-8").strip()
                if not content:
                    result.empty_annotations.append(img.name)
            except Exception as exc:
                result.errors.append(f"Cannot read label {lbl_path.name}: {exc}")

        # Basic corruption check
        if not _check_image(img):
            result.corrupted_images.append(img.name)

    # Labels without images
    image_stems = {f.stem for f in image_files}
    for stem in label_files:
        if stem not in image_stems:
            result.missing_images.append(f"{stem}.txt")

    logger.debug(
        f"[{split_name}] images={result.images_found} labels={result.labels_found} "
        f"missing_labels={len(result.missing_labels)} corrupted={len(result.corrupted_images)}"
    )
    return result


def validate(meta: DatasetMetadata) -> ValidationReport:
    """Run full validation on the dataset described by meta."""
    report = ValidationReport(
        dataset_root=meta.dataset_root,
        data_yaml_present=meta.data_yaml_path.exists(),
    )

    if not meta.loaded:
        report.global_errors.append(
            meta.error or "data.yaml could not be loaded; validation skipped."
        )
        return report

    # Determine splits to validate
    PRIORITY_SPLITS = ["train", "val", "valid", "test"]
    splits_to_check = {
        k: v for k, v in meta.splits.items() if k in PRIORITY_SPLITS
    }

    # Require at least train + (val or valid)
    has_train = bool(splits_to_check.get("train"))
    has_val = bool(splits_to_check.get("val") or splits_to_check.get("valid"))

    if not has_train:
        report.global_errors.append("'train' split is missing or not configured in data.yaml")
    if not has_val:
        report.global_errors.append("'val'/'valid' split is missing or not configured in data.yaml")

    for split_name, split_path in splits_to_check.items():
        result = _validate_split(split_name, split_path)
        report.splits[split_name] = result

    # Overall pass/fail (no global errors, no per-split errors, no corrupted images)
    no_global_errors = len(report.global_errors) == 0
    no_split_errors = all(
        len(r.errors) == 0 and len(r.corrupted_images) == 0
        for r in report.splits.values()
    )
    report.passed = no_global_errors and no_split_errors

    logger.info(f"Validation complete. Passed={report.passed}")
    return report
