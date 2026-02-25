import { useQuery, useMutation } from '@tanstack/react-query';
import { Send, CheckCircle, AlertCircle, Loader } from 'lucide-react';
import { settingsApi } from '@/lib/api';

export default function SettingsPage() {
  const { data: telegramCfg, isLoading } = useQuery({
    queryKey: ['telegramSettings'],
    queryFn: () => settingsApi.getTelegram(),
  });

  const testMutation = useMutation({
    mutationFn: () => settingsApi.testTelegram(),
  });

  return (
    <>
      <header className="page-header">
        <h1 className="page-title">Cài đặt</h1>
        <p className="page-subtitle">Cấu hình hệ thống phát hiện cháy khói</p>
      </header>

      <div className="page-content">
        {/* Telegram settings */}
        <div className="card" style={{ maxWidth: 600 }}>
          <h3 style={{ marginBottom: 8 }}>Thông báo Telegram</h3>
          <p style={{ color: 'var(--text-muted)', fontSize: 13, marginBottom: 20 }}>
            Cấu hình qua biến môi trường:{' '}
            <code>FIRE_DETECT__TELEGRAM__BOT_TOKEN</code>,{' '}
            <code>FIRE_DETECT__TELEGRAM__DEFAULT_CHAT_ID</code>,{' '}
            <code>FIRE_DETECT__TELEGRAM__ENABLED</code>.
          </p>

          {isLoading ? (
            <div style={{ color: 'var(--text-muted)', fontSize: 13 }}>Đang tải...</div>
          ) : telegramCfg && (
            <div style={{ display: 'grid', gap: 12, marginBottom: 20 }}>
              <div style={{ display: 'flex', gap: 12, alignItems: 'center' }}>
                <span style={{ color: 'var(--text-muted)', fontSize: 13, minWidth: 120 }}>Trạng thái:</span>
                <span style={{
                  fontSize: 13, fontWeight: 600,
                  color: telegramCfg.enabled ? 'var(--color-success)' : 'var(--text-muted)',
                }}>
                  {telegramCfg.enabled ? '✓ Đang bật' : '✗ Tắt'}
                </span>
              </div>
              <div style={{ display: 'flex', gap: 12, alignItems: 'center' }}>
                <span style={{ color: 'var(--text-muted)', fontSize: 13, minWidth: 120 }}>Bot Token:</span>
                <code style={{ fontSize: 13 }}>{telegramCfg.bot_token_masked}</code>
              </div>
              <div style={{ display: 'flex', gap: 12, alignItems: 'center' }}>
                <span style={{ color: 'var(--text-muted)', fontSize: 13, minWidth: 120 }}>Chat ID:</span>
                <code style={{ fontSize: 13 }}>{telegramCfg.default_chat_id || '(chưa đặt)'}</code>
              </div>
              <div style={{ display: 'flex', gap: 12, alignItems: 'center' }}>
                <span style={{ color: 'var(--text-muted)', fontSize: 13, minWidth: 120 }}>Rate limit:</span>
                <span style={{ fontSize: 13 }}>{telegramCfg.rate_limit_per_minute} msg/phút</span>
              </div>
            </div>
          )}

          <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
            <button
              className="btn btn-primary"
              disabled={testMutation.isPending || !telegramCfg?.enabled}
              onClick={() => testMutation.mutate()}
            >
              {testMutation.isPending
                ? <><Loader size={16} /> Đang gửi...</>
                : <><Send size={16} /> Gửi test message</>}
            </button>

            {testMutation.isSuccess && (
              <span style={{ color: 'var(--color-success)', display: 'flex', alignItems: 'center', gap: 6, fontSize: 13 }}>
                <CheckCircle size={16} /> Đã gửi thành công
              </span>
            )}
            {testMutation.isError && (
              <span style={{ color: 'var(--color-danger)', display: 'flex', alignItems: 'center', gap: 6, fontSize: 13 }}>
                <AlertCircle size={16} />
                {(testMutation.error as any)?.response?.data?.message || 'Gửi thất bại'}
              </span>
            )}
          </div>

          {!telegramCfg?.enabled && !isLoading && (
            <p style={{ marginTop: 12, fontSize: 12, color: 'var(--color-warning)' }}>
              ⚠ Telegram chưa được bật. Đặt <code>FIRE_DETECT__TELEGRAM__ENABLED=true</code> trong .env rồi restart API.
            </p>
          )}
        </div>

        {/* Inference info */}
        <div className="card" style={{ maxWidth: 600, marginTop: 24 }}>
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
                ['HLS stream', 'detector:51051'],
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
