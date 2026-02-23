# Fire & Smoke Detection Web UI

## Cài đặt và Chạy

### 1. Cài đặt dependencies
```bash
npm install
```

### 2. Chạy Mock API Server (Terminal 1)
```bash
npm run mock-api
```

Mock API sẽ chạy tại `http://localhost:8080` với:
- WebSocket endpoint: `ws://localhost:8080/ws/events`
- API endpoints: `/api/*`
- Tự động tạo test events mỗi 10 giây

### 3. Chạy Web UI (Terminal 2)
```bash
npm run dev
```

Web UI sẽ chạy tại `http://localhost:3000`

## Đăng nhập

- **Email**: `admin@example.com` (hoặc bất kỳ email nào)
- **Password**: Bất kỳ password nào (mock API không kiểm tra)

## Tính năng

- ✅ Dashboard với stats và events real-time
- ✅ WebSocket connection cho real-time events
- ✅ Events page với filter và acknowledge
- ✅ Cameras management
- ✅ Settings page

## Test Events

Mock API tự động tạo test events mỗi 10 giây. Bạn cũng có thể gửi events thủ công:

```bash
curl -X POST http://localhost:8080/api/events \
  -H "Content-Type: application/json" \
  -d '{
    "event_type": "fire",
    "camera_id": "cam-01",
    "site_id": "site-main",
    "confidence": 0.85
  }'
```

## Cấu hình

- **Vite dev server**: Port 3000
- **Mock API**: Port 8080
- **WebSocket**: Tự động proxy qua Vite

## Kết nối với Real API

Để kết nối với real API service, chỉ cần:
1. Đảm bảo API service chạy tại `http://localhost:8080`
2. Không cần chạy mock-api
3. Web UI sẽ tự động kết nối với real API
