# ────────────────────────────────────────────────────────────────────────────
# Fire Detection API — Docker image
#
# Build:
#   docker build -t fire-detection-api .
#
# Run (CPU):
#   docker run -p 8000:8000 \
#     -v $(pwd)/Model:/app/Model:ro \
#     -v $(pwd)/config:/app/config:ro \
#     fire-detection-api
#
# Run (GPU — requires NVIDIA Container Toolkit):
#   docker run --gpus all -p 8000:8000 \
#     -v $(pwd)/Model:/app/Model:ro \
#     -v $(pwd)/config:/app/config:ro \
#     fire-detection-api
#
# Environment variables:
#   API_KEY          — Bearer token for write endpoints (optional)
#   MLFLOW_TRACKING_URI — override config/mlflow.yaml value
# ────────────────────────────────────────────────────────────────────────────

FROM python:3.11-slim

# System libraries required by OpenCV + ultralytics
RUN apt-get update && apt-get install -y --no-install-recommends \
        libgl1 \
        libglib2.0-0 \
        libgomp1 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# ── Python dependencies ──────────────────────────────────────────────────────
# Copy requirements first to leverage Docker layer cache
COPY requirements.txt .

# Install framework + API deps (torch/ultralytics are commented-out in
# requirements.txt — install CPU versions explicitly here so the image
# builds without a GPU and stays ~2 GB lighter than CUDA-bundled images).
# At runtime, CUDA is provided by the NVIDIA container runtime.
RUN pip install --no-cache-dir -r requirements.txt && \
    pip install --no-cache-dir \
        torch torchvision --index-url https://download.pytorch.org/whl/cpu && \
    pip install --no-cache-dir ultralytics>=8.2

# ── Application source ───────────────────────────────────────────────────────
COPY src/    ./src/
COPY config/ ./config/

# Model weights are NOT baked into the image (64 MB .pt file, gitignored).
# Mount at runtime:  -v /path/to/Model:/app/Model:ro
# OR set init_weights_local_path in config/model.yaml to a mounted path.
RUN mkdir -p Model

# ── Runtime ──────────────────────────────────────────────────────────────────
EXPOSE 8000

# Healthcheck — allows Docker/K8s to detect unhealthy containers
HEALTHCHECK --interval=30s --timeout=10s --start-period=30s --retries=3 \
    CMD python -c "import urllib.request; urllib.request.urlopen('http://localhost:8000/health')"

CMD ["python", "-m", "src.api.main"]
