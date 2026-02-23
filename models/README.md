# YOLOv26 Fire Detection Model

Model phát hiện lửa và khói real-time dựa trên **YOLOv26-S** (Ultralytics). Đạt **94.9% mAP@50** trên bài toán phát hiện fire/smoke.

## Thông tin model

| Thuộc tính | Giá trị |
|------------|--------|
| Base model | YOLOv26-S |
| Task | Object detection |
| Classes | **fire**, **smoke**, **other** (chỉ số liên quan cháy) |
| Input size | 640×640 |
| Epochs | 100 |
| Dataset | 8,939 ảnh đã gán nhãn |

### Hiệu năng

| Metric | Score |
|--------|--------|
| mAP@50 | **94.9%** |
| mAP@50-95 | 68.0% |
| Precision | 89.6% |
| Recall | 88.8% |

### Classes

- **fire** (0) – Ngọn lửa
- **smoke** (1) – Khói
- **other** (2) – Chỉ số liên quan cháy (các dấu hiệu khác)

---

## Export sang ONNX

Hệ thống detector (Rust) chạy file **best.onnx**. Cần export từ **best.pt**:

```bash
cd models
pip install -r requirements.txt
python export_onnx.py --weights best.pt --output best.onnx --imgsz 640 --simplify --validate
```

Hoặc dùng script (PowerShell):

```powershell
.\models\export_and_validate.ps1
```

Export xong, file `best.onnx` phải nằm trong thư mục `models/`. Cấu hình trong `configs/settings.yaml`:

```yaml
inference:
  model_path: "/app/models/best.onnx"
  device: "cpu"   # hoặc "cuda:0"
  warmup_frames: 3
```

---

## Kiểm tra ONNX (sau khi export)

```bash
python export_onnx.py --weights best.pt --output best.onnx --validate
```

Kỳ vọng:

- Input: `images`, shape `[1, 3, 640, 640]`
- Output: end-to-end thường `[1, 300, 6]` (x1, y1, x2, y2, conf, class_id) với class_id ∈ {0, 1, 2}

---

## Dùng với Python (run_inference.py)

```python
from ultralytics import YOLO

model = YOLO("models/best.pt")
results = model.predict("image.jpg", conf=0.25)

for result in results:
    for box in result.boxes:
        cls = int(box.cls[0])
        conf = float(box.conf[0])
        label = model.names[cls]  # "fire", "smoke", "other"
        print(f"Detected: {label} ({conf:.2f})")
```

Video / webcam: dùng `source="video.mp4"` hoặc `source=0`, tham số `conf` tương tự.

---

## Ghi chú

- **YOLOv26** tương thích export qua Ultralytics giống YOLOv10/v11.
- Nếu export báo thiếu `onnxslim` / `onnxruntime-gpu`, có thể cài:  
  `pip install onnx onnxruntime` (CPU) hoặc `onnxruntime-gpu` (GPU).
- Detector Rust hỗ trợ đủ 3 class: fire, smoke, other; ngưỡng confidence cấu hình trong `configs/settings.yaml` và `configs/cameras.yaml`.
