"""
Dataset report exporter: writes dataset_report.md and dataset_stats.json.
Handles missing dataset gracefully — always produces output files.
"""
from __future__ import annotations

import json
from datetime import datetime, timezone
from pathlib import Path
from typing import Optional

from src.core.config_loader import PROJECT_ROOT
from src.core.logger import get_logger
from src.dataset.analyzer import DatasetStats
from src.dataset.metadata import DatasetMetadata
from src.dataset.validator import ValidationReport

logger = get_logger()

REPORT_DIR = PROJECT_ROOT / "reports" / "evaluation"
REPORT_DIR.mkdir(parents=True, exist_ok=True)

REPORT_MD_PATH = REPORT_DIR / "dataset_report.md"
REPORT_JSON_PATH = REPORT_DIR / "dataset_stats.json"


def _format_class_dist(dist: dict[str, int]) -> str:
    if not dist:
        return "_No annotations found._"
    rows = ["| Class | Count |", "|-------|-------|"]
    total = sum(dist.values())
    for name, cnt in sorted(dist.items(), key=lambda x: -x[1]):
        pct = cnt / total * 100 if total else 0
        rows.append(f"| {name} | {cnt} ({pct:.1f}%) |")
    return "\n".join(rows)


def _format_boxes_stats(stats: dict[str, float]) -> str:
    if not stats:
        return "_No box statistics available._"
    return (
        f"mean={stats.get('mean', 'N/A')}  "
        f"median={stats.get('median', 'N/A')}  "
        f"min={stats.get('min', 'N/A')}  "
        f"max={stats.get('max', 'N/A')}  "
        f"std={stats.get('std', 'N/A')}"
    )


def export(
    meta: DatasetMetadata,
    validation: Optional[ValidationReport],
    stats: Optional[DatasetStats],
) -> tuple[Path, Path]:
    """Write markdown report and JSON stats. Returns (md_path, json_path)."""
    now = datetime.now(timezone.utc).isoformat()

    # ---------------------------------------------------------------
    # Markdown report
    # ---------------------------------------------------------------
    md_lines: list[str] = [
        "# Dataset Report",
        f"\n_Generated: {now}_\n",
        f"**Dataset root:** `{meta.dataset_root}`  ",
        f"**data.yaml present:** {'Yes' if meta.data_yaml_path.exists() else 'No'}  ",
    ]

    if meta.error or not meta.loaded:
        md_lines += [
            "\n## Dataset Not Available\n",
            f"> **Error:** {meta.error or 'data.yaml could not be loaded.'}\n",
            "The dataset has not been downloaded yet.",
            "Download from Roboflow using the `dataset_source` configured in `config/dataset.yaml`.",
            "\n```",
            "# Example using Roboflow Python SDK:",
            "from roboflow import Roboflow",
            "rf = Roboflow(api_key=YOUR_API_KEY)",
            'project = rf.workspace().project("fire-dji3l")',
            'dataset = project.version(4).download("yolov8")',
            "```\n",
        ]
    else:
        # Classes
        md_lines += [
            "\n## Classes\n",
            f"**Total classes:** {meta.num_classes}  ",
            f"**Names:** {', '.join(meta.class_names) or '_none_'}  ",
        ]

        # Validation
        if validation:
            status = "PASSED" if validation.passed else "FAILED"
            md_lines += [
                "\n## Validation\n",
                f"**Status:** {status}  ",
            ]
            if validation.global_errors:
                md_lines.append("\n**Global errors:**")
                for err in validation.global_errors:
                    md_lines.append(f"- {err}")
            for split_name, r in validation.splits.items():
                md_lines += [
                    f"\n### {split_name}",
                    f"- Images found: {r.images_found}",
                    f"- Labels found: {r.labels_found}",
                    f"- Missing labels: {len(r.missing_labels)}",
                    f"- Corrupted images: {len(r.corrupted_images)}",
                    f"- Empty annotation rate: {r.empty_annotation_rate:.2%}",
                ]
                if r.errors:
                    md_lines.append("- Errors:")
                    for err in r.errors:
                        md_lines.append(f"  - {err}")

        # Stats
        if stats:
            md_lines += [
                "\n## Statistics\n",
                f"**Total images:** {stats.total_images}  ",
                f"**Total boxes:** {stats.total_boxes}  ",
                "\n### Global Class Distribution\n",
                _format_class_dist(stats.global_class_distribution),
            ]
            for split_name, ss in stats.splits.items():
                if ss.error:
                    md_lines.append(f"\n### {split_name} — ERROR: {ss.error}")
                    continue
                md_lines += [
                    f"\n### {split_name}",
                    f"- Images: {ss.images}",
                    f"- Annotations: {ss.annotations}",
                    f"- Boxes per image: {_format_boxes_stats(ss.boxes_per_image)}",
                    "\n**Class distribution:**\n",
                    _format_class_dist(ss.class_distribution),
                ]

    REPORT_MD_PATH.write_text("\n".join(md_lines) + "\n", encoding="utf-8")
    logger.info(f"Dataset report written to {REPORT_MD_PATH}")

    # ---------------------------------------------------------------
    # JSON stats
    # ---------------------------------------------------------------
    json_data: dict = {
        "generated_utc": now,
        "dataset_root": str(meta.dataset_root),
        "data_yaml_present": meta.data_yaml_path.exists(),
        "loaded": meta.loaded,
        "error": meta.error,
        "class_names": meta.class_names,
        "num_classes": meta.num_classes,
        "validation": None,
        "stats": None,
    }

    if validation:
        json_data["validation"] = {
            "passed": validation.passed,
            "global_errors": validation.global_errors,
            "splits": {
                split_name: {
                    "images_found": r.images_found,
                    "labels_found": r.labels_found,
                    "missing_labels_count": len(r.missing_labels),
                    "corrupted_images_count": len(r.corrupted_images),
                    "empty_annotations_count": len(r.empty_annotations),
                    "empty_annotation_rate": round(r.empty_annotation_rate, 4),
                    "errors": r.errors,
                }
                for split_name, r in validation.splits.items()
            },
        }

    if stats:
        json_data["stats"] = stats.to_dict()

    REPORT_JSON_PATH.write_text(
        json.dumps(json_data, indent=2, default=str),
        encoding="utf-8",
    )
    logger.info(f"Dataset stats JSON written to {REPORT_JSON_PATH}")

    return REPORT_MD_PATH, REPORT_JSON_PATH
