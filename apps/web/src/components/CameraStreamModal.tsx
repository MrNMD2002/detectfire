import { useEffect, useRef, useState } from 'react';
import Hls from 'hls.js';
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
  const videoRef = useRef<HTMLVideoElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const hlsRef = useRef<Hls | null>(null);
  const [streamError, setStreamError] = useState<string | null>(null);
  const [streamLoading, setStreamLoading] = useState(true);
  const [lastDetections, setLastDetections] = useState<Detection[]>([]);
  const token = useAuthStore((s) => s.token);

  // WebSocket: nhận detection events cho camera này
  useWebSocket({
    onMessage: (msg: StreamEvent) => {
      if (msg.camera_id === camera.id && (msg.event_type === 'fire' || msg.event_type === 'smoke')) {
        setLastDetections(msg.detections || []);
      }
    },
  });

  // Vẽ overlay detection lên canvas
  useEffect(() => {
    const video = videoRef.current;
    const canvas = canvasRef.current;
    const container = containerRef.current;
    if (!video || !canvas || !container || lastDetections.length === 0) return;

    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    const draw = () => {
      const rect = video.getBoundingClientRect();
      const scaleX = rect.width / (video.videoWidth || 640);
      const scaleY = rect.height / (video.videoHeight || 360);

      canvas.width = rect.width;
      canvas.height = rect.height;
      ctx.clearRect(0, 0, canvas.width, canvas.height);

      for (const det of lastDetections) {
        const bbox = det.bbox;
        // bbox có thể là normalized (0-1) hoặc pixel
        const isNormalized = bbox.x <= 1 && bbox.y <= 1 && bbox.width <= 1 && bbox.height <= 1;
        const x = isNormalized ? bbox.x * rect.width : bbox.x * scaleX;
        const y = isNormalized ? bbox.y * rect.height : bbox.y * scaleY;
        const w = isNormalized ? bbox.width * rect.width : bbox.width * scaleX;
        const h = isNormalized ? bbox.height * rect.height : bbox.height * scaleY;

        const cls = det.class || det.class_name || 'detection';
        ctx.strokeStyle = cls === 'fire' ? '#ff4444' : '#44aaff';
        ctx.lineWidth = 3;
        ctx.strokeRect(x, y, w, h);
        ctx.fillStyle = cls === 'fire' ? 'rgba(255,68,68,0.2)' : 'rgba(68,170,255,0.2)';
        ctx.fillRect(x, y, w, h);
        ctx.fillStyle = '#fff';
        ctx.font = '12px sans-serif';
        ctx.fillText(`${cls} ${(det.confidence * 100).toFixed(0)}%`, x, y - 4);
      }
    };

    draw();
    const interval = setInterval(draw, 200);
    return () => clearInterval(interval);
  }, [lastDetections]);

  // HLS stream - chỉ load khi modal mở
  useEffect(() => {
    const video = videoRef.current;
    if (!video || !camera.id) return;

    const playlistUrl = `/api/cameras/${camera.id}/stream/playlist.m3u8`;

    if (Hls.isSupported()) {
      const hls = new Hls({
        xhrSetup: (xhr) => {
          if (token) {
            xhr.setRequestHeader('Authorization', `Bearer ${token}`);
          }
        },
      });

      hls.loadSource(playlistUrl);
      hls.attachMedia(video);

      hls.on(Hls.Events.MANIFEST_PARSED, () => {
        setStreamLoading(false);
        setStreamError(null);
      });

      hls.on(Hls.Events.ERROR, (_, data) => {
        if (data.fatal) {
          setStreamLoading(false);
          setStreamError(data.type === Hls.ErrorTypes.NETWORK_ERROR ? 'Không kết nối được stream' : 'Lỗi phát stream');
          hls.destroy();
        }
      });

      hlsRef.current = hls;
      return () => {
        hls.destroy();
        hlsRef.current = null;
      };
    } else if (video.canPlayType('application/vnd.apple.mpegurl')) {
      video.src = playlistUrl;
      setStreamLoading(false);
      return () => {
        video.src = '';
      };
    } else {
      setStreamLoading(false);
      setStreamError('Trình duyệt không hỗ trợ HLS');
    }
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
          <div
            ref={containerRef}
            style={{
              background: '#000',
              borderRadius: 8,
              minHeight: 400,
              position: 'relative',
              overflow: 'hidden',
            }}
          >
            {/* HLS Video */}
            {!streamError && (
              <>
                <video
                  ref={videoRef}
                  autoPlay
                  muted
                  playsInline
                  style={{
                    width: '100%',
                    maxHeight: 500,
                    display: streamLoading ? 'none' : 'block',
                  }}
                />
                {/* Canvas overlay cho detection */}
                <canvas
                  ref={canvasRef}
                  style={{
                    position: 'absolute',
                    top: 0,
                    left: 0,
                    width: '100%',
                    height: '100%',
                    pointerEvents: 'none',
                  }}
                />
              </>
            )}

            {/* Snapshot fallback */}
            {showSnapshot && latestEvent?.snapshot_path && (
              <img
                src={`/api/snapshots/${latestEvent.snapshot_path}`}
                alt="Snapshot"
                style={{ width: '100%', maxHeight: 500, objectFit: 'contain' }}
              />
            )}

            {/* Loading */}
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

            {/* Error placeholder */}
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
