# MLflow Infrastructure

Runs MLflow tracking server backed by PostgreSQL and MinIO (S3-compatible artifact store).

## Services

| Service   | Port(s)       | Description                        |
|-----------|---------------|------------------------------------|
| postgres  | 5432          | MLflow backend store               |
| minio     | 9000 / 9001   | Artifact store (S3-compatible)     |
| mlflow    | 5000          | Tracking server UI + REST API      |

## Quick Start

### 1. Start all services

```bash
cd infra/mlflow
docker compose up -d
```

Wait ~30 seconds for all health checks to pass.

### 2. Create the MLflow artifact bucket in MinIO

Open the MinIO console: http://localhost:9001

- Login: `minioadmin` / `minioadmin`
- Click **Buckets** → **Create Bucket**
- Name: `mlflow`
- Click **Create Bucket**

> **This step is required before running any MLflow experiment.**
> Without the `mlflow` bucket the tracking server cannot store artifacts.

### 3. Verify MLflow is running

Open the MLflow UI: http://localhost:5000

You should see the MLflow home screen with no experiments yet.

### 4. Run the project orchestrator

From the project root:

```bash
python -m src.pipeline.orchestrator
```

This creates one MLflow run under the `fire-detection` experiment with all
framework artifacts logged.

## Stopping Services

```bash
cd infra/mlflow
docker compose down
```

To also delete all stored data (Postgres + MinIO volumes):

```bash
docker compose down -v
```

## Environment Variables (already set in docker-compose)

| Variable                  | Value                              |
|---------------------------|------------------------------------|
| MLFLOW_S3_ENDPOINT_URL    | http://minio:9000 (internal DNS)   |
| AWS_ACCESS_KEY_ID         | minioadmin                         |
| AWS_SECRET_ACCESS_KEY     | minioadmin                         |

When running the Python client **on the host** (outside Docker), these must be
set in your shell or `.env`:

```bash
export MLFLOW_S3_ENDPOINT_URL=http://localhost:9000
export AWS_ACCESS_KEY_ID=minioadmin
export AWS_SECRET_ACCESS_KEY=minioadmin
```

The orchestrator sets these automatically from `config/mlflow.yaml`.
