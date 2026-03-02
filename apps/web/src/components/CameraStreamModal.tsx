import { useEffect, useRef, useState } from 'react';
import { X, Video, AlertTriangle } from 'lucide-react';
import { useWebSocket } from '@/hooks/useWebSocket';
import { useAuthStore } from '@/stores/authStore';

interface Detection {
  class?: string;
  class_name?: string;
  confidence: number;
  bbox: { x: number; y: number; width: number; height: number };
}

interface StreamEvent {
  camera_id: string;
  event_type: string;
  detections: Detection[];
  metadata?: { frame_width?: number; frame_height?: number };
}

interface CameraStreamModalProps {
  camera: { id: string; name: string; detector_camera_id?: string; site_id?: string; status?: string; fps_sample?: number; conf_fire?: number; conf_smoke?: number };
  onClose: () => void;
  latestEvent?: { snapshot_path?: string; timestamp?: string } | null;
}

export function CameraStreamModal({ camera, onClose, latestEvent }: CameraStreamModalProps) {
  const imgRef = useRef<HTMLImageElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const [streamError, setStreamError] = useState<string | null>(null);
  const [streamLoading, setStreamLoading] = useState(true);
  const [lastDetections, setLastDetections] = useState<Detection[]>([]);
  const token = useAuthStore((s) => s.token);

  // WebSocket: receive detection events for this camera
  useWebSocket({
    onMessage: (msg: StreamEvent) => {
      if (msg.camera_id === camera.id && (msg.event_type === 'fire' || msg.event_type === 'smoke')) {
        setLastDetections(msg.detections || []);
      }
    },
  });

  // Draw detection overlay on canvas — positioned to match the actual rendered image area
  useEffect(() => {
    const img = imgRef.current;
    const canvas = canvasRef.current;
    if (!img || !canvas || lastDetections.length === 0) return;

    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    const draw = () => {
      // Use the image's natural rendered rect (width:100%, height:auto)
      const imgRect = img.getBoundingClientRect();
      const renderW = imgRect.width;
      const renderH = imgRect.height;

      canvas.width = renderW;
      canvas.height = renderH;

      // Position canvas exactly over the image element
      canvas.style.width = `${renderW}px`;
      canvas.style.height = `${renderH}px`;

      ctx.clearRect(0, 0, renderW, renderH);

      const natW = img.naturalWidth || 640;
      const natH = img.naturalHeight || 640;
      const scaleX = renderW / natW;
      const scaleY = renderH / natH;

      for (const det of lastDetections) {
        const bbox = det.bbox;
        const isNormalized = bbox.x <= 1 && bbox.y <= 1 && bbox.width <= 1 && bbox.height <= 1;
        const x = isNormalized ? bbox.x * renderW : bbox.x * scaleX;
        const y = isNormalized ? bbox.y * renderH : bbox.y * scaleY;
        const w = isNormalized ? bbox.width * renderW : bbox.width * scaleX;
        const h = isNormalized ? bbox.height * renderH : bbox.height * scaleY;

        const cls = det.class || det.class_name || 'detection';
        ctx.strokeStyle = cls === 'fire' ? '#ff4444' : '#44aaff';
        ctx.lineWidth = 3;
        ctx.strokeRect(x, y, w, h);
        ctx.fillStyle = cls === 'fire' ? 'rgba(255,68,68,0.2)' : 'rgba(68,170,255,0.2)';
        ctx.fillRect(x, y, w, h);
        ctx.fillStyle = '#fff';
        ctx.font = 'bold 12px sans-serif';
        ctx.fillText(`${cls} ${(det.confidence * 100).toFixed(0)}%`, x, y > 16 ? y - 4 : y + 14);
      }
    };

    draw();
    const interval = setInterval(draw, 200);
    return () => clearInterval(interval);
  }, [lastDetections]);

  // MJPEG stream via fetch (MBFS-Stream approach)
  // Uses fetch + Authorization header instead of <img src> so JWT auth works.
  // Parses JPEG frames from the multipart/x-mixed-replace stream by scanning for
  // SOI (FF D8) and EOI (FF D9) JPEG markers, then creates blob URLs for display.
  useEffect(() => {
    if (!camera.id) return;

    const controller = new AbortController();
    let currentBlobUrl: string | null = null;

    const startStream = async () => {
      try {
        const headers: Record<string, string> = {};
        if (token) headers['Authorization'] = `Bearer ${token}`;

        const response = await fetch(`/api/cameras/${camera.id}/stream/mjpeg`, {
          headers,
          signal: controller.signal,
        });

        if (!response.ok || !response.body) {
          setStreamLoading(false);
          setStreamError(
            response.status === 404
              ? 'Camera chưa được detector nhận dạng — kiểm tra detector_camera_id'
              : 'Không kết nối được stream'
          );
          return;
        }

        setStreamLoading(false);
        setStreamError(null);

        const reader = response.body.getReader();
        let buffer = new Uint8Array(0);

        // JPEG markers
        const SOI_0 = 0xff;
        const SOI_1 = 0xd8;
        const EOI_0 = 0xff;
        const EOI_1 = 0xd9;

        while (true) {
          const { value, done } = await reader.read();
          if (done) break;

          // Append chunk to buffer
          const newBuf = new Uint8Array(buffer.length + value.length);
          newBuf.set(buffer);
          newBuf.set(value, buffer.length);
          buffer = newBuf;

          // Find JPEG SOI and EOI within the buffer
          let soiIdx = -1;
          let eoiIdx = -1;

          for (let i = 0; i < buffer.length - 1; i++) {
            if (soiIdx === -1 && buffer[i] === SOI_0 && buffer[i + 1] === SOI_1) {
              soiIdx = i;
            }
            if (soiIdx !== -1 && buffer[i] === EOI_0 && buffer[i + 1] === EOI_1) {
              eoiIdx = i + 2;
              break;
            }
          }

          if (soiIdx >= 0 && eoiIdx > soiIdx) {
            const jpegSlice = buffer.slice(soiIdx, eoiIdx);
            buffer = buffer.slice(eoiIdx); // keep remaining data

            const blob = new Blob([jpegSlice], { type: 'image/jpeg' });
            const url = URL.createObjectURL(blob);

            if (imgRef.current) {
              imgRef.current.src = url;
            }

            // Revoke previous blob URL to free memory
            if (currentBlobUrl) URL.revokeObjectURL(currentBlobUrl);
            currentBlobUrl = url;
          }

          // Prevent unbounded buffer growth (> 2MB = something is wrong)
          if (buffer.length > 2 * 1024 * 1024) {
            buffer = new Uint8Array(0);
          }
        }

        // Stream ended normally
        setStreamError('Stream đã kết thúc');
      } catch (err: any) {
        if (err.name === 'AbortError') return; // Normal cleanup
        setStreamLoading(false);
        setStreamError('Lỗi kết nối stream');
      }
    };

    startStream();

    return () => {
      controller.abort();
      if (currentBlobUrl) URL.revokeObjectURL(currentBlobUrl);
    };
  }, [camera.id, token]);

  const showSnapshot = streamError && latestEvent?.snapshot_path;
  const showPlaceholder = streamError && !latestEvent?.snapshot_path;

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal" style={{ maxWidth: 900 }} onClick={(e) => e.stopPropagation()}>
        <div className="modal-header">
          <h2>{camera.name}</h2>
          <button className="btn btn-icon" onClick={onClose}>
            <X size={20} />
          </button>
        </div>
        <div className="modal-body">
          {/* Stream container — no black background; image fills width at natural aspect ratio */}
          <div
            ref={containerRef}
            style={{
              borderRadius: 8,
              position: 'relative',
              overflow: 'hidden',
              background: 'var(--bg-secondary, #1a1f2e)',
              // Reserve height only while loading so layout doesn't collapse
              minHeight: streamLoading && !streamError ? 400 : undefined,
            }}
          >
            {/* MJPEG live frame via blob URL — updated by fetch loop above */}
            {!streamError && (
              <>
                <img
                  ref={imgRef}
                  alt="Live stream"
                  style={{
                    // width:100% + height:auto = natural aspect ratio, no black bars
                    width: '100%',
                    height: 'auto',
                    display: streamLoading ? 'none' : 'block',
                    borderRadius: 8,
                  }}
                />
                {/* Canvas overlay sits exactly on top of the img element */}
                <canvas
                  ref={canvasRef}
                  style={{
                    position: 'absolute',
                    top: 0,
                    left: 0,
                    pointerEvents: 'none',
                  }}
                />
              </>
            )}

            {/* Snapshot fallback when stream fails but we have a recent snapshot */}
            {showSnapshot && latestEvent?.snapshot_path && (
              <img
                src={`/api/snapshots/${latestEvent.snapshot_path}`}
                alt="Snapshot"
                style={{ width: '100%', height: 'auto', display: 'block', borderRadius: 8 }}
              />
            )}

            {/* Loading indicator */}
            {streamLoading && !streamError && (
              <div
                style={{
                  position: 'absolute',
                  inset: 0,
                  display: 'flex',
                  alignItems: 'center',
                  justifyContent: 'center',
                  flexDirection: 'column',
                  color: 'var(--text-muted)',
                }}
              >
                <Video size={48} style={{ marginBottom: 12 }} />
                <span>Đang kết nối stream...</span>
              </div>
            )}

            {/* Error state */}
            {showPlaceholder && (
              <div
                style={{
                  padding: 32,
                  display: 'flex',
                  flexDirection: 'column',
                  alignItems: 'center',
                  justifyContent: 'center',
                  color: 'var(--text-muted)',
                  minHeight: 400,
                }}
              >
                <AlertTriangle size={48} style={{ color: 'var(--color-warning)', marginBottom: 16 }} />
                <p style={{ marginBottom: 8 }}>{streamError}</p>
                <p style={{ fontSize: 12 }}>
                  Đảm bảo Detector service đang chạy và camera có detector_camera_id khớp với config
                </p>
              </div>
            )}
          </div>

          {camera.site_id && (
            <div style={{ marginTop: 16 }}>
              <h4 style={{ marginBottom: 8 }}>Thông tin Camera</h4>
              <table style={{ width: '100%', fontSize: 14 }}>
                <tbody>
                  <tr>
                    <td style={{ padding: '4px 0', color: 'var(--text-muted)' }}>Site:</td>
                    <td>{camera.site_id}</td>
                  </tr>
                  {camera.status && (
                    <tr>
                      <td style={{ padding: '4px 0', color: 'var(--text-muted)' }}>Status:</td>
                      <td>{camera.status}</td>
                    </tr>
                  )}
                  {camera.fps_sample != null && (
                    <tr>
                      <td style={{ padding: '4px 0', color: 'var(--text-muted)' }}>FPS Sample:</td>
                      <td>{camera.fps_sample}</td>
                    </tr>
                  )}
                  {camera.conf_fire != null && (
                    <tr>
                      <td style={{ padding: '4px 0', color: 'var(--text-muted)' }}>Fire Threshold:</td>
                      <td>{camera.conf_fire}</td>
                    </tr>
                  )}
                  {camera.conf_smoke != null && (
                    <tr>
                      <td style={{ padding: '4px 0', color: 'var(--text-muted)' }}>Smoke Threshold:</td>
                      <td>{camera.conf_smoke}</td>
                    </tr>
                  )}
                </tbody>
              </table>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
