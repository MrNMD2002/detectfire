import http from 'http';
import crypto from 'crypto';

const PORT = 8080;

// Config
const cameras = [
  {
    id: 'cam-01',
    name: 'Camera Nhà Xưởng',
    rtsp_url: 'rtsp://root:Admin!123@10.1.1.174:554/cam/realmonitor?channel=1&subtype=1',
    site_id: 'site-main',
    status: 'streaming',
    enabled: true,
    fps_sample: 3,
    conf_fire: 0.5,
    conf_smoke: 0.4
  }
];

// Mock data
let events = [];

// WebSocket clients
const wsClients = new Set();

// Parse JSON body
async function parseBody(req) {
  return new Promise((resolve) => {
    let body = '';
    req.on('data', chunk => body += chunk);
    req.on('end', () => {
      try {
        resolve(JSON.parse(body || '{}'));
      } catch {
        resolve({});
      }
    });
  });
}

// Broadcast event to all WebSocket clients
function broadcastEvent(event) {
  const message = JSON.stringify(event);
  wsClients.forEach((ws) => {
    if (ws.readyState === 1) { // OPEN
      ws.send(message);
    }
  });
}

// Handle WebSocket upgrade
function handleWebSocket(req, socket, head) {
  const path = req.url;
  
  if (path === '/ws/events') {
    // Simple WebSocket handshake
    const key = req.headers['sec-websocket-key'];
    const accept = crypto
      .createHash('sha1')
      .update(key + '258EAFA5-E914-47DA-95CA-C5AB0DC85B11')
      .digest('base64');
    
    const responseHeaders = [
      'HTTP/1.1 101 Switching Protocols',
      'Upgrade: websocket',
      'Connection: Upgrade',
      `Sec-WebSocket-Accept: ${accept}`,
      '',
      ''
    ].join('\r\n');
    
    socket.write(responseHeaders);
    
    // Add to clients
    wsClients.add(socket);
    console.log(`📡 WebSocket client connected (${wsClients.size} total)`);
    
    // Send welcome message
    socket.write(createWebSocketFrame(JSON.stringify({ type: 'connected' })));
    
    // Handle close
    socket.on('close', () => {
      wsClients.delete(socket);
      console.log(`📡 WebSocket client disconnected (${wsClients.size} total)`);
    });
    
    // Handle ping
    socket.on('data', (data) => {
      // Simple ping/pong handling
      if (data[0] === 0x89) { // Ping
        socket.write(Buffer.from([0x8A, 0x00])); // Pong
      }
    });
  } else {
    socket.destroy();
  }
}

// Simple WebSocket frame creator (for text frames)
function createWebSocketFrame(data) {
  const dataBuffer = Buffer.from(data, 'utf8');
  const length = dataBuffer.length;
  
  let frame;
  if (length < 126) {
    frame = Buffer.allocUnsafe(2 + length);
    frame[0] = 0x81; // FIN + text frame
    frame[1] = length;
    dataBuffer.copy(frame, 2);
  } else if (length < 65536) {
    frame = Buffer.allocUnsafe(4 + length);
    frame[0] = 0x81;
    frame[1] = 126;
    frame.writeUInt16BE(length, 2);
    dataBuffer.copy(frame, 4);
  } else {
    frame = Buffer.allocUnsafe(10 + length);
    frame[0] = 0x81;
    frame[1] = 127;
    frame.writeUInt32BE(0, 2);
    frame.writeUInt32BE(length, 6);
    dataBuffer.copy(frame, 10);
  }
  
  return frame;
}

// Handler
async function handleRequest(req, res) {
  // CORS
  res.setHeader('Access-Control-Allow-Origin', '*');
  res.setHeader('Access-Control-Allow-Methods', 'GET, POST, PUT, DELETE, OPTIONS');
  res.setHeader('Access-Control-Allow-Headers', 'Content-Type, Authorization');

  if (req.method === 'OPTIONS') {
    res.writeHead(204);
    res.end();
    return;
  }

  const url = new URL(req.url, `http://${req.headers.host}`);
  const path = url.pathname;

  console.log(`${req.method} ${path}`);

  // Auth
  if (path === '/api/auth/login' && req.method === 'POST') {
    const body = await parseBody(req);
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify({
      token: 'mock-jwt-token',
      user: { 
        id: '1', 
        email: body.email || 'admin@example.com', 
        name: 'Admin User',
        role: 'admin' 
      }
    }));
    return;
  }

  if (path === '/api/auth/me') {
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify({ 
      id: '1', 
      email: 'admin@example.com', 
      name: 'Admin User',
      role: 'admin' 
    }));
    return;
  }

  // Cameras
  if (path === '/api/cameras' && req.method === 'GET') {
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify(cameras));
    return;
  }

  // Events POST (from Python)
  if (path === '/api/events' && req.method === 'POST') {
    const data = await parseBody(req);
    const newEvent = {
      id: crypto.randomUUID(),
      event_type: data.event_type || 'fire',
      camera_id: data.camera_id || cameras[0].id,
      camera_name: cameras.find(c => c.id === (data.camera_id || cameras[0].id))?.name || 'Camera Test',
      site_id: data.site_id || cameras[0].site_id,
      timestamp: new Date().toISOString(),
      confidence: data.confidence || 0.85,
      detections: data.detections || [],
      snapshot_path: data.snapshot_path || null,
      metadata: data.metadata || {},
      acknowledged: false,
      acknowledged_by: null,
      acknowledged_at: null,
    };
    events.unshift(newEvent);
    if (events.length > 50) events.pop();
    console.log(`🔥 New Event: ${newEvent.event_type} (${newEvent.confidence})`);
    
    // Broadcast to WebSocket clients
    broadcastEvent(newEvent);
    
    res.writeHead(201, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify({ data: newEvent }));
    return;
  }

  // Events Stats
  if (path === '/api/events/stats') {
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify({
      total: events.length,
      fire_count: events.filter(e => e.event_type === 'fire').length,
      smoke_count: events.filter(e => e.event_type === 'smoke').length,
      acknowledged_count: events.filter(e => e.acknowledged).length,
      pending_count: events.filter(e => !e.acknowledged).length
    }));
    return;
  }

  // Events List
  if (path === '/api/events' || path.startsWith('/api/events')) {
    const limit = parseInt(url.searchParams.get('limit') || '50');
    const eventType = url.searchParams.get('event_type') || '';
    
    let filteredEvents = events;
    if (eventType) {
      filteredEvents = events.filter(e => e.event_type === eventType);
    }
    
    const result = filteredEvents.slice(0, limit);
    
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify(result));
    return;
  }

  // Event acknowledge
  if (path.startsWith('/api/events/') && path.endsWith('/acknowledge') && req.method === 'POST') {
    const eventId = path.split('/')[3];
    const event = events.find(e => e.id === eventId);
    if (event) {
      event.acknowledged = true;
      event.acknowledged_by = '1';
      event.acknowledged_at = new Date().toISOString();
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify(event));
    } else {
      res.writeHead(404, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ error: 'Event not found' }));
    }
    return;
  }

  // Health
  if (path === '/health') {
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify({ status: 'ok', version: '0.1.0' }));
    return;
  }

  res.writeHead(404, { 'Content-Type': 'application/json' });
  res.end(JSON.stringify({ error: 'not_found' }));
}

const server = http.createServer(handleRequest);

// Handle WebSocket upgrade
server.on('upgrade', handleWebSocket);

server.listen(PORT, () => {
  console.log(`🚀 Mock API Server running at http://localhost:${PORT}`);
  console.log(`📧 Login: admin@example.com / (any password)`);
  console.log(`📷 Cameras: ${cameras.length} loaded`);
  console.log(`📡 WebSocket: ws://localhost:${PORT}/ws/events`);
  console.log(`\n💡 Tip: Open another terminal and run: npm run dev`);
  
  // Generate some test events periodically
  setInterval(() => {
    if (events.length < 5) {
      const eventTypes = ['fire', 'smoke'];
      const eventType = eventTypes[Math.floor(Math.random() * eventTypes.length)];
      const testEvent = {
        id: crypto.randomUUID(),
        event_type: eventType,
        camera_id: cameras[0].id,
        camera_name: cameras[0].name,
        site_id: cameras[0].site_id,
        timestamp: new Date().toISOString(),
        confidence: 0.7 + Math.random() * 0.2,
        detections: [],
        snapshot_path: null,
        metadata: {},
        acknowledged: false,
        acknowledged_by: null,
        acknowledged_at: null,
      };
      events.unshift(testEvent);
      if (events.length > 50) events.pop();
      console.log(`🔥 Auto-generated test event: ${eventType}`);
      broadcastEvent(testEvent);
    }
  }, 10000); // Every 10 seconds
});
