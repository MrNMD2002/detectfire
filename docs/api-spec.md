# Fire & Smoke Detection API Specification

## Base URL

```
Production: https://fire-detect.example.com/api
Development: http://localhost:8080/api
```

## Authentication

JWT Bearer token required for most endpoints.

```
Authorization: Bearer <token>
```

### Login

```http
POST /auth/login
Content-Type: application/json

{
  "email": "admin@example.com",
  "password": "your-password"
}
```

**Response:**

```json
{
  "token": "eyJhbGci...",
  "token_type": "Bearer",
  "expires_in": 86400,
  "user": {
    "id": "uuid",
    "email": "admin@example.com",
    "name": "Administrator",
    "role": "admin"
  }
}
```

### Get Current User

```http
GET /auth/me
Authorization: Bearer <token>
```

---

## Cameras

### List Cameras

```http
GET /cameras
Authorization: Bearer <token>
```

**Response:**

```json
[
  {
    "id": "uuid",
    "site_id": "site-a",
    "name": "Camera Nhà kho A",
    "description": "Góc 1 nhà kho",
    "enabled": true,
    "fps_sample": 3,
    "imgsz": 640,
    "conf_fire": 0.5,
    "conf_smoke": 0.4,
    "window_size": 10,
    "fire_hits": 3,
    "smoke_hits": 3,
    "cooldown_sec": 60,
    "created_at": "2024-01-01T00:00:00Z",
    "updated_at": "2024-01-01T00:00:00Z"
  }
]
```

### Get Camera

```http
GET /cameras/:id
Authorization: Bearer <token>
```

### Create Camera

```http
POST /cameras
Authorization: Bearer <token>
Content-Type: application/json

{
  "site_id": "site-a",
  "name": "Camera Nhà kho A",
  "description": "Góc 1 nhà kho",
  "rtsp_url": "rtsp://user:pass@192.168.1.100:554/stream",
  "enabled": true,
  "fps_sample": 3,
  "imgsz": 640,
  "conf_fire": 0.5,
  "conf_smoke": 0.4,
  "window_size": 10,
  "fire_hits": 3,
  "smoke_hits": 3,
  "cooldown_sec": 60
}
```

### Update Camera

```http
PUT /cameras/:id
Authorization: Bearer <token>
Content-Type: application/json

{
  "name": "Camera Nhà kho A - Updated",
  "enabled": false
}
```

### Delete Camera

```http
DELETE /cameras/:id
Authorization: Bearer <token>
```

### Get Camera Status

```http
GET /cameras/:id/status
Authorization: Bearer <token>
```

**Response:**

```json
{
  "camera_id": "uuid",
  "status": "streaming",
  "reconnect_count": 0,
  "fps_in": 25.0,
  "fps_infer": 3.0,
  "last_frame_timestamp": 1706886000,
  "error_message": null
}
```

---

## Events

### List Events

```http
GET /events?camera_id=uuid&event_type=fire&limit=50&offset=0
Authorization: Bearer <token>
```

**Query Parameters:**

- `camera_id` (optional): Filter by camera
- `site_id` (optional): Filter by site
- `event_type` (optional): "fire" or "smoke"
- `start_time` (optional): ISO 8601 datetime
- `end_time` (optional): ISO 8601 datetime
- `acknowledged` (optional): true/false
- `limit` (optional): Default 50
- `offset` (optional): Default 0

**Response:**

```json
[
  {
    "id": "uuid",
    "event_type": "fire",
    "camera_id": "uuid",
    "site_id": "site-a",
    "timestamp": "2024-02-02T15:30:00Z",
    "confidence": 0.87,
    "detections": [
      {
        "class": "fire",
        "confidence": 0.92,
        "bbox": { "x": 100, "y": 200, "width": 150, "height": 180 }
      }
    ],
    "snapshot_path": "/snapshots/2024-02-02/cam-01-153000.jpg",
    "metadata": {
      "fps_in": 25,
      "fps_infer": 3,
      "inference_ms": 45
    },
    "acknowledged": false,
    "acknowledged_by": null,
    "acknowledged_at": null
  }
]
```

### Acknowledge Event

```http
POST /events/:id/acknowledge
Authorization: Bearer <token>
```

**Response:**

```json
{
  "acknowledged": true,
  "acknowledged_by": "uuid",
  "acknowledged_at": "2024-02-02T15:35:00Z"
}
```

### Event Statistics

```http
GET /events/stats?start_time=2024-02-01&end_time=2024-02-02
Authorization: Bearer <token>
```

**Response:**

```json
{
  "total": 150,
  "fire_count": 45,
  "smoke_count": 105,
  "acknowledged_count": 120,
  "pending_count": 30
}
```

---

## WebSocket

### Real-time Events

```
WS /ws/events
```

Connect with JWT in query string or header:

```javascript
const ws = new WebSocket("ws://localhost:8080/ws/events?token=<jwt>");
```

**Message Format:**

```json
{
  "event_type": "fire",
  "camera_id": "uuid",
  "site_id": "site-a",
  "timestamp": "2024-02-02T15:30:00Z",
  "confidence": 0.87,
  "detections": [...],
  "snapshot": "base64-encoded-jpeg"
}
```

---

## Health Check

```http
GET /health
```

**Response:**

```json
{
  "status": "ok",
  "version": "0.1.0",
  "uptime_seconds": 3600,
  "database": "connected",
  "detector": "connected"
}
```

---

## Error Responses

All errors follow this format:

```json
{
  "error": "error_code",
  "message": "Human readable message",
  "details": null
}
```

**Error Codes:**

- `auth_error`: Authentication failed
- `forbidden`: Insufficient permissions
- `not_found`: Resource not found
- `validation_error`: Invalid input
- `database_error`: Database operation failed
- `internal_error`: Internal server error
