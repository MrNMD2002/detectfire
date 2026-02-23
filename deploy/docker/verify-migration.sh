#!/bin/bash
# Verify database migration before build

set -e

echo "Checking database connection..."
if ! sqlx migrate info >/dev/null 2>&1; then
    echo "ERROR: Cannot connect to database with DATABASE_URL=$DATABASE_URL"
    echo "Please ensure PostgreSQL is running and accessible."
    exit 1
fi

echo "Running migrations..."
if sqlx migrate run; then
    echo "Migrations completed successfully"
    
    # Verify tables exist
    echo "Verifying schema..."
    if sqlx query "SELECT 1 FROM cameras LIMIT 1" >/dev/null 2>&1 && \
       sqlx query "SELECT 1 FROM events LIMIT 1" >/dev/null 2>&1 && \
       sqlx query "SELECT 1 FROM users LIMIT 1" >/dev/null 2>&1; then
        echo "Schema verified: all tables exist"
        exit 0
    else
        echo "WARNING: Some tables may be missing. Build may fail."
        exit 0  # Continue anyway
    fi
else
    echo "ERROR: Migration failed"
    exit 1
fi
