import { useState } from 'react';
import { Save, CheckCircle, AlertCircle } from 'lucide-react';

interface TelegramSettings {
  botToken: string;
  chatId: string;
  rateLimit: number;
}

interface InferenceSettings {
  modelPath: string;
  device: string;
}

type SaveStatus = 'idle' | 'saving' | 'success' | 'error';

export default function SettingsPage() {
  const [telegram, setTelegram] = useState<TelegramSettings>({
    botToken: '',
    chatId: '',
    rateLimit: 10,
  });

  const [inference, setInference] = useState<InferenceSettings>({
    modelPath: 'models/best.onnx',
    device: 'cuda:0',
  });

  const [telegramStatus, setTelegramStatus] = useState<SaveStatus>('idle');
  const [inferenceStatus, setInferenceStatus] = useState<SaveStatus>('idle');
  const [errorMsg, setErrorMsg] = useState('');

  const saveTelegram = async (e: React.FormEvent) => {
    e.preventDefault();
    setTelegramStatus('saving');
    setErrorMsg('');

    try {
      // POST settings to the backend when a settings API endpoint is available.
      // Currently Telegram config is managed via environment variables
      // (FIRE_DETECT__TELEGRAM__BOT_TOKEN, FIRE_DETECT__TELEGRAM__DEFAULT_CHAT_ID).
      // This placeholder simulates the save and shows the correct guidance.
      await new Promise((resolve) => setTimeout(resolve, 500));

      console.info(
        'Telegram settings (apply via env vars in production):',
        { chatId: telegram.chatId, rateLimit: telegram.rateLimit }
        // NOTE: never log the bot token
      );

      setTelegramStatus('success');
      setTimeout(() => setTelegramStatus('idle'), 3000);
    } catch (err: any) {
      setErrorMsg(err?.message ?? 'Lưu thất bại');
      setTelegramStatus('error');
    }
  };

  const saveInference = async (e: React.FormEvent) => {
    e.preventDefault();
    setInferenceStatus('saving');
    setErrorMsg('');

    try {
      // Inference settings are currently managed via configs/settings.yaml.
      // When a settings CRUD API is added, replace this with a real API call.
      await new Promise((resolve) => setTimeout(resolve, 500));

      setInferenceStatus('success');
      setTimeout(() => setInferenceStatus('idle'), 3000);
    } catch (err: any) {
      setErrorMsg(err?.message ?? 'Lưu thất bại');
      setInferenceStatus('error');
    }
  };

  return (
    <>
      <header className="page-header">
        <h1 className="page-title">Cài đặt</h1>
        <p className="page-subtitle">Cấu hình hệ thống phát hiện cháy khói</p>
      </header>

      <div className="page-content">
        {/* Telegram settings */}
        <form className="card" style={{ maxWidth: 600 }} onSubmit={saveTelegram}>
          <h3 style={{ marginBottom: 24 }}>Thông báo Telegram</h3>

          <p style={{ color: 'var(--text-muted)', marginBottom: 16, fontSize: 13 }}>
            Để áp dụng trong môi trường production, đặt các biến môi trường:{' '}
            <code>FIRE_DETECT__TELEGRAM__BOT_TOKEN</code>,{' '}
            <code>FIRE_DETECT__TELEGRAM__DEFAULT_CHAT_ID</code>.
          </p>

          <div className="form-group">
            <label className="form-label">Bot Token</label>
            <input
              className="form-input"
              type="password"
              placeholder="••••••••"
              value={telegram.botToken}
              onChange={(e) => setTelegram({ ...telegram, botToken: e.target.value })}
              autoComplete="new-password"
            />
          </div>

          <div className="form-group">
            <label className="form-label">Chat ID</label>
            <input
              className="form-input"
              placeholder="-1001234567890"
              value={telegram.chatId}
              onChange={(e) => setTelegram({ ...telegram, chatId: e.target.value })}
            />
          </div>

          <div className="form-group">
            <label className="form-label">Rate Limit (msg/phút)</label>
            <input
              className="form-input"
              type="number"
              min={1}
              max={60}
              value={telegram.rateLimit}
              onChange={(e) => setTelegram({ ...telegram, rateLimit: Number(e.target.value) })}
            />
          </div>

          <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
            <button
              type="submit"
              className="btn btn-primary"
              disabled={telegramStatus === 'saving'}
            >
              <Save size={18} />
              {telegramStatus === 'saving' ? 'Đang lưu…' : 'Lưu cài đặt'}
            </button>

            {telegramStatus === 'success' && (
              <span style={{ color: 'var(--color-success)', display: 'flex', alignItems: 'center', gap: 6 }}>
                <CheckCircle size={16} /> Đã lưu
              </span>
            )}
            {telegramStatus === 'error' && (
              <span style={{ color: 'var(--color-danger)', display: 'flex', alignItems: 'center', gap: 6 }}>
                <AlertCircle size={16} /> {errorMsg || 'Lỗi'}
              </span>
            )}
          </div>
        </form>

        {/* Inference settings */}
        <form
          className="card"
          style={{ maxWidth: 600, marginTop: 24 }}
          onSubmit={saveInference}
        >
          <h3 style={{ marginBottom: 24 }}>Inference Engine</h3>

          <div className="form-group">
            <label className="form-label">Model Path</label>
            <input
              className="form-input"
              value={inference.modelPath}
              onChange={(e) => setInference({ ...inference, modelPath: e.target.value })}
              placeholder="models/best.onnx"
            />
            <small style={{ color: 'var(--text-muted)', display: 'block', marginTop: 6 }}>
              Model YOLOv26 (best.onnx). Detector thực tế đọc từ configs/settings.yaml
              (Docker: /app/models/best.onnx).
            </small>
          </div>

          <div className="form-group">
            <label className="form-label">Device</label>
            <select
              className="form-input"
              value={inference.device}
              onChange={(e) => setInference({ ...inference, device: e.target.value })}
            >
              <option value="cuda:0">CUDA GPU 0</option>
              <option value="cpu">CPU</option>
            </select>
          </div>

          <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
            <button
              type="submit"
              className="btn btn-primary"
              disabled={inferenceStatus === 'saving'}
            >
              <Save size={18} />
              {inferenceStatus === 'saving' ? 'Đang lưu…' : 'Lưu cấu hình'}
            </button>

            {inferenceStatus === 'success' && (
              <span style={{ color: 'var(--color-success)', display: 'flex', alignItems: 'center', gap: 6 }}>
                <CheckCircle size={16} /> Đã lưu
              </span>
            )}
            {inferenceStatus === 'error' && (
              <span style={{ color: 'var(--color-danger)', display: 'flex', alignItems: 'center', gap: 6 }}>
                <AlertCircle size={16} /> {errorMsg || 'Lỗi'}
              </span>
            )}
          </div>
        </form>
      </div>
    </>
  );
}
