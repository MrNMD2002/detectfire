import { useQuery, useQueries } from '@tanstack/react-query';
import { type ReactNode } from 'react';
import {
  Flame,
  Wind,
  Camera,
  CheckCircle,
  TrendingUp,
  TrendingDown,
  Activity,
  Wifi,
  WifiOff,
  AlertTriangle,
} from 'lucide-react';
import { eventsApi, camerasApi } from '@/lib/api';
import { useWebSocket } from '@/hooks/useWebSocket';
import { useState } from 'react';

interface CameraStatusData {
  status: string;
  fps_in?: number;
  fps_infer?: number;
  reconnect_count?: number;
  error_message?: string;
}

const STATUS_CONFIG: Record<string, { label: string; color: string; icon: ReactNode }> = {
  streaming:            { label: 'Streaming',    color: 'var(--color-success)',  icon: <Wifi size={13} /> },
  connected:            { label: 'Kết nối',      color: 'var(--color-success)',  icon: <Wifi size={13} /> },
  connecting:           { label: 'Đang kết nối', color: 'var(--color-warning)',  icon: <Activity size={13} /> },
  reconnecting:         { label: 'Reconnecting', color: '#f97316',               icon: <Activity size={13} /> },
  failed:               { label: 'Lỗi',          color: 'var(--color-danger)',   icon: <WifiOff size={13} /> },
  disabled:             { label: 'Tắt',          color: 'var(--text-muted)',     icon: <WifiOff size={13} /> },
  not_found:            { label: 'Không tìm thấy', color: 'var(--text-muted)',   icon: <AlertTriangle size={13} /> },
  detector_unavailable: { label: 'Detector off', color: 'var(--text-muted)',     icon: <AlertTriangle size={13} /> },
  error:                { label: 'Lỗi',          color: 'var(--color-danger)',   icon: <AlertTriangle size={13} /> },
  unknown:              { label: 'Không rõ',     color: 'var(--text-muted)',     icon: <Activity size={13} /> },
};

export default function DashboardPage() {
  const [realtimeEvents, setRealtimeEvents] = useState<any[]>([]);

  // Fetch stats — filter "today" (from midnight local time)
  const todayStart = new Date();
  todayStart.setHours(0, 0, 0, 0);

  const { data: stats } = useQuery({
    queryKey: ['eventStats', todayStart.toISOString()],
    queryFn: () => eventsApi.stats({ start_time: todayStart.toISOString() }),
    refetchInterval: 30000,
  });

  // Fetch cameras
  const { data: cameras } = useQuery({
    queryKey: ['cameras'],
    queryFn: () => camerasApi.list(),
  });

  // Fetch recent events
  const { data: recentEventsPage } = useQuery({
    queryKey: ['recentEvents'],
    queryFn: () => eventsApi.list({ limit: 5 }),
    refetchInterval: 10000,
  });
  const recentEvents = recentEventsPage?.data;

  // Camera status polling — one query per camera, refetch every 15s
  const cameraStatusQueries = useQueries({
    queries: (cameras || []).map((camera: any) => ({
      queryKey: ['cameraStatus', camera.id],
      queryFn: (): Promise<CameraStatusData> => camerasApi.getStatus(camera.id),
      refetchInterval: 15000,
      retry: false,
    })),
  });

  // WebSocket for real-time events
  const { isConnected } = useWebSocket({
    onMessage: (message) => {
      setRealtimeEvents((prev) => [message, ...prev].slice(0, 5));
    },
  });

  const statsData = stats || {
    fire_count: 0,
    smoke_count: 0,
    total: 0,
    acknowledged_count: 0,
  };

  const camerasData = cameras || [];
  const activeCount = camerasData.filter((c: any) => c.enabled).length;

  return (
    <>
      <header className="page-header" style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start' }}>
        <div>
          <h1 className="page-title">Dashboard</h1>
          <p className="page-subtitle">Tổng quan hệ thống phát hiện cháy và khói</p>
        </div>
        <div style={{
          display: 'flex', alignItems: 'center', gap: 6, fontSize: 13,
          color: isConnected ? 'var(--color-success)' : 'var(--text-muted)',
          paddingTop: 4,
        }}>
          <span style={{
            width: 8, height: 8, borderRadius: '50%',
            backgroundColor: isConnected ? 'var(--color-success)' : 'var(--color-warning)',
            display: 'inline-block',
            boxShadow: isConnected ? '0 0 0 2px color-mix(in srgb, var(--color-success) 25%, transparent)' : 'none',
          }} />
          {isConnected ? 'Live' : 'Đang kết nối...'}
        </div>
      </header>

      <div className="page-content">
        {/* Stats Grid */}
        <div className="stats-grid">
          <div className="stat-card">
            <div className="stat-icon fire">
              <Flame size={24} />
            </div>
            <div className="stat-content">
              <div className="stat-label">Cảnh báo Cháy</div>
              <div className="stat-value">{statsData.fire_count}</div>
              <div className="stat-trend up">
                <TrendingUp size={14} />
                <span>Hôm nay</span>
              </div>
            </div>
          </div>

          <div className="stat-card">
            <div className="stat-icon smoke">
              <Wind size={24} />
            </div>
            <div className="stat-content">
              <div className="stat-label">Cảnh báo Khói</div>
              <div className="stat-value">{statsData.smoke_count}</div>
              <div className="stat-trend up">
                <TrendingUp size={14} />
                <span>Hôm nay</span>
              </div>
            </div>
          </div>

          <div className="stat-card">
            <div className="stat-icon cameras">
              <Camera size={24} />
            </div>
            <div className="stat-content">
              <div className="stat-label">Camera Hoạt động</div>
              <div className="stat-value">
                {activeCount} / {camerasData.length}
              </div>
              <div className="stat-trend down">
                <Activity size={14} />
                <span>Đang stream</span>
              </div>
            </div>
          </div>

          <div className="stat-card">
            <div className="stat-icon success">
              <CheckCircle size={24} />
            </div>
            <div className="stat-content">
              <div className="stat-label">Đã xác nhận</div>
              <div className="stat-value">{statsData.acknowledged_count}</div>
              <div className="stat-trend down">
                <TrendingDown size={14} />
                <span>
                  {statsData.total - statsData.acknowledged_count} chờ xử lý
                </span>
              </div>
            </div>
          </div>
        </div>

        {/* Camera Status */}
        {camerasData.length > 0 && (
          <div className="card" style={{ marginTop: 'var(--spacing-6)' }}>
            <div className="card-header">
              <h2 className="card-title">Trạng thái Camera</h2>
              <span style={{ fontSize: 12, color: 'var(--text-muted)' }}>Cập nhật mỗi 15s</span>
            </div>
            <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(220px, 1fr))', gap: 12, padding: '0 0 4px' }}>
              {camerasData.map((camera: any, idx: number) => {
                const query = cameraStatusQueries[idx];
                const st = query?.data as CameraStatusData | undefined;
                const statusKey = st?.status || 'unknown';
                const cfg = STATUS_CONFIG[statusKey] || STATUS_CONFIG['unknown'];
                const isLoading = query?.isLoading;

                return (
                  <div key={camera.id} style={{
                    border: '1px solid var(--border-color)',
                    borderRadius: 8,
                    padding: '12px 14px',
                    background: 'var(--bg-secondary)',
                  }}>
                    <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start', marginBottom: 8 }}>
                      <div>
                        <div style={{ fontWeight: 600, fontSize: 14 }}>{camera.name}</div>
                        <div style={{ fontSize: 11, color: 'var(--text-muted)', marginTop: 2 }}>
                          {camera.detector_camera_id || camera.id.slice(0, 8)}
                        </div>
                      </div>
                      {isLoading ? (
                        <span style={{ fontSize: 12, color: 'var(--text-muted)' }}>...</span>
                      ) : (
                        <span style={{
                          display: 'flex', alignItems: 'center', gap: 4,
                          fontSize: 12, color: cfg.color,
                          background: `color-mix(in srgb, ${cfg.color} 12%, transparent)`,
                          padding: '3px 8px', borderRadius: 12,
                        }}>
                          {cfg.icon}{cfg.label}
                        </span>
                      )}
                    </div>
                    {st && (
                      <div style={{ display: 'flex', gap: 12, fontSize: 12, color: 'var(--text-muted)' }}>
                        {st.fps_in != null && (
                          <span title="FPS vào">▶ {Number(st.fps_in).toFixed(1)} fps</span>
                        )}
                        {st.fps_infer != null && (
                          <span title="FPS inference">⚡ {Number(st.fps_infer).toFixed(1)} infer</span>
                        )}
                        {(st.reconnect_count ?? 0) > 0 && (
                          <span title="Số lần reconnect" style={{ color: '#f97316' }}>
                            ↺ {st.reconnect_count}
                          </span>
                        )}
                        {st.error_message && (
                          <span title={st.error_message} style={{ color: 'var(--color-danger)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', maxWidth: 120 }}>
                            {st.error_message}
                          </span>
                        )}
                      </div>
                    )}
                  </div>
                );
              })}
            </div>
          </div>
        )}

        {/* Recent Events */}
        <div className="card" style={{ marginTop: 'var(--spacing-6)' }}>
          <div className="card-header">
            <h2 className="card-title">Sự kiện gần đây</h2>
            <a href="/events" className="btn btn-ghost btn-sm">
              Xem tất cả
            </a>
          </div>

          <div className="events-timeline">
            {(realtimeEvents.length > 0 ? realtimeEvents : recentEvents || []).map(
              (event: any, index: number) => (
                <div
                  key={event.id || index}
                  className={`event-item ${event.event_type}`}
                >
                  <div className="event-thumbnail">
                    {event.snapshot_path ? (
                      <img
                        src={`/snapshots/${event.snapshot_path}`}
                        alt="Snapshot"
                        onError={(e) => {
                          e.currentTarget.style.display = 'none';
                        }}
                      />
                    ) : (
                      <div
                        style={{
                          width: '100%',
                          height: '100%',
                          display: 'flex',
                          alignItems: 'center',
                          justifyContent: 'center',
                          color: 'var(--text-muted)',
                        }}
                      >
                        {event.event_type === 'fire' ? (
                          <Flame size={24} />
                        ) : (
                          <Wind size={24} />
                        )}
                      </div>
                    )}
                  </div>

                  <div className="event-content">
                    <div className="event-header">
                      <span
                        className={`badge ${
                          event.event_type === 'fire' ? 'badge-fire' : 'badge-smoke'
                        }`}
                      >
                        {event.event_type === 'fire' ? '🔥 Cháy' : '💨 Khói'}
                      </span>
                      <span className="event-camera">{event.camera_name || event.site_id || event.camera_id}</span>
                      <span className="event-time">
                        {new Date(event.timestamp).toLocaleString('vi-VN')}
                      </span>
                    </div>
                    <div className="event-details">
                      <span>📍 {event.site_id}</span>
                      <span>🎯 {(event.confidence * 100).toFixed(1)}%</span>
                      <span>
                        {event.acknowledged ? (
                          <span style={{ color: 'var(--color-success)' }}>✓ Đã xác nhận</span>
                        ) : (
                          <span style={{ color: 'var(--color-warning)' }}>⏳ Chờ xử lý</span>
                        )}
                      </span>
                    </div>
                  </div>
                </div>
              )
            )}

            {(!recentEvents || recentEvents.length === 0) && realtimeEvents.length === 0 && (
              <div className="text-center text-muted" style={{ padding: 'var(--spacing-8)' }}>
                Không có sự kiện nào gần đây
              </div>
            )}
          </div>
        </div>
      </div>
    </>
  );
}
