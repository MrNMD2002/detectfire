#!/bin/bash
# Initialize database with migrations
# This script runs migrations before API build

set -e

echo "Waiting for PostgreSQL to be ready..."
until PGPASSWORD="${POSTGRES_PASSWORD}" psql -h "${POSTGRES_HOST:-postgres}" -U "${POSTGRES_USER:-fire_detect}" -d "${POSTGRES_DB:-fire_detect}" -c '\q' 2>/dev/null; do
  echo "PostgreSQL is unavailable - sleeping"
  sleep 1
done

echo "PostgreSQL is ready - running migrations..."

# Run migrations
cd /app
sqlx migrate run

echo "Migrations completed successfully!"
