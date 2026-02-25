import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { Camera, Plus, Trash2, Video, VideoOff, X, Play } from 'lucide-react';
import { camerasApi, eventsApi } from '@/lib/api';
import { useState } from 'react';
import { CameraStreamModal } from '@/components/CameraStreamModal';

interface CameraFormData {
  name: string;
  site_id: string;
  rtsp_url: string;
  description: string;
  detector_camera_id: string;
  codec: string;
  fps_sample: number;
  conf_fire: number;
  conf_smoke: number;
  conf_other: number;
}

export default function CamerasPage() {
  const queryClient = useQueryClient();
  const [showModal, setShowModal] = useState(false);
  const [showStreamModal, setShowStreamModal] = useState(false);
  const [selectedCamera, setSelectedCamera] = useState<any>(null);
  const [formData, setFormData] = useState<CameraFormData>({
    name: '',
    site_id: '',
    rtsp_url: '',
    description: '',
    detector_camera_id: 'cam-01',
    codec: 'h264',
    fps_sample: 3,
    conf_fire: 0.5,
    conf_smoke: 0.4,
    conf_other: 0.4,
  });
  const [editingCamera, setEditingCamera] = useState<any>(null);
  const [editData, setEditData] = useState<{
    detector_camera_id: string;
    codec: string;
    fps_sample: number;
    conf_fire: number;
    conf_smoke: number;
    conf_other: number;
  }>({ detector_camera_id: '', codec: 'h264', fps_sample: 3, conf_fire: 0.5, conf_smoke: 0.4, conf_other: 0.4 });

  const { data: cameras, isLoading } = useQuery({
    queryKey: ['cameras'],
    queryFn: () => camerasApi.list(),
  });

  // Get latest event for selected camera to show snapshot
  const { data: latestEvents } = useQuery({
    queryKey: ['latestEvent', selectedCamera?.id],
    queryFn: async () => {
      if (!selectedCamera) return [];
      try {
        return await eventsApi.list({ limit: 5, camera_id: selectedCamera.id });
      } catch {
        return [];
      }
    },
    enabled: !!selectedCamera && showStreamModal,
    refetchInterval: 3000, // Refresh every 3 seconds
  });
  
  const latestEventsArr = Array.isArray(latestEvents) ? latestEvents : (latestEvents as any)?.data ?? [];
  const latestEvent = latestEventsArr.length > 0 ? latestEventsArr[0] : null;

  const addCameraMutation = useMutation({
    mutationFn: (data: CameraFormData) => camerasApi.create(data),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['cameras'] });
      setShowModal(false);
      setFormData({
        name: '',
        site_id: '',
        rtsp_url: '',
        description: '',
        detector_camera_id: 'cam-01',
        codec: 'h264',
        fps_sample: 3,
        conf_fire: 0.5,
        conf_smoke: 0.4,
        conf_other: 0.4,
      });
    },
  });

  const updateCameraMutation = useMutation({
    mutationFn: ({ id, data }: { id: string; data: Record<string, unknown> }) =>
      camerasApi.update(id, data),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['cameras'] });
      setEditingCamera(null);
    },
  });

  const deleteCameraMutation = useMutation({
    mutationFn: (id: string) => camerasApi.delete(id),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['cameras'] }),
  });

  const camerasData = cameras || [];

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    addCameraMutation.mutate(formData);
  };

  const openStream = (camera: any) => {
    setSelectedCamera(camera);
    setShowStreamModal(true);
  };

  return (
    <>
      <header className="page-header">
        <div className="flex items-center justify-between">
          <div>
            <h1 className="page-title">Quản lý Camera</h1>
            <p className="page-subtitle">Cấu hình và giám sát các camera RTSP</p>
          </div>
          <button className="btn btn-primary" onClick={() => setShowModal(true)}>
            <Plus size={18} /> Thêm Camera
          </button>
        </div>
      </header>

      <div className="page-content">
        {isLoading ? (
          <div className="text-center text-muted">Đang tải...</div>
        ) : camerasData.length === 0 ? (
          <div className="card text-center">
            <Camera size={48} style={{ margin: '0 auto 16px', color: 'var(--text-muted)' }} />
            <h3>Chưa có camera nào</h3>
            <p className="text-muted">Thêm camera RTSP để bắt đầu giám sát</p>
            <button className="btn btn-primary" style={{ marginTop: 16 }} onClick={() => setShowModal(true)}>
              <Plus size={18} /> Thêm Camera đầu tiên
            </button>
          </div>
        ) : (
          <div className="camera-grid">
            {camerasData.map((camera: any) => (
              <div key={camera.id} className="camera-card">
                <div className="camera-preview" onClick={() => openStream(camera)} style={{ cursor: 'pointer' }}>
                  <div className={`camera-status-indicator ${camera.enabled ? 'online' : 'offline'}`} />
                  <div className="camera-preview-placeholder">
                    {camera.enabled ? (
                      <>
                        <Play size={32} />
                        <span style={{ fontSize: 12, marginTop: 8 }}>Click để xem stream</span>
                      </>
                    ) : (
                      <VideoOff size={32} />
                    )}
                  </div>
                </div>
                <div className="camera-info">
                  <h3 className="camera-name">{camera.name}</h3>
                  <p className="camera-site">📍 {camera.site_id}</p>
                  <p style={{ fontSize: 12, color: 'var(--text-muted)', marginTop: 4 }}>
                    Status: <span style={{ color: camera.status === 'streaming' ? 'var(--color-success)' : 'var(--color-warning)' }}>
                      {camera.status}
                    </span>
                  </p>
                </div>
                <div className="camera-actions" style={{ display: 'flex', gap: 8, padding: '8px 12px' }}>
                  <button 
                    className="btn btn-sm btn-secondary" 
                    onClick={() => openStream(camera)}
                    title="Xem stream"
                  >
                    <Video size={14} />
                  </button>
                  <button
                    className="btn btn-sm btn-secondary"
                    onClick={() => {
                      setEditingCamera(camera);
                      setEditData({
                        detector_camera_id: camera.detector_camera_id || 'cam-01',
                        codec: camera.codec || 'h264',
                        fps_sample: camera.fps_sample ?? 3,
                        conf_fire: camera.conf_fire ?? 0.5,
                        conf_smoke: camera.conf_smoke ?? 0.4,
                        conf_other: camera.conf_other ?? 0.4,
                      });
                    }}
                    title="Sửa cài đặt camera"
                  >
                    Sửa
                  </button>
                  <button 
                    className="btn btn-sm btn-secondary" 
                    onClick={() => deleteCameraMutation.mutate(camera.id)}
                    title="Xóa camera"
                  >
                    <Trash2 size={14} />
                  </button>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* Add Camera Modal */}
      {showModal && (
        <div className="modal-overlay" onClick={() => setShowModal(false)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <div className="modal-header">
              <h2>Thêm Camera RTSP</h2>
              <button className="btn btn-icon" onClick={() => setShowModal(false)}>
                <X size={20} />
              </button>
            </div>
            <form onSubmit={handleSubmit}>
              <div className="modal-body">
                <div className="form-group">
                  <label className="form-label">Tên Camera *</label>
                  <input
                    className="form-input"
                    placeholder="VD: Camera Nhà kho A"
                    value={formData.name}
                    onChange={(e) => setFormData({ ...formData, name: e.target.value })}
                    required
                  />
                </div>

                <div className="form-group">
                  <label className="form-label">RTSP URL *</label>
                  <input
                    className="form-input"
                    placeholder="rtsp://user:pass@192.168.1.100:554/stream"
                    value={formData.rtsp_url}
                    onChange={(e) => setFormData({ ...formData, rtsp_url: e.target.value })}
                    required
                  />
                  <small style={{ color: 'var(--text-muted)' }}>
                    Format: rtsp://username:password@ip:port/path
                  </small>
                </div>

                <div className="form-group">
                  <label className="form-label">Detector Camera ID *</label>
                  <input
                    className="form-input"
                    placeholder="cam-01"
                    value={formData.detector_camera_id}
                    onChange={(e) => setFormData({ ...formData, detector_camera_id: e.target.value })}
                    required
                  />
                  <small style={{ color: 'var(--text-muted)' }}>
                    Phải khớp với camera_id trong configs/cameras.yaml (VD: cam-01) để xem stream
                  </small>
                </div>

                <div className="form-group">
                  <label className="form-label">Site ID</label>
                  <input
                    className="form-input"
                    placeholder="VD: site-a"
                    value={formData.site_id}
                    onChange={(e) => setFormData({ ...formData, site_id: e.target.value })}
                  />
                </div>

                <div className="form-group">
                  <label className="form-label">Mô tả</label>
                  <input
                    className="form-input"
                    placeholder="VD: Góc 1 nhà kho"
                    value={formData.description}
                    onChange={(e) => setFormData({ ...formData, description: e.target.value })}
                  />
                </div>

                <div className="form-group">
                  <label className="form-label">Codec Video</label>
                  <select
                    className="form-input"
                    value={formData.codec}
                    onChange={(e) => setFormData({ ...formData, codec: e.target.value })}
                  >
                    <option value="h264">H.264 (phổ biến)</option>
                    <option value="h265">H.265 / HEVC (chất lượng cao hơn)</option>
                  </select>
                  <small style={{ color: 'var(--text-muted)' }}>
                    Chọn đúng codec camera đang dùng để stream hoạt động
                  </small>
                </div>

                <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr 1fr 1fr', gap: 12 }}>
                  <div className="form-group">
                    <label className="form-label">FPS Sample</label>
                    <input
                      className="form-input"
                      type="number"
                      min="1"
                      max="10"
                      value={formData.fps_sample}
                      onChange={(e) => setFormData({ ...formData, fps_sample: parseInt(e.target.value) })}
                    />
                  </div>
                  <div className="form-group">
                    <label className="form-label">Conf Fire</label>
                    <input
                      className="form-input"
                      type="number"
                      step="0.05"
                      min="0"
                      max="1"
                      value={formData.conf_fire}
                      onChange={(e) => setFormData({ ...formData, conf_fire: parseFloat(e.target.value) })}
                    />
                  </div>
                  <div className="form-group">
                    <label className="form-label">Conf Smoke</label>
                    <input
                      className="form-input"
                      type="number"
                      step="0.05"
                      min="0"
                      max="1"
                      value={formData.conf_smoke}
                      onChange={(e) => setFormData({ ...formData, conf_smoke: parseFloat(e.target.value) })}
                    />
                  </div>
                  <div className="form-group">
                    <label className="form-label">Conf Other</label>
                    <input
                      className="form-input"
                      type="number"
                      step="0.05"
                      min="0"
                      max="1"
                      value={formData.conf_other}
                      onChange={(e) => setFormData({ ...formData, conf_other: parseFloat(e.target.value) })}
                    />
                    <small style={{ color: 'var(--text-muted)' }}>Class 2</small>
                  </div>
                </div>
              </div>

              <div className="modal-footer">
                <button type="button" className="btn btn-secondary" onClick={() => setShowModal(false)}>
                  Hủy
                </button>
                <button 
                  type="submit" 
                  className="btn btn-primary"
                  disabled={addCameraMutation.isPending}
                >
                  {addCameraMutation.isPending ? 'Đang thêm...' : 'Thêm Camera'}
                </button>
              </div>
            </form>
          </div>
        </div>
      )}

      {/* Edit Camera Settings Modal */}
      {editingCamera && (
        <div className="modal-overlay" onClick={() => setEditingCamera(null)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <div className="modal-header">
              <h2>Sửa cài đặt — {editingCamera.name}</h2>
              <button className="btn btn-icon" onClick={() => setEditingCamera(null)}>
                <X size={20} />
              </button>
            </div>
            <div className="modal-body">
              <div className="form-group">
                <label className="form-label">Detector Camera ID</label>
                <input
                  className="form-input"
                  value={editData.detector_camera_id}
                  onChange={(e) => setEditData({ ...editData, detector_camera_id: e.target.value })}
                  placeholder="cam-01"
                />
                <small style={{ color: 'var(--text-muted)' }}>Khớp với camera_id trong configs/cameras.yaml</small>
              </div>
              <div className="form-group">
                <label className="form-label">Codec Video</label>
                <select
                  className="form-input"
                  value={editData.codec}
                  onChange={(e) => setEditData({ ...editData, codec: e.target.value })}
                >
                  <option value="h264">H.264</option>
                  <option value="h265">H.265 / HEVC</option>
                </select>
              </div>
              <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr 1fr 1fr', gap: 12 }}>
                <div className="form-group">
                  <label className="form-label">FPS Sample</label>
                  <input
                    className="form-input"
                    type="number"
                    min="1"
                    max="10"
                    value={editData.fps_sample}
                    onChange={(e) => setEditData({ ...editData, fps_sample: parseInt(e.target.value) || 3 })}
                  />
                </div>
                <div className="form-group">
                  <label className="form-label">Conf Fire</label>
                  <input
                    className="form-input"
                    type="number"
                    step="0.05"
                    min="0"
                    max="1"
                    value={editData.conf_fire}
                    onChange={(e) => setEditData({ ...editData, conf_fire: parseFloat(e.target.value) || 0.5 })}
                  />
                </div>
                <div className="form-group">
                  <label className="form-label">Conf Smoke</label>
                  <input
                    className="form-input"
                    type="number"
                    step="0.05"
                    min="0"
                    max="1"
                    value={editData.conf_smoke}
                    onChange={(e) => setEditData({ ...editData, conf_smoke: parseFloat(e.target.value) || 0.4 })}
                  />
                </div>
                <div className="form-group">
                  <label className="form-label">Conf Other</label>
                  <input
                    className="form-input"
                    type="number"
                    step="0.05"
                    min="0"
                    max="1"
                    value={editData.conf_other}
                    onChange={(e) => setEditData({ ...editData, conf_other: parseFloat(e.target.value) || 0.4 })}
                  />
                </div>
              </div>
            </div>
            <div className="modal-footer">
              <button type="button" className="btn btn-secondary" onClick={() => setEditingCamera(null)}>
                Hủy
              </button>
              <button
                type="button"
                className="btn btn-primary"
                disabled={updateCameraMutation.isPending}
                onClick={() => updateCameraMutation.mutate({ id: editingCamera.id, data: editData })}
              >
                {updateCameraMutation.isPending ? 'Đang lưu...' : 'Lưu'}
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Stream Modal - HLS live stream + fire/smoke overlay */}
      {showStreamModal && selectedCamera && (
        <CameraStreamModal
          camera={selectedCamera}
          onClose={() => setShowStreamModal(false)}
          latestEvent={latestEvent}
        />
      )}
    </>
  );
}
