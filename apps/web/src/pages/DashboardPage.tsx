import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
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
import { useState, useMemo } from 'react';

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
  const queryClient = useQueryClient();
  const [realtimeEvents, setRealtimeEvents] = useState<any[]>([]);
  const [ackError, setAckError] = useState<string | null>(null);

  const acknowledgeMutation = useMutation({
    mutationFn: (id: string) => eventsApi.acknowledge(id),
    onMutate: async (id) => {
      setAckError(null);
      await queryClient.cancelQueries({ queryKey: ['recentEvents'] });
      const previousData = queryClient.getQueriesData<{ data: any[]; total: number }>({ queryKey: ['recentEvents'] });
      // Optimistically mark acknowledged in query cache
      queryClient.setQueriesData<{ data: any[]; total: number }>(
        { queryKey: ['recentEvents'] },
        (old) => {
          if (!old?.data) return old;
          return { ...old, data: old.data.map((e: any) => e.id === id ? { ...e, acknowledged: true } : e) };
        }
      );
      // Optimistically mark acknowledged in realtime events state
      const previousRealtime = realtimeEvents;
      setRealtimeEvents(prev => prev.map(e => e.id === id ? { ...e, acknowledged: true } : e));
      return { previousData, previousRealtime };
    },
    onError: (_err, _id, context: any) => {
      context?.previousData?.forEach(([key, data]: [readonly unknown[], unknown]) => {
        queryClient.setQueryData(key as any, data);
      });
      if (context?.previousRealtime) setRealtimeEvents(context.previousRealtime);
      setAckError('Xác nhận thất bại. Vui lòng thử lại.');
    },
    onSettled: () => {
      queryClient.invalidateQueries({ queryKey: ['recentEvents'] });
      queryClient.invalidateQueries({ queryKey: ['eventStats'] });
    },
  });

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

  // WebSocket for real-time events — must be declared before queries that depend on isConnected
  const { isConnected } = useWebSocket({
    onMessage: (message) => {
      setRealtimeEvents((prev) => [message, ...prev].slice(0, 5));
    },
  });

  // Fetch recent events — poll only when WebSocket is down; WS pushes live events otherwise
  const { data: recentEventsPage } = useQuery({
    queryKey: ['recentEvents'],
    queryFn: () => eventsApi.list({ limit: 5 }),
    refetchInterval: isConnected ? false : 10000,
  });
  const recentEvents = recentEventsPage?.data;

  // Single batch query for all camera statuses — 1 DB query + 1 gRPC call per cycle
  const { data: allCameraStatuses, isLoading: statusesLoading } = useQuery({
    queryKey: ['cameraStatuses'],
    queryFn: () => camerasApi.getAllStatuses(),
    refetchInterval: 15000,
    retry: false,
  });

  const statsData = stats || {
    fire_count: 0,
    smoke_count: 0,
    total: 0,
    acknowledged_count: 0,
  };

  const camerasData = cameras || [];
  const activeCount = camerasData.filter((c: any) => c.enabled).length;

  // Derive active unacknowledged critical events for Glitch effect and Terminal red alerts
  const activeCritEvents = useMemo(() => {
    return ((recentEventsPage?.data || []) as any[])
      .filter(e => !e.acknowledged && (e.event_type === 'fire' || e.event_type === 'smoke')).length;
  }, [recentEventsPage]);

  return (
    <>
      {activeCritEvents > 0 && <div className="critical-alert-active" />}
      <div className={activeCritEvents > 0 ? "glitch-effect" : ""}>
        <header className="page-header" style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start' }}>
        <div>
          <h1 className="page-title">Dashboard</h1>
          <p className="page-subtitle">Tổng quan hệ thống phát hiện cháy và khói</p>
        </div>
        <div style={{
          display: 'flex', alignItems: 'center', gap: 8, fontSize: 13,
          color: isConnected ? 'var(--color-success)' : 'var(--text-muted)',
          padding: '6px 12px',
          background: 'var(--bg-card)',
          borderRadius: 20,
          border: '1px solid var(--border-color)',
          boxShadow: isConnected ? 'var(--glow-success)' : 'none',
        }}>
          <span style={{
            width: 8, height: 8, borderRadius: '50%',
            backgroundColor: isConnected ? 'var(--color-success)' : 'var(--color-warning)',
            display: 'inline-block',
            boxShadow: isConnected ? '0 0 0 2px color-mix(in srgb, var(--color-success) 25%, transparent)' : 'none',
            animation: isConnected ? 'pulse-glow-fire 2s infinite' : 'none',
          }} />
          <span style={{ fontWeight: 500 }}>{isConnected ? 'System Online' : 'Connecting...'}</span>
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
              {camerasData.map((camera: any) => {
                const st = allCameraStatuses?.[camera.id] as CameraStatusData | undefined;
                const statusKey = st?.status || 'unknown';
                const cfg = STATUS_CONFIG[statusKey] || STATUS_CONFIG['unknown'];
                const isLoading = statusesLoading;

                return (
                  <div key={camera.id} style={{
                    border: '1px solid var(--border-color)',
                    borderRadius: 12,
                    padding: '14px 16px',
                    background: 'var(--bg-secondary)',
                    boxShadow: 'var(--shadow-sm)',
                    position: 'relative',
                    overflow: 'hidden',
                  }}>
                    {/* Glowing active indicator line at the top */}
                    {statusKey === 'streaming' && (
                      <div style={{
                        position: 'absolute', top: 0, left: 0, right: 0, height: 3,
                        background: 'linear-gradient(90deg, transparent, var(--color-success), transparent)',
                        opacity: 0.8
                      }} />
                    )}
                    {statusKey === 'failed' && (
                      <div style={{
                        position: 'absolute', top: 0, left: 0, right: 0, height: 3,
                        background: 'linear-gradient(90deg, transparent, var(--color-error), transparent)',
                        opacity: 0.8
                      }} />
                    )}

                    <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start', marginBottom: 12 }}>
                      <div>
                        <div style={{ fontWeight: 600, fontSize: 14 }}>{camera.name}</div>
                        <div style={{ fontSize: 11, color: 'var(--text-muted)', marginTop: 2, fontFamily: 'var(--font-mono)' }}>
                          ID: {camera.detector_camera_id || camera.id.slice(0, 8)}
                        </div>
                      </div>
                      {isLoading ? (
                        <span style={{ fontSize: 12, color: 'var(--text-muted)' }}>...</span>
                      ) : (
                        <span style={{
                          display: 'flex', alignItems: 'center', gap: 6,
                          fontSize: 12, color: cfg.color, fontWeight: 500,
                          background: `color-mix(in srgb, ${cfg.color} 10%, transparent)`,
                          padding: '4px 10px', borderRadius: 12,
                          border: `1px solid color-mix(in srgb, ${cfg.color} 30%, transparent)`,
                        }}>
                          {cfg.icon}{cfg.label}
                        </span>
                      )}
                    </div>
                    {st ? (
                      <div style={{ display: 'flex', gap: 12, fontSize: 12, color: 'var(--text-secondary)' }}>
                        {st.fps_in != null && (
                          <div style={{ display: 'flex', alignItems: 'center', gap: 4 }} title="FPS Camera gửi vào">
                            <span style={{ color: 'var(--color-primary)' }}>▶</span> {Number(st.fps_in).toFixed(1)} <span style={{fontSize: 10, color: 'var(--text-muted)'}}>FPS IN</span>
                          </div>
                        )}
                        {st.fps_infer != null && (
                          <div style={{ display: 'flex', alignItems: 'center', gap: 4 }} title="FPS CPU/GPU xử lý">
                            <span style={{ color: 'var(--color-secondary)' }}>⚡</span> {Number(st.fps_infer).toFixed(1)} <span style={{fontSize: 10, color: 'var(--text-muted)'}}>FPS OUT</span>
                          </div>
                        )}
                        {(st.reconnect_count ?? 0) > 0 && (
                          <span title="Số lần reconnect" style={{ color: 'var(--color-warning)' }}>
                            ↺ {st.reconnect_count}
                          </span>
                        )}
                        {st.error_message && (
                          <span title={st.error_message} style={{ color: 'var(--color-error)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', maxWidth: 120 }}>
                            {st.error_message}
                          </span>
                        )}
                      </div>
                    ) : (
                       <div style={{ height: 18 }}></div>
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

          {ackError && (
            <div style={{ background: 'var(--color-danger, #e53e3e)', color: '#fff', padding: '8px 16px', borderRadius: 6, marginBottom: 12, fontSize: 14 }}>
              {ackError}
            </div>
          )}
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
                        src={`/api/snapshots/${event.snapshot_path}`}
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
                      {event.acknowledged ? (
                        <span style={{ color: 'var(--color-success)' }}>✓ Đã xác nhận</span>
                      ) : (
                        <button
                          className="btn btn-sm btn-secondary"
                          disabled={acknowledgeMutation.isPending}
                          onClick={() => acknowledgeMutation.mutate(event.id)}
                        >
                          Xác nhận
                        </button>
                      )}
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
      </div>
    </>
  );
}
