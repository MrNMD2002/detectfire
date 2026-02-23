# Export YOLOv10/YOLOv26 (Ultralytics) model to ONNX format
# Usage: python export_onnx.py --weights best.pt --output best.onnx

import argparse
import torch
from pathlib import Path

def export_yolov10_onnx(
    weights_path: str,
    output_path: str,
    imgsz: int = 640,
    opset: int = 12,
    simplify: bool = True,
    half: bool = False,
    dynamic: bool = False
):
    """Export YOLOv10 model to ONNX format."""
    
    print(f"Loading model from {weights_path}...")
    
    # Try ultralytics first (YOLOv10)
    try:
        from ultralytics import YOLO
        model = YOLO(weights_path)
        
        # Export using ultralytics
        export_path = model.export(
            format="onnx",
            imgsz=imgsz,
            opset=opset,
            simplify=simplify,
            half=half,
            dynamic=dynamic
        )
        
        # Move to output path if different
        if export_path != output_path:
            import shutil
            shutil.move(export_path, output_path)
        
        print(f"✅ Exported to {output_path}")
        return
    except ImportError:
        print("ultralytics not found, trying torch hub...")
    
    # Fallback: manual export
    model = torch.load(weights_path, map_location='cpu')
    if isinstance(model, dict):
        model = model.get('model', model.get('ema', model))
    
    model.eval()
    
    # Create dummy input
    dummy_input = torch.randn(1, 3, imgsz, imgsz)
    
    # Dynamic axes for batch size
    dynamic_axes = None
    if dynamic:
        dynamic_axes = {
            'images': {0: 'batch'},
            'output': {0: 'batch'}
        }
    
    # Export
    torch.onnx.export(
        model,
        dummy_input,
        output_path,
        opset_version=opset,
        input_names=['images'],
        output_names=['output'],
        dynamic_axes=dynamic_axes,
        do_constant_folding=True
    )
    
    print(f"✅ Exported to {output_path}")
    
    # Simplify if requested
    if simplify:
        try:
            import onnxsim
            import onnx
            
            model_onnx = onnx.load(output_path)
            model_simp, check = onnxsim.simplify(model_onnx)
            
            if check:
                onnx.save(model_simp, output_path)
                print("✅ Model simplified")
            else:
                print("⚠️ Simplification failed, keeping original")
        except ImportError:
            print("⚠️ onnxsim not installed, skipping simplification")

def validate_onnx(model_path: str, imgsz: int = 640):
    """Validate ONNX model."""
    import onnxruntime as ort
    import numpy as np
    
    print(f"Validating {model_path}...")
    
    # Create session
    providers = ['CUDAExecutionProvider', 'CPUExecutionProvider']
    session = ort.InferenceSession(model_path, providers=providers)
    
    # Get input info
    input_info = session.get_inputs()[0]
    print(f"  Input: {input_info.name}, shape: {input_info.shape}, type: {input_info.type}")
    
    # Get output info
    for output in session.get_outputs():
        print(f"  Output: {output.name}, shape: {output.shape}, type: {output.type}")
    
    # Test inference
    dummy_input = np.random.randn(1, 3, imgsz, imgsz).astype(np.float32)
    outputs = session.run(None, {input_info.name: dummy_input})
    
    print(f"  Test output shape: {outputs[0].shape}")
    print("✅ Validation passed")

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Export YOLOv10 to ONNX")
    parser.add_argument("--weights", type=str, required=True, help="Path to .pt weights")
    parser.add_argument("--output", type=str, default="best.onnx", help="Output ONNX path (must match configs/settings.yaml)")
    parser.add_argument("--imgsz", type=int, default=640, help="Image size")
    parser.add_argument("--opset", type=int, default=12, help="ONNX opset version")
    parser.add_argument("--simplify", action="store_true", help="Simplify ONNX model")
    parser.add_argument("--half", action="store_true", help="Export FP16")
    parser.add_argument("--dynamic", action="store_true", help="Dynamic batch size")
    parser.add_argument("--validate", action="store_true", help="Validate after export")
    
    args = parser.parse_args()
    
    export_yolov10_onnx(
        weights_path=args.weights,
        output_path=args.output,
        imgsz=args.imgsz,
        opset=args.opset,
        simplify=args.simplify,
        half=args.half,
        dynamic=args.dynamic
    )
    
    if args.validate:
        validate_onnx(args.output, args.imgsz)
