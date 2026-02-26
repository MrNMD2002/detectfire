import { useState, useEffect } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { Send, CheckCircle, AlertCircle, Loader, Eye, EyeOff, Save, Edit3 } from 'lucide-react';
import { settingsApi } from '@/lib/api';

export default function SettingsPage() {
  const queryClient = useQueryClient();

  const { data: telegramCfg, isLoading } = useQuery({
    queryKey: ['telegramSettings'],
    queryFn: () => settingsApi.getTelegram(),
  });

  // ── Edit form state ──────────────────────────────────────────────────────
  const [editing, setEditing] = useState(false);
  const [showToken, setShowToken] = useState(false);
  const [form, setForm] = useState({
    bot_token: '',
    default_chat_id: '',
    enabled: true,
    rate_limit_per_minute: 10,
  });

  // Populate form when data loads or when entering edit mode
  useEffect(() => {
    if (telegramCfg && editing) {
      setForm(prev => ({
        ...prev,
        default_chat_id: telegramCfg.default_chat_id || '',
        enabled: telegramCfg.enabled ?? true,
        rate_limit_per_minute: telegramCfg.rate_limit_per_minute ?? 10,
        bot_token: '', // never pre-fill token — user must re-enter if changing
      }));
    }
  }, [telegramCfg, editing]);

  const saveMutation = useMutation({
    mutationFn: () =>
      settingsApi.updateTelegram({
        bot_token: form.bot_token || undefined, // empty = keep existing
        default_chat_id: form.default_chat_id,
        enabled: form.enabled,
        rate_limit_per_minute: form.rate_limit_per_minute,
      }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['telegramSettings'] });
      setEditing(false);
      setShowToken(false);
    },
  });

  const testMutation = useMutation({
    mutationFn: () => settingsApi.testTelegram(),
  });

  const handleSave = (e: React.FormEvent) => {
    e.preventDefault();
    saveMutation.mutate();
  };

  return (
    <>
      <header className="page-header">
        <h1 className="page-title">Cài đặt</h1>
        <p className="page-subtitle">Cấu hình hệ thống phát hiện cháy khói</p>
      </header>

      <div className="page-content">
        {/* ── Telegram settings card ─────────────────────────────────────── */}
        <div className="card" style={{ maxWidth: 620 }}>
          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 20 }}>
            <div>
              <h3 style={{ margin: 0 }}>Thông báo Telegram</h3>
              <p style={{ color: 'var(--text-muted)', fontSize: 12, marginTop: 4 }}>
                Thay đổi có hiệu lực ngay — không cần restart
              </p>
            </div>
            {!editing && !isLoading && (
              <button
                className="btn btn-secondary"
                style={{ display: 'flex', alignItems: 'center', gap: 6 }}
                onClick={() => setEditing(true)}
              >
                <Edit3 size={14} /> Chỉnh sửa
              </button>
            )}
          </div>

          {isLoading ? (
            <div style={{ color: 'var(--text-muted)', fontSize: 13 }}>Đang tải...</div>
          ) : editing ? (
            /* ── Edit form ─────────────────────────────────────────────── */
            <form onSubmit={handleSave}>
              <div style={{ display: 'grid', gap: 16, marginBottom: 20 }}>

                {/* Enabled toggle */}
                <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
                  <label style={{ fontSize: 13, color: 'var(--text-muted)', minWidth: 140 }}>
                    Trạng thái:
                  </label>
                  <label style={{ display: 'flex', alignItems: 'center', gap: 8, cursor: 'pointer' }}>
                    <input
                      type="checkbox"
                      checked={form.enabled}
                      onChange={e => setForm(f => ({ ...f, enabled: e.target.checked }))}
                    />
                    <span style={{ fontSize: 13, fontWeight: 500 }}>
                      {form.enabled ? '✓ Bật thông báo' : '✗ Tắt thông báo'}
                    </span>
                  </label>
                </div>

                {/* Bot Token */}
                <div>
                  <label style={{ display: 'block', fontSize: 13, color: 'var(--text-muted)', marginBottom: 6 }}>
                    Bot Token
                    <span style={{ fontSize: 11, marginLeft: 8, color: 'var(--text-muted)' }}>
                      (để trống = giữ nguyên token hiện tại)
                    </span>
                  </label>
                  <div style={{ position: 'relative' }}>
                    <input
                      type={showToken ? 'text' : 'password'}
                      className="form-input"
                      placeholder="Nhập bot token mới..."
                      value={form.bot_token}
                      onChange={e => setForm(f => ({ ...f, bot_token: e.target.value }))}
                      style={{ paddingRight: 40, width: '100%', boxSizing: 'border-box', fontFamily: 'monospace', fontSize: 13 }}
                      autoComplete="off"
                    />
                    <button
                      type="button"
                      onClick={() => setShowToken(v => !v)}
                      style={{
                        position: 'absolute', right: 10, top: '50%', transform: 'translateY(-50%)',
                        background: 'none', border: 'none', cursor: 'pointer', color: 'var(--text-muted)', padding: 0,
                      }}
                    >
                      {showToken ? <EyeOff size={15} /> : <Eye size={15} />}
                    </button>
                  </div>
                  <p style={{ fontSize: 11, color: 'var(--text-muted)', marginTop: 4 }}>
                    Lấy token từ <code>@BotFather</code> trên Telegram
                  </p>
                </div>

                {/* Chat ID */}
                <div>
                  <label style={{ display: 'block', fontSize: 13, color: 'var(--text-muted)', marginBottom: 6 }}>
                    Chat ID (hoặc Group ID)
                  </label>
                  <input
                    type="text"
                    className="form-input"
                    placeholder="Ví dụ: 123456789 hoặc -100123456789"
                    value={form.default_chat_id}
                    onChange={e => setForm(f => ({ ...f, default_chat_id: e.target.value }))}
                    style={{ width: '100%', boxSizing: 'border-box', fontFamily: 'monospace', fontSize: 13 }}
                  />
                  <p style={{ fontSize: 11, color: 'var(--text-muted)', marginTop: 4 }}>
                    Dùng <code>@userinfobot</code> để lấy Chat ID của bạn
                  </p>
                </div>

                {/* Rate limit */}
                <div>
                  <label style={{ display: 'block', fontSize: 13, color: 'var(--text-muted)', marginBottom: 6 }}>
                    Giới hạn tin nhắn / phút
                  </label>
                  <input
                    type="number"
                    className="form-input"
                    min={1}
                    max={60}
                    value={form.rate_limit_per_minute}
                    onChange={e => setForm(f => ({ ...f, rate_limit_per_minute: Number(e.target.value) }))}
                    style={{ width: 100, fontFamily: 'monospace', fontSize: 13 }}
                  />
                </div>
              </div>

              {/* Save / Cancel */}
              <div style={{ display: 'flex', gap: 10, alignItems: 'center' }}>
                <button
                  type="submit"
                  className="btn btn-primary"
                  disabled={saveMutation.isPending}
                >
                  {saveMutation.isPending
                    ? <><Loader size={15} /> Đang lưu...</>
                    : <><Save size={15} /> Lưu cài đặt</>}
                </button>
                <button
                  type="button"
                  className="btn btn-secondary"
                  onClick={() => { setEditing(false); setShowToken(false); saveMutation.reset(); }}
                  disabled={saveMutation.isPending}
                >
                  Hủy
                </button>

                {saveMutation.isSuccess && (
                  <span style={{ color: 'var(--color-success)', display: 'flex', alignItems: 'center', gap: 5, fontSize: 13 }}>
                    <CheckCircle size={15} /> Đã lưu
                  </span>
                )}
                {saveMutation.isError && (
                  <span style={{ color: 'var(--color-danger)', display: 'flex', alignItems: 'center', gap: 5, fontSize: 13 }}>
                    <AlertCircle size={15} />
                    {(saveMutation.error as any)?.response?.data?.message || 'Lưu thất bại'}
                  </span>
                )}
              </div>
            </form>
          ) : (
            /* ── Read-only view ────────────────────────────────────────── */
            <>
              {telegramCfg && (
                <div style={{ display: 'grid', gap: 10, marginBottom: 20 }}>
                  <Row label="Trạng thái">
                    <span style={{
                      fontSize: 13, fontWeight: 600,
                      color: telegramCfg.enabled ? 'var(--color-success)' : 'var(--text-muted)',
                    }}>
                      {telegramCfg.enabled ? '✓ Đang bật' : '✗ Tắt'}
                    </span>
                  </Row>
                  <Row label="Bot Token">
                    <code style={{ fontSize: 13 }}>{telegramCfg.bot_token_masked}</code>
                  </Row>
                  <Row label="Chat ID">
                    <code style={{ fontSize: 13 }}>{telegramCfg.default_chat_id || '(chưa đặt)'}</code>
                  </Row>
                  <Row label="Rate limit">
                    <span style={{ fontSize: 13 }}>{telegramCfg.rate_limit_per_minute} msg/phút</span>
                  </Row>
                </div>
              )}

              {/* Test button */}
              <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
                <button
                  className="btn btn-primary"
                  disabled={testMutation.isPending || !telegramCfg?.enabled}
                  onClick={() => testMutation.mutate()}
                >
                  {testMutation.isPending
                    ? <><Loader size={15} /> Đang gửi...</>
                    : <><Send size={15} /> Gửi test message</>}
                </button>

                {testMutation.isSuccess && (
                  <span style={{ color: 'var(--color-success)', display: 'flex', alignItems: 'center', gap: 5, fontSize: 13 }}>
                    <CheckCircle size={15} /> Đã gửi thành công
                  </span>
                )}
                {testMutation.isError && (
                  <span style={{ color: 'var(--color-danger)', display: 'flex', alignItems: 'center', gap: 5, fontSize: 13 }}>
                    <AlertCircle size={15} />
                    {(testMutation.error as any)?.response?.data?.message || 'Gửi thất bại'}
                  </span>
                )}
              </div>

              {!telegramCfg?.enabled && !isLoading && (
                <p style={{ marginTop: 10, fontSize: 12, color: 'var(--color-warning)' }}>
                  ⚠ Telegram chưa được bật. Nhấn <b>Chỉnh sửa</b> để bật và cấu hình.
                </p>
              )}
            </>
          )}
        </div>

        {/* ── Inference info ─────────────────────────────────────────────── */}
        <div className="card" style={{ maxWidth: 620, marginTop: 24 }}>
          <h3 style={{ marginBottom: 8 }}>Inference Engine</h3>
          <p style={{ color: 'var(--text-muted)', fontSize: 13, marginBottom: 16 }}>
            Cấu hình inference đọc từ <code>configs/settings.yaml</code>.
            Thay đổi cần restart detector service.
          </p>
          <table style={{ fontSize: 13, borderCollapse: 'collapse', width: '100%' }}>
            <tbody>
              {[
                ['Model', '/app/models/best.onnx'],
                ['Device', 'CUDA GPU (cuda:0)'],
                ['Framework', 'ONNX Runtime 1.23.2 GPU'],
                ['Detector gRPC', 'detector:50051'],
                ['MJPEG stream', 'detector:51051'],
              ].map(([k, v]) => (
                <tr key={k} style={{ borderBottom: '1px solid var(--border-color)' }}>
                  <td style={{ padding: '8px 0', color: 'var(--text-muted)', width: 140 }}>{k}:</td>
                  <td style={{ padding: '8px 0' }}><code>{v}</code></td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>
    </>
  );
}

// ── Small helper component ────────────────────────────────────────────────────

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div style={{ display: 'flex', gap: 12, alignItems: 'center' }}>
      <span style={{ color: 'var(--text-muted)', fontSize: 13, minWidth: 120 }}>{label}:</span>
      <span>{children}</span>
    </div>
  );
}
