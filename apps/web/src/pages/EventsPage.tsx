import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { Flame, Wind, CheckCircle } from 'lucide-react';
import { eventsApi } from '@/lib/api';
import { useState } from 'react';

export default function EventsPage() {
  const queryClient = useQueryClient();
  const [filter, setFilter] = useState({ event_type: '', limit: 50 });

  const { data: events, isLoading } = useQuery({
    queryKey: ['events', filter],
    queryFn: () => eventsApi.list(filter),
  });

  const acknowledgeMutation = useMutation({
    mutationFn: (id: string) => eventsApi.acknowledge(id),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['events'] }),
  });

  const eventsData = events || [];

  return (
    <>
      <header className="page-header">
        <h1 className="page-title">Sự kiện phát hiện</h1>
        <p className="page-subtitle">Lịch sử các sự kiện cháy và khói</p>
      </header>

      <div className="page-content">
        <div className="card" style={{ marginBottom: 16 }}>
          <div className="flex gap-4">
            <select
              className="form-input"
              style={{ width: 200 }}
              value={filter.event_type}
              onChange={(e) => setFilter({ ...filter, event_type: e.target.value })}
            >
              <option value="">Tất cả loại</option>
              <option value="fire">🔥 Cháy</option>
              <option value="smoke">💨 Khói</option>
            </select>
          </div>
        </div>

        {isLoading ? (
          <div className="text-center text-muted">Đang tải...</div>
        ) : eventsData.length === 0 ? (
          <div className="card text-center">
            <CheckCircle size={48} style={{ margin: '0 auto 16px', color: 'var(--color-success)' }} />
            <h3>Không có sự kiện nào</h3>
          </div>
        ) : (
          <div className="events-timeline">
            {eventsData.map((event: any) => (
              <div key={event.id} className={`event-item ${event.event_type}`}>
                <div className="event-content">
                  <div className="event-header">
                    <span className={`badge ${event.event_type === 'fire' ? 'badge-fire' : 'badge-smoke'}`}>
                      {event.event_type === 'fire' ? <><Flame size={14} /> Cháy</> : <><Wind size={14} /> Khói</>}
                    </span>
                    <span className="event-camera">{event.camera_id}</span>
                    <span className="event-time">{new Date(event.timestamp).toLocaleString('vi-VN')}</span>
                  </div>
                  <div className="event-details">
                    <span>📍 {event.site_id}</span>
                    <span>🎯 {(event.confidence * 100).toFixed(1)}%</span>
                    {event.acknowledged ? (
                      <span style={{ color: 'var(--color-success)' }}>✓ Đã xác nhận</span>
                    ) : (
                      <button
                        className="btn btn-sm btn-secondary"
                        onClick={() => acknowledgeMutation.mutate(event.id)}
                      >
                        Xác nhận
                      </button>
                    )}
                  </div>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </>
  );
}
