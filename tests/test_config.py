"""
Tests for ConfigLoader — validates all YAML configs load and have required keys.
No GPU, no MLflow, no external services needed.
"""
import pytest
from pathlib import Path

from src.core.config_loader import ConfigLoader, PROJECT_ROOT


@pytest.fixture
def cfg():
    return ConfigLoader()


# ── Load all configs ─────────────────────────────────────────────────────────

def test_load_all_configs(cfg):
    loaded = cfg.load_all()
    assert len(loaded) > 0
    for name, data in loaded.items():
        assert isinstance(data, dict), f"{name} should parse to a dict"


def test_app_config_required_keys(cfg):
    app = cfg.app
    for key in ("project_name", "environment", "logging_level", "seed"):
        assert key in app, f"app.yaml missing key: {key}"


def test_dataset_config_required_keys(cfg):
    ds = cfg.dataset
    for key in ("dataset_source", "dataset_path"):
        assert key in ds, f"dataset.yaml missing key: {key}"


def test_mlflow_config_required_keys(cfg):
    ml = cfg.mlflow
    for key in ("tracking_uri", "experiment_name", "minio_endpoint", "minio_bucket"):
        assert key in ml, f"mlflow.yaml missing key: {key}"


def test_model_config_required_keys(cfg):
    model = cfg.model
    for key in ("init_weights_source", "init_weights_repo", "init_weights_file",
                "init_weights_local_path", "model_family"):
        assert key in model, f"model.yaml missing key: {key}"


def test_training_config_loads(cfg):
    train = cfg.load("training.yaml")
    assert "epochs" in train
    assert "batch" in train
    assert isinstance(train["epochs"], int)


def test_monitoring_config_loads(cfg):
    mon = cfg.load("monitoring.yaml")
    assert "prometheus_url" in mon
    assert "grafana_url" in mon
    assert "evidently_url" in mon


# ── Path resolution ──────────────────────────────────────────────────────────

def test_resolve_path_returns_absolute(cfg):
    p = cfg.resolve_path("config/app.yaml")
    assert p.is_absolute()
    assert p.exists()


def test_project_root_is_correct():
    assert (PROJECT_ROOT / "config").exists(), "PROJECT_ROOT should contain config/"
    assert (PROJECT_ROOT / "src").exists(), "PROJECT_ROOT should contain src/"


# ── Config caching ───────────────────────────────────────────────────────────

def test_config_is_cached(cfg):
    a = cfg.app
    b = cfg.app
    assert a is b, "ConfigLoader should cache loaded configs"


# ── Missing file raises ──────────────────────────────────────────────────────

def test_missing_config_raises(cfg):
    with pytest.raises(FileNotFoundError):
        cfg.load("nonexistent_config.yaml")
