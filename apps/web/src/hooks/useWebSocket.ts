import { useEffect, useRef, useCallback, useState } from 'react';
import { useAuthStore } from '@/stores/authStore';

interface WebSocketMessage {
  event_type: string;
  camera_id: string;
  site_id: string;
  timestamp: string;
  confidence: number;
  detections: any[];
  snapshot?: string;
}

interface UseWebSocketOptions {
  onMessage?: (message: WebSocketMessage) => void;
  onOpen?: () => void;
  onClose?: () => void;
  onError?: (error: Event) => void;
}

export function useWebSocket(options: UseWebSocketOptions = {}) {
  const [isConnected, setIsConnected] = useState(false);
  const wsRef = useRef<WebSocket | null>(null);
  const reconnectAttemptsRef = useRef(0);
  const reconnectTimeoutRef = useRef<number | null>(null);
  const destroyedRef = useRef(false);

  // Keep callbacks in refs to avoid stale closures in onopen/onclose/onmessage
  const onMessageRef = useRef(options.onMessage);
  const onOpenRef = useRef(options.onOpen);
  const onCloseRef = useRef(options.onClose);
  const onErrorRef = useRef(options.onError);
  onMessageRef.current = options.onMessage;
  onOpenRef.current = options.onOpen;
  onCloseRef.current = options.onClose;
  onErrorRef.current = options.onError;

  const connect = useCallback(() => {
    if (destroyedRef.current) return;
    if (wsRef.current?.readyState === WebSocket.OPEN) return;
    if (wsRef.current?.readyState === WebSocket.CONNECTING) return;

    const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    const token = useAuthStore.getState().token;
    const tokenParam = token ? `?token=${encodeURIComponent(token)}` : '';
    const wsUrl = `${protocol}//${window.location.host}/ws/events${tokenParam}`;

    wsRef.current = new WebSocket(wsUrl);

    wsRef.current.onopen = () => {
      if (destroyedRef.current) { wsRef.current?.close(); return; }
      console.log('WebSocket connected');
      reconnectAttemptsRef.current = 0;
      setIsConnected(true);
      onOpenRef.current?.();
    };

    wsRef.current.onmessage = (event) => {
      try {
        const message = JSON.parse(event.data) as WebSocketMessage;
        onMessageRef.current?.(message);
      } catch (e) {
        console.error('Failed to parse WebSocket message:', e);
      }
    };

    wsRef.current.onclose = () => {
      console.log('WebSocket disconnected');
      setIsConnected(false);
      onCloseRef.current?.();

      if (!destroyedRef.current) {
        // Exponential backoff: 1s, 2s, 4s, 8s, 16s, capped at 30s
        const delay = Math.min(30000, 1000 * Math.pow(2, reconnectAttemptsRef.current));
        reconnectAttemptsRef.current++;
        console.log(`WebSocket reconnecting in ${delay}ms (attempt ${reconnectAttemptsRef.current})`);
        reconnectTimeoutRef.current = window.setTimeout(connect, delay);
      }
    };

    wsRef.current.onerror = (error) => {
      console.error('WebSocket error:', error);
      onErrorRef.current?.(error);
    };
  }, []); // stable — uses refs for all callbacks

  const disconnect = useCallback(() => {
    destroyedRef.current = true;
    if (reconnectTimeoutRef.current !== null) {
      clearTimeout(reconnectTimeoutRef.current);
      reconnectTimeoutRef.current = null;
    }
    wsRef.current?.close();
  }, []);

  useEffect(() => {
    destroyedRef.current = false;
    connect();
    return () => {
      destroyedRef.current = true;
      if (reconnectTimeoutRef.current !== null) clearTimeout(reconnectTimeoutRef.current);
      wsRef.current?.close();
    };
  }, []); // run once on mount

  return { connect, disconnect, isConnected };
}
