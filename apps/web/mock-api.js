#!/usr/bin/env node
// Mock API Server for testing Web UI
// Run: node mock-api.js

const http = require('http');

const PORT = 8080;

// Mock data
const mockUser = {
  id: '550e8400-e29b-41d4-a716-446655440000',
  email: 'admin@example.com',
  name: 'Administrator',
  role: 'admin'
};

const mockCameras = [
  {
    id: '550e8400-e29b-41d4-a716-446655440001',
    site_id: 'site-a',
    name: 'Camera Nhà kho A',
    description: 'Góc 1 nhà kho',
    enabled: true,
    fps_sample: 3,
    imgsz: 640,
    conf_fire: 0.5,
    conf_smoke: 0.4,
    status: 'streaming',
    created_at: new Date().toISOString()
  },
  {
    id: '550e8400-e29b-41d4-a716-446655440002',
    site_id: 'site-a',
    name: 'Camera Nhà kho B',
    description: 'Góc 2 nhà kho',
    enabled: true,
    fps_sample: 3,
    imgsz: 640,
    conf_fire: 0.5,
    conf_smoke: 0.4,
    status: 'connected',
    created_at: new Date().toISOString()
  }
];

const mockEvents = [
  {
    id: '550e8400-e29b-41d4-a716-446655440010',
    event_type: 'fire',
    camera_id: '550e8400-e29b-41d4-a716-446655440001',
    site_id: 'site-a',
    timestamp: new Date().toISOString(),
    confidence: 0.87,
    detections: [{ class: 'fire', confidence: 0.92, bbox: { x: 100, y: 200, width: 150, height: 180 } }],
    acknowledged: false,
    created_at: new Date().toISOString()
  },
  {
    id: '550e8400-e29b-41d4-a716-446655440011',
    event_type: 'smoke',
    camera_id: '550e8400-e29b-41d4-a716-446655440002',
    site_id: 'site-a',
    timestamp: new Date(Date.now() - 3600000).toISOString(),
    confidence: 0.75,
    detections: [{ class: 'smoke', confidence: 0.75, bbox: { x: 200, y: 100, width: 250, height: 200 } }],
    acknowledged: true,
    created_at: new Date(Date.now() - 3600000).toISOString()
  }
];

const mockStats = {
  total: 150,
  fire_count: 45,
  smoke_count: 105,
  acknowledged_count: 120,
  pending_count: 30
};

// Simple router
function handleRequest(req, res) {
  // CORS headers
  res.setHeader('Access-Control-Allow-Origin', '*');
  res.setHeader('Access-Control-Allow-Methods', 'GET, POST, PUT, DELETE, OPTIONS');
  res.setHeader('Access-Control-Allow-Headers', 'Content-Type, Authorization');
  res.setHeader('Content-Type', 'application/json');

  if (req.method === 'OPTIONS') {
    res.writeHead(200);
    res.end();
    return;
  }

  const url = req.url;
  console.log(`${req.method} ${url}`);

  // Routes
  if (url === '/api/auth/login' && req.method === 'POST') {
    let body = '';
    req.on('data', chunk => body += chunk);
    req.on('end', () => {
      const { email, password } = JSON.parse(body || '{}');
      if (email === 'admin@example.com' && password === 'admin123') {
        res.writeHead(200);
        res.end(JSON.stringify({
          token: 'mock-jwt-token-12345',
          token_type: 'Bearer',
          expires_in: 86400,
          user: mockUser
        }));
      } else {
        res.writeHead(401);
        res.end(JSON.stringify({ error: 'auth_error', message: 'Invalid credentials' }));
      }
    });
    return;
  }

  if (url === '/api/auth/me' && req.method === 'GET') {
    res.writeHead(200);
    res.end(JSON.stringify(mockUser));
    return;
  }

  if (url === '/api/cameras' && req.method === 'GET') {
    res.writeHead(200);
    res.end(JSON.stringify(mockCameras));
    return;
  }

  if (url.startsWith('/api/events') && req.method === 'GET') {
    if (url.includes('/stats')) {
      res.writeHead(200);
      res.end(JSON.stringify(mockStats));
    } else {
      res.writeHead(200);
      res.end(JSON.stringify(mockEvents));
    }
    return;
  }

  if (url === '/health' && req.method === 'GET') {
    res.writeHead(200);
    res.end(JSON.stringify({
      status: 'ok',
      version: '0.1.0',
      uptime_seconds: Math.floor(process.uptime())
    }));
    return;
  }

  // 404
  res.writeHead(404);
  res.end(JSON.stringify({ error: 'not_found', message: 'Endpoint not found' }));
}

const server = http.createServer(handleRequest);

server.listen(PORT, () => {
  console.log(`🚀 Mock API Server running at http://localhost:${PORT}`);
  console.log(`📧 Login: admin@example.com / admin123`);
});
