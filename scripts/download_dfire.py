"""
Download D-Fire dataset (Kaggle version: sayedgamal99/smoke-fire-detection-yolo)
and place it into data/DFire/ with a correct data.yaml.

Usage:
    1. Get your Kaggle API key:
       - Go to https://www.kaggle.com/settings
       - Click "Create New Token" -> downloads kaggle.json
       - Place kaggle.json at: C:\\Users\\<your-username>\\.kaggle\\kaggle.json
         (Windows) or ~/.kaggle/kaggle.json (Linux/Mac)

    2. Run this script from the project root:
           python scripts/download_dfire.py

    3. Done! The orchestrator will pick it up automatically.
"""
from __future__ import annotations

import os
import shutil
import sys
import zipfile
from pathlib import Path

# ── Project root (two levels up from this script) ─────────────────────────────
PROJECT_ROOT = Path(__file__).resolve().parent.parent
DATA_DIR = PROJECT_ROOT / "data" / "DFire"

KAGGLE_DATASET = "sayedgamal99/smoke-fire-detection-yolo"
ZIP_NAME = "smoke-fire-detection-yolo.zip"


# ── Helpers ───────────────────────────────────────────────────────────────────

def _check_kaggle() -> None:
    try:
        import kaggle  # noqa: F401  – triggers credential validation
    except ImportError:
        print("[ERROR] kaggle package not installed. Run:  pip install kaggle")
        sys.exit(1)
    except Exception as exc:
        print(f"[ERROR] Kaggle credentials problem: {exc}")
        print("  -> Place kaggle.json in ~/.kaggle/ and chmod 600 it.")
        sys.exit(1)


def _download(dest: Path) -> Path:
    """Download and return path to the extracted directory."""
    import kaggle  # noqa: F811

    print(f"[INFO] Downloading '{KAGGLE_DATASET}' ...")
    dest.mkdir(parents=True, exist_ok=True)

    kaggle.api.dataset_download_files(
        KAGGLE_DATASET,
        path=str(dest),
        unzip=False,
        quiet=False,
    )

    zip_path = dest / ZIP_NAME
    if not zip_path.exists():
        # Find whatever zip was created
        zips = list(dest.glob("*.zip"))
        if not zips:
            raise FileNotFoundError("No zip file found after Kaggle download.")
        zip_path = zips[0]

    print(f"[INFO] Extracting {zip_path} ...")
    with zipfile.ZipFile(zip_path, "r") as zf:
        zf.extractall(dest)
    zip_path.unlink()
    print("[INFO] Extraction complete.")
    return dest


def _inspect_structure(base: Path) -> dict[str, Path | None]:
    """Return {'train': Path|None, 'val': Path|None, 'test': Path|None}."""
    splits: dict[str, Path | None] = {"train": None, "val": None, "test": None}

    # Common layouts:
    #   base/train/images, base/valid/images, base/test/images
    #   base/images/train, base/images/val, base/images/test
    for split_name, aliases in [
        ("train", ["train"]),
        ("val",   ["val", "valid", "validation"]),
        ("test",  ["test"]),
    ]:
        for alias in aliases:
            candidate_a = base / alias / "images"
            candidate_b = base / "images" / alias
            if candidate_a.exists():
                splits[split_name] = candidate_a
                break
            if candidate_b.exists():
                splits[split_name] = candidate_b
                break

    return splits


def _make_val_from_train(train_img_dir: Path, val_fraction: float = 0.15) -> Path:
    """
    If there is no val split, carve val_fraction of training images into
    data/DFire/val/images (and val/labels).
    """
    import random

    train_lbl_dir = train_img_dir.parent.parent / "labels" / "train"
    # Handle alternate layout: train_img_dir = base/train/images
    if not train_lbl_dir.exists():
        train_lbl_dir = train_img_dir.parent / "labels"

    val_img_dir = train_img_dir.parent.parent / "val" / "images"
    val_lbl_dir = train_img_dir.parent.parent / "val" / "labels"
    val_img_dir.mkdir(parents=True, exist_ok=True)
    val_lbl_dir.mkdir(parents=True, exist_ok=True)

    images = sorted(train_img_dir.glob("*.[jp][pn]g")) + sorted(
        train_img_dir.glob("*.jpeg")
    )
    random.seed(42)
    random.shuffle(images)
    n_val = max(1, int(len(images) * val_fraction))
    val_images = images[:n_val]

    print(
        f"[INFO] No val split found. Moving {n_val}/{len(images)} images to val ..."
    )
    for img_path in val_images:
        lbl_path = train_lbl_dir / (img_path.stem + ".txt")
        shutil.move(str(img_path), val_img_dir / img_path.name)
        if lbl_path.exists():
            shutil.move(str(lbl_path), val_lbl_dir / lbl_path.name)

    return val_img_dir


def _write_data_yaml(base: Path, splits: dict[str, Path | None]) -> None:
    """Write data/DFire/data.yaml with correct relative paths."""
    import yaml  # PyYAML, already in requirements.txt

    # Paths in data.yaml are relative to data.yaml location
    def _rel(p: Path | None) -> str | None:
        if p is None:
            return None
        try:
            return str(p.relative_to(base)).replace("\\", "/")
        except ValueError:
            return str(p).replace("\\", "/")

    data = {
        "path": str(base).replace("\\", "/"),  # absolute root
        "train": _rel(splits["train"]),
        "val":   _rel(splits["val"]),
        "test":  _rel(splits["test"]),
        "nc": 2,
        # D-Fire uses fire=0, smoke=1 — matches Model/best.pt exactly
        "names": ["fire", "smoke"],
    }
    # Remove None entries
    data = {k: v for k, v in data.items() if v is not None}

    yaml_path = base / "data.yaml"
    with open(yaml_path, "w", encoding="utf-8") as fh:
        yaml.dump(data, fh, sort_keys=False, allow_unicode=True)
    print(f"[INFO] data.yaml written to {yaml_path}")
    print(f"       classes: {data['names']}")
    for split in ("train", "val", "test"):
        if split in data:
            print(f"       {split}: {data[split]}")


def _count_images(splits: dict[str, Path | None]) -> None:
    for split, img_dir in splits.items():
        if img_dir and img_dir.exists():
            n = len(list(img_dir.glob("*.[jp][pn]g")) + list(img_dir.glob("*.jpeg")))
            print(f"[INFO]   {split}: {n} images at {img_dir}")


# ── Main ──────────────────────────────────────────────────────────────────────

def main() -> None:
    print("=" * 60)
    print("  D-Fire Dataset Downloader")
    print("  Source: Kaggle — sayedgamal99/smoke-fire-detection-yolo")
    print("=" * 60)

    _check_kaggle()

    # Download + extract
    base = _download(DATA_DIR)

    # Discover structure (handle nested subfolder from zip)
    # Sometimes the zip contains a single top-level folder
    subdirs = [p for p in base.iterdir() if p.is_dir()]
    if len(subdirs) == 1 and not (base / "train").exists() and not (base / "images").exists():
        base = subdirs[0]
        print(f"[INFO] Dataset root adjusted to: {base}")

    splits = _inspect_structure(base)
    print("\n[INFO] Detected splits:")
    _count_images(splits)

    # Create val split if missing
    if splits["val"] is None:
        if splits["train"] is None:
            print("[ERROR] Could not find train split. Please inspect data/DFire manually.")
            sys.exit(1)
        splits["val"] = _make_val_from_train(splits["train"])

    # Write data.yaml
    print()
    _write_data_yaml(DATA_DIR, splits)

    print("\n[OK] D-Fire dataset is ready at:  data/DFire/")
    print("     Run the orchestrator to verify:")
    print("       python -m src.pipeline.orchestrator")


if __name__ == "__main__":
    main()
