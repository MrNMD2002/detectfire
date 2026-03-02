import { useState, useEffect, useRef } from 'react';

interface LogMessage {
  id: string;
  type: 'info' | 'warning' | 'error' | 'success' | 'dim';
  text: string;
}

const FAKE_LOGS = [
  "Initialize core neural network... OK",
  "Loading ONNX TensorRT context... OK",
  "Allocating 8192MB VRAM... SUCCESS",
  "RTSP Stream CAM-01 established. Latency: 42ms",
  "Model inference optimized via CUDA v12.2",
  "Start frame pooling... 15 IN / 15 OUT",
  "Scanning vectors: [0.12, 0.45, 0.88, 0.02]",
  "Probability threshold calibrated at 0.65",
  "Awaiting anomalies..."
];

export function CyberTerminal({ activeEvents = 0 }: { activeEvents?: number }) {
  const [logs, setLogs] = useState<LogMessage[]>([]);
  const logsEndRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    // Generate initial logs
    const initialLogs: LogMessage[] = FAKE_LOGS.map((t, i) => ({
      id: `init-${i}`,
      type: t.includes('OK') || t.includes('SUCCESS') ? 'success' : 'dim',
      text: `[SYS] ${new Date().toISOString().split('T')[1].slice(0,-1)} - ${t}`
    }));
    setLogs(initialLogs);

    // Randomize logs interval
    const interval = setInterval(() => {
      const isAnim = Math.random() > 0.8;
      const memRnd = (Math.random() * 4 + 4).toFixed(2);
      const confRnd = (Math.random() * 0.9 + 0.1).toFixed(3);
      const fps = (Math.random() * 2 + 14).toFixed(1);

      let newLog: LogMessage;

      if (activeEvents > 0) {
        newLog = {
          id: Date.now().toString(),
          type: 'error',
          text: `[ALERT] ${new Date().toISOString().split('T')[1].slice(0,-1)} - ANOMALY DETECTED! CONF: ${confRnd} | TENSOR [##!!##]`
        };
      } else {
         newLog = {
          id: Date.now().toString(),
          type: isAnim ? 'info' : 'dim',
          text: `[INFER] ${new Date().toISOString().split('T')[1].slice(0,-1)} - Batch processed. GPU: ${memRnd}GB | FPS: ${fps}`
        };
      }

      setLogs(prev => [...prev.slice(-15), newLog]);
    }, activeEvents > 0 ? 800 : 2500);

    return () => clearInterval(interval);
  }, [activeEvents]);

  useEffect(() => {
    if (logsEndRef.current) {
      logsEndRef.current.scrollIntoView({ behavior: 'smooth' });
    }
  }, [logs]);

  return (
    <div className="cyber-terminal">
      <div className="cyber-terminal-lines">
        {logs.map(log => (
          <div key={log.id} className={`terminal-line ${log.type}`}>
            {log.text}
          </div>
        ))}
        <div ref={logsEndRef} />
      </div>
    </div>
  );
}
