import { useQuery } from '@tanstack/react-query';
import { 
  Flame, 
  Wind, 
  Camera, 
  CheckCircle,
  TrendingUp,
  TrendingDown,
  Activity
} from 'lucide-react';
import { eventsApi, camerasApi } from '@/lib/api';
import { useWebSocket } from '@/hooks/useWebSocket';
import { useState } from 'react';

export default function DashboardPage() {
  const [realtimeEvents, setRealtimeEvents] = useState<any[]>([]);

  // Fetch stats
  const { data: stats } = useQuery({
    queryKey: ['eventStats'],
    queryFn: () => eventsApi.stats(),
    refetchInterval: 30000,
  });

  // Fetch cameras
  const { data: cameras } = useQuery({
    queryKey: ['cameras'],
    queryFn: () => camerasApi.list(),
  });

  // Fetch recent events
  const { data: recentEvents } = useQuery({
    queryKey: ['recentEvents'],
    queryFn: () => eventsApi.list({ limit: 5 }),
    refetchInterval: 10000,
  });

  // WebSocket for real-time events
  useWebSocket({
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
      <header className="page-header">
        <h1 className="page-title">Dashboard</h1>
        <p className="page-subtitle">Tổng quan hệ thống phát hiện cháy và khói</p>
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
                      <span className="event-camera">{event.camera_name || event.camera_id}</span>
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
