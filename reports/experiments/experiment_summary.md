# Experiment Summary

_Generated: 2026-03-23T09:41:57.067713+00:00_  
**Experiment:** `fire-detection`  

**Total runs shown:** 20 (of last 20)

| Run ID | Name | Status | Started | Duration (s) | dataset_project | dataset_version | init_weights_source | init_weights_local_path | git_commit | cuda_available | gpu_name | framework_smoke_metric | val_map50 | val_map50_95 | train_loss |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| `fd33cd77` | evaluate_test | FINISHED | 2026-03-23 09:41 UTC | 0.1 | — | — | — | — | — | — | — | — | — | — | — |
| `f88f8db3` | framework-smoke | FINISHED | 2026-03-23 09:40 UTC | 1.4 | — |  | huggingface | runs/train/dfire_finetune_2... |  | True | NVIDIA GeForce RTX 3050 6GB... | 1.0 | — | — | — |
| `a36177bc` | model-register | FINISHED | 2026-03-23 07:59 UTC | 0.3 | — | — | — | — | — | — | — | — | — | — | — |
| `5f96e87b` | dfire_finetune_20260323_045812 | FINISHED | 2026-03-23 04:58 UTC | 10830.8 | — | — | — | — | — | — | — | — | — | — | — |
| `8f2a89c1` | framework-smoke | FINISHED | 2026-03-23 04:58 UTC | 0.9 | — |  | huggingface | runs/train/dfire_finetune_2... |  | True | NVIDIA GeForce RTX 3050 6GB... | 1.0 | — | — | — |
| `fc8b3649` | model-register | FINISHED | 2026-03-23 04:49 UTC | 0.7 | — | — | — | — | — | — | — | — | — | — | — |
| `bbabed6a` | evaluate_test | FINISHED | 2026-03-23 04:49 UTC | 0.1 | — | — | — | — | — | — | — | — | — | — | — |
| `c13967f5` | model-register | FINISHED | 2026-03-23 04:47 UTC | 1.5 | — | — | — | — | — | — | — | — | — | — | — |
| `85f56e8f` | framework-smoke | FINISHED | 2026-03-23 04:47 UTC | 0.9 | — |  | huggingface | runs/train/dfire_finetune_2... |  | True | NVIDIA GeForce RTX 3050 6GB... | 1.0 | — | — | — |
| `39894a02` | framework-smoke | FINISHED | 2026-03-23 04:42 UTC | 1.2 | — |  | huggingface | runs/train/dfire_finetune_2... |  | True | NVIDIA GeForce RTX 3050 6GB... | 1.0 | — | — | — |
| `b0c492fa` | model-register | FINISHED | 2026-03-23 04:31 UTC | 1.2 | — | — | — | — | — | — | — | — | — | — | — |
| `f10e849b` | framework-smoke | FINISHED | 2026-03-23 04:08 UTC | 1.0 | — |  | huggingface | runs/train/dfire_finetune_2... |  | True | NVIDIA GeForce RTX 3050 6GB... | 1.0 | — | — | — |
| `d1913a83` | framework-smoke | FINISHED | 2026-03-23 03:42 UTC | 1.4 | — |  | huggingface | runs/train/dfire_finetune_2... |  | True | NVIDIA GeForce RTX 3050 6GB... | 1.0 | — | — | — |
| `a7c522d2` | framework-smoke | FINISHED | 2026-03-23 03:06 UTC | 0.8 | — |  | huggingface | runs/train/dfire_finetune_2... |  | True | NVIDIA GeForce RTX 3050 6GB... | 1.0 | — | — | — |
| `636ceace` | model-register | FINISHED | 2026-03-23 03:00 UTC | 0.9 | — | — | — | — | — | — | — | — | — | — | — |
| `0908f90a` | evaluate_test | FINISHED | 2026-03-23 03:00 UTC | 0.1 | — | — | — | — | — | — | — | — | — | — | — |
| `ffabcb5b` | framework-smoke | FINISHED | 2026-03-23 02:58 UTC | 0.7 | — |  | huggingface | runs/train/dfire_finetune_2... |  | True | NVIDIA GeForce RTX 3050 6GB... | 1.0 | — | — | — |
| `70866364` | model-register | FINISHED | 2026-03-23 02:58 UTC | 1.1 | — | — | — | — | — | — | — | — | — | — | — |
| `bdf39974` | evaluate_test | FINISHED | 2026-03-23 02:58 UTC | 0.1 | — | — | — | — | — | — | — | — | — | — | — |
| `8cb49cf0` | framework-smoke | FINISHED | 2026-03-23 02:55 UTC | 1.0 | — |  | huggingface | runs/train/dfire_finetune_2... |  | True | NVIDIA GeForce RTX 3050 6GB... | 1.0 | — | — | — |

## Tags

- `fd33cd77` — `phase=evaluate`, `split=test`, `weights=best.pt`
- `f88f8db3` — `phase=framework`, `stage=smoke`
- `a36177bc` — `dataset=DFire`, `mAP50=0.0`, `phase=register`, `weights=runs\train\dfire_finetune_20260323_045812_20260323_045812\weights\best.pt`
- `5f96e87b` — `dataset=DFire`, `freeze_layers=5`, `phase=train`, `stage=finetune`
- `8f2a89c1` — `phase=framework`, `stage=smoke`
- `fc8b3649` — `dataset=DFire`, `mAP50=0.624`, `phase=register`, `weights=runs\train\dfire_finetune_20260323_040840_20260323_040840\weights\best.pt`
- `bbabed6a` — `phase=evaluate`, `split=test`, `weights=best.pt`
- `c13967f5` — `dataset=DFire`, `mAP50=0.0`, `phase=register`, `weights=runs\train\dfire_finetune_20260323_040840_20260323_040840\weights\best.pt`
- `85f56e8f` — `phase=framework`, `stage=smoke`
- `39894a02` — `phase=framework`, `stage=smoke`
- `b0c492fa` — `dataset=DFire`, `mAP50=0.0`, `phase=register`, `weights=runs\train\dfire_finetune_20260323_040840_20260323_040840\weights\best.pt`
- `f10e849b` — `phase=framework`, `stage=smoke`
- `d1913a83` — `phase=framework`, `stage=smoke`
- `a7c522d2` — `phase=framework`, `stage=smoke`
- `636ceace` — `dataset=DFire`, `mAP50=0.6076`, `phase=register`, `weights=runs\train\dfire_finetune_20260318_124053_20260318_124053\weights\best.pt`
- `0908f90a` — `phase=evaluate`, `split=test`, `weights=best.pt`
- `ffabcb5b` — `phase=framework`, `stage=smoke`
- `70866364` — `dataset=DFire`, `mAP50=0.6076`, `phase=register`, `weights=runs\train\dfire_finetune_20260318_124053_20260318_124053\weights\best.pt`
- `bdf39974` — `phase=evaluate`, `split=test`, `weights=best.pt`
- `8cb49cf0` — `phase=framework`, `stage=smoke`
