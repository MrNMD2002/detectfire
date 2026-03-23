"""
Tests for pipeline stages 1–3 (framework stages — no GPU, no MLflow, no dataset needed).

Stage 1 — LoadConfig       : loads YAML into ctx
Stage 2 — EnvFingerprint   : collects OS/Python/GPU info
Stage 3 — InitWeightsManifest: checks best.pt exists (non-fatal if missing)
"""
import json
import pytest
from pathlib import Path

from src.core.config_loader import ConfigLoader
from src.pipeline.stages import (
    LoadConfigStage,
    EnvFingerprintStage,
    InitWeightsManifestStage,
)


@pytest.fixture
def cfg():
    return ConfigLoader()


@pytest.fixture
def ctx(cfg):
    """Empty context dict — stages mutate this."""
    return {}


# ── Stage 1 — LoadConfig ─────────────────────────────────────────────────────

class TestLoadConfigStage:
    def test_returns_true(self, cfg, ctx):
        stage = LoadConfigStage(cfg)
        assert stage.run(ctx) is True

    def test_populates_context_keys(self, cfg, ctx):
        LoadConfigStage(cfg).run(ctx)
        for key in ("app_cfg", "dataset_cfg", "mlflow_cfg", "model_cfg", "env_cfg"):
            assert key in ctx, f"ctx missing key: {key}"

    def test_app_cfg_has_project_name(self, cfg, ctx):
        LoadConfigStage(cfg).run(ctx)
        assert "project_name" in ctx["app_cfg"]


# ── Stage 2 — EnvFingerprint ─────────────────────────────────────────────────

class TestEnvFingerprintStage:
    def test_returns_true(self, cfg, ctx):
        stage = EnvFingerprintStage(cfg)
        assert stage.run(ctx) is True

    def test_fingerprint_in_ctx(self, cfg, ctx):
        EnvFingerprintStage(cfg).run(ctx)
        assert "env_fingerprint" in ctx
        fp = ctx["env_fingerprint"]
        assert isinstance(fp, dict)

    def test_fingerprint_has_python_key(self, cfg, ctx):
        EnvFingerprintStage(cfg).run(ctx)
        fp = ctx["env_fingerprint"]
        assert "python" in fp or "python_version" in fp or any(
            "python" in k for k in fp
        ), f"Expected python info in fingerprint, got keys: {list(fp.keys())}"

    def test_fingerprint_file_written(self, cfg, ctx):
        EnvFingerprintStage(cfg).run(ctx)
        path = ctx.get("env_fingerprint_path")
        assert path is not None
        assert Path(path).exists()

    def test_fingerprint_file_is_valid_json(self, cfg, ctx):
        EnvFingerprintStage(cfg).run(ctx)
        path = Path(ctx["env_fingerprint_path"])
        data = json.loads(path.read_text(encoding="utf-8"))
        assert isinstance(data, dict)


# ── Stage 3 — InitWeightsManifest ────────────────────────────────────────────

class TestInitWeightsManifestStage:
    def test_returns_true_even_if_weights_missing(self, cfg, ctx):
        """Stage is always non-fatal — returns True regardless of file presence."""
        LoadConfigStage(cfg).run(ctx)
        stage = InitWeightsManifestStage(cfg)
        assert stage.run(ctx) is True

    def test_manifest_in_ctx(self, cfg, ctx):
        LoadConfigStage(cfg).run(ctx)
        InitWeightsManifestStage(cfg).run(ctx)
        assert "init_weights_manifest" in ctx
        manifest = ctx["init_weights_manifest"]
        assert isinstance(manifest, dict)

    def test_manifest_has_required_keys(self, cfg, ctx):
        LoadConfigStage(cfg).run(ctx)
        InitWeightsManifestStage(cfg).run(ctx)
        manifest = ctx["init_weights_manifest"]
        for key in ("present", "local_path", "model_family", "generated_utc"):
            assert key in manifest, f"manifest missing key: {key}"

    def test_manifest_file_written(self, cfg, ctx):
        LoadConfigStage(cfg).run(ctx)
        InitWeightsManifestStage(cfg).run(ctx)
        path = ctx.get("init_weights_manifest_path")
        assert path is not None
        assert Path(path).exists()

    def test_manifest_file_is_valid_json(self, cfg, ctx):
        LoadConfigStage(cfg).run(ctx)
        InitWeightsManifestStage(cfg).run(ctx)
        path = Path(ctx["init_weights_manifest_path"])
        data = json.loads(path.read_text(encoding="utf-8"))
        assert isinstance(data, dict)
        assert "present" in data
