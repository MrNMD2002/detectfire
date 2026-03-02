import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { Flame, Wind, CheckCircle, ChevronLeft, ChevronRight, Image } from 'lucide-react';
import { eventsApi } from '@/lib/api';
import { useState } from 'react';

const PAGE_SIZE_OPTIONS = [20, 50, 100];

export default function EventsPage() {
  const queryClient = useQueryClient();
  const [filter, setFilter] = useState({ event_type: '' });
  const [page, setPage] = useState(0);
  const [pageSize, setPageSize] = useState(50);
  const [snapshotModal, setSnapshotModal] = useState<string | null>(null);

  const { data: eventsPage, isLoading } = useQuery({
    queryKey: ['events', filter, page, pageSize],
    queryFn: () => eventsApi.list({
      event_type: filter.event_type || undefined,
      limit: pageSize,
      offset: page * pageSize,
    }),
  });

  const [ackError, setAckError] = useState<string | null>(null);

  const acknowledgeMutation = useMutation({
    mutationFn: (id: string) => eventsApi.acknowledge(id),
    onMutate: async (id) => {
      setAckError(null);
      // Cancel in-flight refetches so they don't overwrite our optimistic update
      await queryClient.cancelQueries({ queryKey: ['events'] });
      // Snapshot all events cache entries for rollback
      const previousData = queryClient.getQueriesData<{ data: any[]; total: number }>({ queryKey: ['events'] });
      // Optimistically mark the event as acknowledged in every cached page
      queryClient.setQueriesData<{ data: any[]; total: number }>(
        { queryKey: ['events'] },
        (old) => {
          if (!old?.data) return old;
          return { ...old, data: old.data.map((e) => e.id === id ? { ...e, acknowledged: true } : e) };
        }
      );
      return { previousData };
    },
    onError: (_err, _id, context: any) => {
      // Roll back to pre-mutation state
      context?.previousData?.forEach(([key, data]: [readonly unknown[], unknown]) => {
        queryClient.setQueryData(key as any, data);
      });
      setAckError('Xác nhận thất bại. Vui lòng thử lại.');
    },
    onSettled: () => {
      // Always sync with server after mutation (success or error)
      queryClient.invalidateQueries({ queryKey: ['events'] });
    },
  });

  // Only show fire/smoke events — stream_up/stream_down are internal status events
  const eventsData: any[] = (eventsPage?.data || []).filter(
    (e: any) => e.event_type === 'fire' || e.event_type === 'smoke'
  );
  const total: number = eventsPage?.total ?? 0;
  const totalPages = Math.max(1, Math.ceil(total / pageSize));

  const handleFilterChange = (event_type: string) => {
    setFilter({ event_type });
    setPage(0);
  };

  return (
    <>
      <header className="page-header">
        <h1 className="page-title">Sự kiện phát hiện</h1>
        <p className="page-subtitle">Lịch sử các sự kiện cháy và khói</p>
      </header>

      <div className="page-content">
        {/* Filter bar */}
        <div className="card" style={{ marginBottom: 16 }}>
          <div style={{ display: 'flex', gap: 12, alignItems: 'center', flexWrap: 'wrap' }}>
            <select
              className="form-input"
              style={{ width: 180 }}
              value={filter.event_type}
              onChange={(e) => handleFilterChange(e.target.value)}
            >
              <option value="">Tất cả loại</option>
              <option value="fire">🔥 Cháy</option>
              <option value="smoke">💨 Khói</option>
            </select>

            {total > 0 && (
              <span style={{ fontSize: 13, color: 'var(--text-muted)' }}>
                {total.toLocaleString('vi-VN')} sự kiện
              </span>
            )}

            <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginLeft: 'auto' }}>
              <span style={{ fontSize: 13, color: 'var(--text-muted)' }}>Hiển thị:</span>
              <select
                className="form-input"
                style={{ width: 80 }}
                value={pageSize}
                onChange={(e) => { setPageSize(Number(e.target.value)); setPage(0); }}
              >
                {PAGE_SIZE_OPTIONS.map(n => (
                  <option key={n} value={n}>{n}</option>
                ))}
              </select>
              <span style={{ fontSize: 13, color: 'var(--text-muted)' }}>/ trang</span>
            </div>
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
          <>
            {ackError && (
              <div style={{ background: 'var(--color-danger, #e53e3e)', color: '#fff', padding: '8px 16px', borderRadius: 6, marginBottom: 12, fontSize: 14 }}>
                {ackError}
              </div>
            )}
            <div className="events-timeline">
              {eventsData.map((event: any) => (
                <div key={event.id} className={`event-item ${event.event_type}`}>
                  {/* Snapshot thumbnail */}
                  <div
                    className="event-thumbnail"
                    style={{ cursor: event.snapshot_path ? 'pointer' : 'default' }}
                    onClick={() => event.snapshot_path && setSnapshotModal(event.snapshot_path)}
                  >
                    {event.snapshot_path ? (
                      <img
                        src={`/api/snapshots/${event.snapshot_path}`}
                        alt="Snapshot"
                        onError={(e) => { e.currentTarget.style.display = 'none'; }}
                      />
                    ) : (
                      <div style={{ width: '100%', height: '100%', display: 'flex', alignItems: 'center', justifyContent: 'center', color: 'var(--text-muted)' }}>
                        {event.event_type === 'fire' ? <Flame size={20} /> : <Wind size={20} />}
                      </div>
                    )}
                  </div>

                  <div className="event-content">
                    <div className="event-header">
                      <span className={`badge ${event.event_type === 'fire' ? 'badge-fire' : 'badge-smoke'}`}>
                        {event.event_type === 'fire' ? <><Flame size={14} /> Cháy</> : <><Wind size={14} /> Khói</>}
                      </span>
                      <span className="event-camera">{event.camera_name || event.site_id}</span>
                      <span className="event-time">{new Date(event.timestamp).toLocaleString('vi-VN')}</span>
                    </div>
                    <div className="event-details">
                      <span>📍 {event.site_id}</span>
                      <span>🎯 {(event.confidence * 100).toFixed(1)}%</span>
                      {event.snapshot_path && (
                        <span
                          style={{ cursor: 'pointer', color: 'var(--color-primary)', display: 'inline-flex', alignItems: 'center', gap: 3 }}
                          onClick={() => setSnapshotModal(event.snapshot_path)}
                        >
                          <Image size={13} /> Ảnh
                        </span>
                      )}
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
              ))}
            </div>

            {/* Pagination controls */}
            <div style={{ display: 'flex', justifyContent: 'center', alignItems: 'center', gap: 12, marginTop: 16, padding: '8px 0' }}>
              <button
                className="btn btn-secondary"
                disabled={page === 0}
                onClick={() => setPage(p => p - 1)}
              >
                <ChevronLeft size={16} />
              </button>
              <span style={{ fontSize: 14, color: 'var(--text-muted)' }}>
                Trang {page + 1} / {totalPages}
              </span>
              <button
                className="btn btn-secondary"
                disabled={page >= totalPages - 1}
                onClick={() => setPage(p => p + 1)}
              >
                <ChevronRight size={16} />
              </button>
            </div>
          </>
        )}
      </div>

      {/* Snapshot lightbox */}
      {snapshotModal && (
        <div
          className="modal-overlay"
          onClick={() => setSnapshotModal(null)}
          style={{ zIndex: 1000 }}
        >
          <div onClick={(e) => e.stopPropagation()} style={{ maxWidth: '90vw', maxHeight: '90vh' }}>
            <img
              src={`/api/snapshots/${snapshotModal}`}
              alt="Snapshot"
              style={{ maxWidth: '90vw', maxHeight: '90vh', objectFit: 'contain', borderRadius: 8 }}
            />
          </div>
        </div>
      )}
    </>
  );
}
