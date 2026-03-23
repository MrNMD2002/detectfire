"""
StreamManager — low-latency multi-camera stream processor for fire detection.

Low-latency design:
  latency_ms  = 50        → FFmpeg fflags=nobuffer + rtsp_transport=tcp
  max_buffers = 2         → FrameGrabber keeps only latest frame (drop_frames=true)
  drop_frames = true      → FrameGrabber drops old frames, processing thread gets latest
  max_fps     = 10-15     → rate limiting in processing thread

Key types:
  PluginFrame     → frame data + metadata broadcast to subscribers
  StreamStats     → total_frames, dropped_frames, reconnect_count, using_gpu, fps
  ReconnectConfig → delay_ms, max_attempts, backoff

Each CameraStream runs in its own thread:
  OpenCV capture → YOLO detect → annotate → broadcast PluginFrame to subscribers

Broadcast channel: asyncio.Queue per subscriber
  - maxsize=4: drop oldest frame if subscriber too slow
  - call_soon_threadsafe: thread→async bridge
"""
from __future__ import annotations

import asyncio
import base64
import dataclasses
import json
import threading
import time
from pathlib import Path
from typing import Any

import cv2
import numpy as np

from src.api.detector import FireDetector
from src.core.logger import get_logger

logger = get_logger()

# ── FFmpeg low-latency RTSP options (mirrors GStreamer rtspsrc latency=50) ──
# rtsp_transport=tcp: reliable, prevents UDP packet loss
# fflags=nobuffer:    disable input buffering
# flags=low_delay:    low-latency decode
# framedrop=1:        drop late frames (mirrors drop-on-latency=true)
_FFMPEG_RTSP_OPTIONS = (
    "rtsp_transport;tcp"
    "|fflags;nobuffer"
    "|flags;low_delay"
    "|framedrop;1"
    "|max_delay;0"
    "|stimeout;5000000"   # 5 s socket timeout
)

# ── BGR colours for annotation (fire=red-orange, smoke=gray) ──────────────
_COLORS = {
    "fire":  (30,  60,  230),
    "smoke": (150, 150, 150),
}


# ══════════════════════════════════════════════════════════════════════════
#  Data types
# ══════════════════════════════════════════════════════════════════════════

@dataclasses.dataclass
class PluginFrame:
    """
    Frame data + detection metadata broadcast to every WebSocket subscriber.
    """
    type:        str            # always "frame"
    camera_id:   str
    timestamp_ms: int           # epoch ms  (buffer.pts().mseconds() equivalent)
    width:       int
    height:      int
    frame_b64:   str            # JPEG encoded, base64
    detections:  list[dict]     # [{"class", "confidence", "bbox"}]
    fps:         float
    has_fire:    bool
    has_smoke:   bool

    def to_dict(self) -> dict:
        return {
            "type":         self.type,
            "camera_id":    self.camera_id,
            "timestamp_ms": self.timestamp_ms,
            "width":        self.width,
            "height":       self.height,
            "frame":        self.frame_b64,
            "detections":   self.detections,
            "fps":          round(self.fps, 1),
            "has_fire":     self.has_fire,
            "has_smoke":    self.has_smoke,
        }


@dataclasses.dataclass
class StreamStats:
    """
    Operational metrics per camera stream.
    """
    total_frames:    int  = 0
    dropped_frames:  int  = 0
    reconnect_count: int  = 0
    using_gpu:       bool = False   # True if CUDA decode available (future)
    decoder_type:    str  = "opencv"
    stream_started:  bool = False
    frame_width:     int  = 0
    frame_height:    int  = 0
    fps:             float = 0.0


@dataclasses.dataclass
class ReconnectConfig:
    """
    Exponential backoff reconnect strategy for dropped streams.
    """
    delay_ms:     int = 2_000   # base delay
    max_attempts: int = 0       # 0 = unlimited
    backoff:      float = 1.5   # multiply delay per attempt
    max_delay_ms: int = 30_000

    def delay_for_attempt(self, attempt: int) -> float:
        """Return sleep seconds for attempt N (exponential backoff)."""
        ms = min(self.delay_ms * (self.backoff ** attempt), self.max_delay_ms)
        return ms / 1000.0


# ══════════════════════════════════════════════════════════════════════════
#  Annotation helper
# ══════════════════════════════════════════════════════════════════════════

def _annotate(frame: np.ndarray, detections: list[dict]) -> np.ndarray:
    out = frame.copy()
    for det in detections:
        x1, y1, x2, y2 = det["bbox"]
        cls   = det["class"]
        conf  = det["confidence"]
        color = _COLORS.get(cls, (200, 200, 200))
        label = f"{cls} {conf:.2f}"

        cv2.rectangle(out, (x1, y1), (x2, y2), color, 2)
        (tw, th), _ = cv2.getTextSize(label, cv2.FONT_HERSHEY_SIMPLEX, 0.55, 1)
        cv2.rectangle(out, (x1, y1 - th - 10), (x1 + tw + 4, y1), color, -1)
        cv2.putText(out, label, (x1 + 2, y1 - 4),
                    cv2.FONT_HERSHEY_SIMPLEX, 0.55, (255, 255, 255), 1, cv2.LINE_AA)
    return out


# ══════════════════════════════════════════════════════════════════════════
#  FrameGrabber  (drop=true, max-buffers=2 design)
# ══════════════════════════════════════════════════════════════════════════

class _FrameGrabber:
    """
    Runs in a dedicated thread, continuously calling cap.read() at full speed
    to drain the OS/FFmpeg frame buffer.  Keeps only the LATEST frame.

      - Producer (grab thread): reads every frame, stores latest → zero buffer lag
      - Consumer (processing thread): retrieves latest when ready → no stale frames

    This eliminates the ~500ms–2s delay from FFmpeg's default ring buffer.
    """

    def __init__(self, cap: cv2.VideoCapture, stop_event: threading.Event) -> None:
        self._cap   = cap
        self._stop  = stop_event
        self._frame: np.ndarray | None = None
        self._lock  = threading.Lock()
        self._ready = threading.Event()   # signals "new frame available"
        self.failed = False

        self._thread = threading.Thread(target=self._grab_loop, daemon=True, name="grabber")
        self._thread.start()

    def _grab_loop(self) -> None:
        """Drain buffer continuously — keep only latest frame (drop old ones)."""
        while not self._stop.is_set():
            ret, frame = self._cap.read()
            if not ret:
                self.failed = True
                self._ready.set()   # unblock any waiting get_latest()
                return
            with self._lock:
                self._frame = frame  # overwrite → only latest survives
            self._ready.set()
        self._ready.set()  # unblock get_latest() immediately when stop is requested

    def get_latest(self, timeout: float = 2.0) -> tuple[bool, np.ndarray | None]:
        """
        Block until a new frame is available (or timeout).
        Returns (success, frame_copy) — non-stale, always latest.
        """
        got = self._ready.wait(timeout=timeout)
        self._ready.clear()
        if not got or self.failed:
            return False, None
        with self._lock:
            if self._frame is None:
                return False, None
            return True, self._frame.copy()


# ══════════════════════════════════════════════════════════════════════════
#  CameraStream
# ══════════════════════════════════════════════════════════════════════════

class CameraStream:
    """
    Single-camera stream with AI detection and WebSocket broadcast.

    Lifecycle:
        stream = CameraStream(id, name, source, ...)
        stream.start(loop)          # spawns capture thread
        rx = stream.subscribe()     # get asyncio.Queue (broadcast receiver)
        stream.unsubscribe(rx)
        stream.stop()               # signal thread, join

    Threading model (decoupled AI consumer pattern):
        _FrameGrabber thread  → drain cap at full speed, keep latest frame
        Broadcast thread      → get latest frame → encode JPEG → broadcast
                                (does NOT block on YOLO — reads cached detections)
        YOLO worker thread    → receives latest frame, runs inference, stores result
    """

    def __init__(
        self,
        camera_id:  str,
        name:       str,
        source:     str | int,     # RTSP URL or webcam index
        detector:   FireDetector,
        config:     dict,
        reconnect:  ReconnectConfig | None = None,
    ) -> None:
        self.id     = camera_id
        self.name   = name
        self.source = source

        self._detector  = detector
        self._config    = config
        self._reconnect = reconnect or ReconnectConfig()

        # State (AtomicBool equivalent)
        self._stop_event  = threading.Event()
        self._thread: threading.Thread | None = None
        self._loop:   asyncio.AbstractEventLoop | None = None
        self.is_running = False
        self.error: str | None = None

        # Broadcast channel  (broadcast::Sender equivalent)
        self._subscribers: list[asyncio.Queue] = []
        self._sub_lock     = threading.Lock()

        # Last status — sent immediately to new subscribers (like a replay buffer of 1)
        self._last_status: dict | None = None

        # Stats
        self._stats = StreamStats()

        # ── YOLO async state ────────────────────────────────────────────────
        # Latest detections cached by YOLO worker; broadcast thread reads this
        # without blocking — detection may lag by 1 YOLO inference cycle (~50-300ms)
        # but the VIDEO FEED is always live at target_fps.
        self._latest_detections: list[dict] = []
        self._det_lock = threading.Lock()
        # Frame slot for YOLO worker (latest-only, overwrite if worker is busy)
        self._pending_yolo_frame: np.ndarray | None = None
        self._pending_yolo_lock  = threading.Lock()
        self._pending_yolo_ready = threading.Event()
        # YOLO worker thread lifecycle
        self._yolo_stop   = threading.Event()
        self._yolo_thread: threading.Thread | None = None

        # Alert cooldown tracking (per class) for ALERTS_TOTAL metric
        self._last_alert_time: dict[str, float] = {}

    # ── Lifecycle ──────────────────────────────────────────────────────────

    def start(self, loop: asyncio.AbstractEventLoop) -> None:
        """Start the capture thread."""
        self._loop = loop
        self._stop_event.clear()
        self._yolo_stop.clear()

        # Start YOLO worker (decoupled from broadcast thread)
        self._yolo_thread = threading.Thread(
            target=self._yolo_worker,
            args=(self._yolo_stop,),
            daemon=True,
            name=f"yolo-{self.id}",
        )
        self._yolo_thread.start()

        self._thread = threading.Thread(
            target=self._run_stream_loop,
            daemon=True,
            name=f"cam-{self.id}",
        )
        self._thread.start()
        self.is_running = True
        logger.info(f"[CameraStream:{self.id}] Started — source={self.source!r}")

    def stop(self) -> None:
        """Signal thread to stop and join."""
        self._stop_event.set()
        if self._thread and self._thread.is_alive():
            self._thread.join(timeout=5.0)

        # Stop YOLO worker — unblock its wait then join
        self._yolo_stop.set()
        self._pending_yolo_ready.set()   # unblock waiting _yolo_worker
        if self._yolo_thread and self._yolo_thread.is_alive():
            self._yolo_thread.join(timeout=5.0)

        self.is_running = False
        logger.info(f"[CameraStream:{self.id}] Stopped")

    # ── Subscriber management ──────────────────────────────────────────────

    def subscribe(self) -> asyncio.Queue:
        """Return a new broadcast receiver queue.
        Immediately replays the last status so new subscribers don't miss early errors.
        """
        q: asyncio.Queue = asyncio.Queue(maxsize=4)
        with self._sub_lock:
            self._subscribers.append(q)
            # Replay last known status (error/connecting) to new subscriber
            if self._last_status and self._loop:
                try:
                    self._loop.call_soon_threadsafe(q.put_nowait, self._last_status)
                except Exception:
                    pass
        return q

    def unsubscribe(self, q: asyncio.Queue) -> None:
        """Remove a subscriber queue."""
        with self._sub_lock:
            if q in self._subscribers:
                self._subscribers.remove(q)

    def stats(self) -> StreamStats:
        """Returns a snapshot of current stream statistics."""
        return dataclasses.replace(self._stats)

    # ── YOLO worker ───────────────────────────────────────────────────────

    def _yolo_worker(self, stop: threading.Event) -> None:
        """
        Dedicated YOLO inference thread — fully decoupled from the broadcast thread.

          - Receives latest raw frame via _pending_yolo_frame slot (overwrite if busy)
          - Runs inference at its own rate (no blocking of video broadcast)
          - Stores result in _latest_detections for broadcast thread to read async

        The broadcast thread reads _latest_detections without waiting for YOLO,
        so video latency = encode_time (~10ms) instead of YOLO_time + encode_time.
        """
        while not stop.is_set():
            if not self._pending_yolo_ready.wait(timeout=0.5):
                continue
            self._pending_yolo_ready.clear()

            with self._pending_yolo_lock:
                frame = self._pending_yolo_frame

            if frame is None:
                continue

            try:
                detections = self._detector.predict(frame, camera_id=self.id)
                with self._det_lock:
                    self._latest_detections = detections
            except Exception as exc:
                logger.error(f"[CameraStream:{self.id}] YOLO worker error: {exc}")

    # ── Capture loop  (mirrors run_plugin_stream_loop) ────────────────────

    def _broadcast_status(self, status: str, message: str = "") -> None:
        """Send a status/error event to all subscribers and store as last_status."""
        if not self._loop:
            return
        data = {"type": "status", "camera_id": self.id, "status": status, "message": message}
        with self._sub_lock:
            self._last_status = data
            subs = list(self._subscribers)
        for q in subs:
            try:
                self._loop.call_soon_threadsafe(q.put_nowait, data)
            except Exception:
                pass

    def _run_stream_loop(self) -> None:
        """
        Outer reconnect loop.
        Mirrors run_plugin_stream_loop(): retries run_plugin_pipeline() on error.
        """
        attempt = 0
        while not self._stop_event.is_set():
            self._broadcast_status("connecting", f"Attempt {attempt + 1}…")
            err = self._run_pipeline()

            if self._stop_event.is_set():
                break

            if err is None:
                break   # clean exit

            # Check max attempts
            rc = self._reconnect
            if rc.max_attempts > 0 and attempt >= rc.max_attempts:
                logger.error(f"[CameraStream:{self.id}] Max reconnect attempts reached")
                self._stats.reconnect_count = attempt
                self.error = f"Max reconnect attempts ({rc.max_attempts}) exceeded"
                self._broadcast_status("error", self.error)
                break

            delay = rc.delay_for_attempt(attempt)
            attempt += 1
            self._stats.reconnect_count = attempt
            self.error = err
            logger.warning(
                f"[CameraStream:{self.id}] Error: {err}  "
                f"reconnecting in {delay:.1f}s (attempt {attempt})"
            )
            self._broadcast_status("reconnecting", f"{err} — retry in {delay:.0f}s")

            # Interruptible sleep (100 ms slices — mirrors stop-check loop in Rust)
            deadline = time.monotonic() + delay
            while time.monotonic() < deadline:
                if self._stop_event.is_set():
                    return
                time.sleep(0.1)

    def _run_pipeline(self) -> str | None:
        """
        Single pipeline attempt.
        Mirrors run_plugin_pipeline(): open cap → pull frames → broadcast.
        Returns None on clean exit, error string on failure.
        """
        target_fps   = int(self._config.get("target_fps", 15))
        frame_interval = 1.0 / target_fps
        jpeg_quality = int(self._config.get("jpeg_quality", 75))
        w = int(self._config.get("frame_width",  640))
        h = int(self._config.get("frame_height", 480))

        cap = self._open_cap(w, h)
        if cap is None:
            return f"Cannot open source: {self.source!r}"

        self.error = None
        self._stats.stream_started = True
        self._stats.frame_width    = w
        self._stats.frame_height   = h

        fps_t0    = time.monotonic()
        fps_count = 0

        # Start FrameGrabber — drains buffer at full speed, keeps only latest frame
        grabber = _FrameGrabber(cap, self._stop_event)

        try:
            while not self._stop_event.is_set():
                t0 = time.monotonic()

                # Get latest frame (non-stale — FrameGrabber has drained the buffer)
                ret, frame = grabber.get_latest(timeout=2.0)
                if not ret:
                    return "Frame read failed (EOS / connection dropped)"

                # Resize
                frame = cv2.resize(frame, (w, h))

                # Submit frame to YOLO worker (non-blocking, overwrite if busy)
                with self._pending_yolo_lock:
                    self._pending_yolo_frame = frame
                self._pending_yolo_ready.set()

                # Read cached detections from last YOLO cycle (may lag ~1 inference behind)
                with self._det_lock:
                    detections = list(self._latest_detections)

                # Annotate with cached detections
                annotated = _annotate(frame, detections)

                # Encode JPEG
                ok, buf = cv2.imencode(
                    ".jpg", annotated, [cv2.IMWRITE_JPEG_QUALITY, jpeg_quality]
                )
                if not ok:
                    continue

                # Build PluginFrame
                fps_count += 1
                elapsed = time.monotonic() - fps_t0
                if elapsed >= 1.0:
                    self._stats.fps = fps_count / elapsed
                    fps_t0   = time.monotonic()
                    fps_count = 0

                self._stats.total_frames += 1

                has_fire  = any(d["class"] == "fire"  for d in detections)
                has_smoke = any(d["class"] == "smoke" for d in detections)

                pf = PluginFrame(
                    type         = "frame",
                    camera_id    = self.id,
                    timestamp_ms = int(time.time() * 1000),
                    width        = w,
                    height       = h,
                    frame_b64    = base64.b64encode(buf.tobytes()).decode(),
                    detections   = detections,
                    fps          = self._stats.fps,
                    has_fire     = has_fire,
                    has_smoke    = has_smoke,
                )

                # Track alert events (with cooldown to avoid counting every frame)
                try:
                    from src.monitoring.metrics import ALERTS_TOTAL
                    _now = time.monotonic()
                    _cooldown = float(self._config.get("alert_cooldown_sec", 3))
                    for cls_name, triggered in (("fire", has_fire), ("smoke", has_smoke)):
                        if triggered:
                            _last = self._last_alert_time.get(cls_name, 0.0)
                            if _now - _last >= _cooldown:
                                ALERTS_TOTAL.labels(class_name=cls_name, camera_id=self.id).inc()
                                self._last_alert_time[cls_name] = _now
                except ImportError:
                    pass

                self._broadcast(pf)

                # Rate limiting  (backpressure — mirrors appsink drop: true)
                took = time.monotonic() - t0
                if took < frame_interval:
                    time.sleep(frame_interval - took)

        finally:
            cap.release()

        return None  # clean stop

    def _open_cap(self, w: int, h: int) -> cv2.VideoCapture | None:
        logger.info(f"[CameraStream:{self.id}] Opening {self.source!r}")
        is_rtsp = isinstance(self.source, str) and self.source.lower().startswith("rtsp")

        if is_rtsp:
            import os
            os.environ["OPENCV_FFMPEG_CAPTURE_OPTIONS"] = _FFMPEG_RTSP_OPTIONS
            # CAP_PROP_OPEN_TIMEOUT_MSEC must be passed via params to open(), not set()
            # — it controls OpenCV's own interrupt callback (default 30 s)
            cap = cv2.VideoCapture()
            try:
                open_params = [
                    int(cv2.CAP_PROP_OPEN_TIMEOUT_MSEC), 8_000,
                    int(cv2.CAP_PROP_READ_TIMEOUT_MSEC), 5_000,
                ]
                cap.open(self.source, cv2.CAP_FFMPEG, open_params)
            except (TypeError, cv2.error):
                # Older OpenCV — params overload not supported
                cap.open(self.source, cv2.CAP_FFMPEG)
            cap.set(cv2.CAP_PROP_BUFFERSIZE, 1)
        else:
            cap = cv2.VideoCapture(self.source)
            cap.set(cv2.CAP_PROP_FRAME_WIDTH, w)
            cap.set(cv2.CAP_PROP_FRAME_HEIGHT, h)
            cap.set(cv2.CAP_PROP_BUFFERSIZE, 1)

        if self._stop_event.is_set():
            cap.release()
            return None

        if not cap.isOpened():
            self.error = f"Cannot open: {self.source!r}"
            logger.error(f"[CameraStream:{self.id}] {self.error}")
            self._broadcast_status("error", self.error)
            return None

        # Test-read: retry a few times — Dahua/RTSP cameras need time to negotiate
        # before the first frame arrives. Check stop_event each iteration.
        ret = False
        for _ in range(15):
            if self._stop_event.is_set():
                cap.release()
                return None
            ret, _ = cap.read()
            if ret:
                break
            time.sleep(0.2)
        if not ret:
            cap.release()
            self.error = f"Source opened but no frame received: {self.source!r}"
            logger.error(f"[CameraStream:{self.id}] {self.error}")
            self._broadcast_status("error", self.error)
            return None

        # Rewind for webcam (RTSP doesn't support set)
        if not isinstance(self.source, str):
            cap.set(cv2.CAP_PROP_POS_FRAMES, 0)

        self._broadcast_status("connected", "Stream live")
        return cap

    def _broadcast(self, frame: PluginFrame) -> None:
        """
        Push PluginFrame to all subscribers.
        Mirrors broadcast::Sender::send() — drops oldest if receiver queue full (Lagged).
        Uses call_soon_threadsafe as the thread→asyncio bridge.

        NOTE: QueueFull must be handled INSIDE the event-loop callback because
        call_soon_threadsafe only schedules the call — exceptions raised by put_nowait
        propagate inside the event loop, not in this thread.
        """
        if not self._loop:
            return
        data = frame.to_dict()
        stats = self._stats

        def _put(q: asyncio.Queue) -> None:
            try:
                q.put_nowait(data)
            except asyncio.QueueFull:
                stats.dropped_frames += 1
                # Drain oldest, push latest (mirrors Lagged handling)
                try:
                    q.get_nowait()
                    q.put_nowait(data)
                except Exception:
                    pass

        with self._sub_lock:
            subs = list(self._subscribers)
        for q in subs:
            try:
                self._loop.call_soon_threadsafe(_put, q)
            except Exception:
                pass


# ══════════════════════════════════════════════════════════════════════════
#  StreamManager  (registry of CameraStream instances)
# ══════════════════════════════════════════════════════════════════════════

class StreamManager:
    """Manages a pool of CameraStream objects."""

    def __init__(self, detector: FireDetector, config: dict, persist_path: Path | None = None) -> None:
        self._detector = detector
        self._config   = config
        self._streams: dict[str, CameraStream] = {}
        self._loop: asyncio.AbstractEventLoop | None = None
        # Default: data/cameras.json relative to working directory
        self._persist_path: Path = persist_path or Path("data/cameras.json")

    def set_loop(self, loop: asyncio.AbstractEventLoop) -> None:
        self._loop = loop

    # ── Persistence ──────────────────────────────────────────────────────

    def _save(self) -> None:
        """Persist current camera list to JSON so they survive server/page restarts."""
        try:
            self._persist_path.parent.mkdir(parents=True, exist_ok=True)
            data = [
                {"id": s.id, "name": s.name, "source": s.source}
                for s in self._streams.values()
            ]
            self._persist_path.write_text(json.dumps(data, indent=2), encoding="utf-8")
        except Exception as exc:
            logger.warning(f"[Manager] Failed to save cameras: {exc}")

    def restore_cameras(self) -> None:
        """Load cameras.json on startup and reconnect all persisted cameras."""
        if not self._persist_path.exists():
            return
        try:
            data = json.loads(self._persist_path.read_text(encoding="utf-8"))
            for cam in data:
                source = cam["source"]
                # Webcam sources were saved as int → restore as int
                try:
                    source = int(source)
                except (ValueError, TypeError):
                    pass  # RTSP URL — keep as string
                self.add_stream(cam["id"], source, cam.get("name", cam["id"]), _persist=False)
            logger.info(f"[Manager] Restored {len(data)} camera(s) from {self._persist_path}")
        except Exception as exc:
            logger.warning(f"[Manager] Failed to restore cameras: {exc}")

    # ── Stream lifecycle ─────────────────────────────────────────────────

    def add_stream(
        self,
        camera_id: str,
        source:    str | int,
        name:      str,
        reconnect: ReconnectConfig | None = None,
        _persist:  bool = True,
    ) -> CameraStream:
        if camera_id in self._streams:
            self._streams[camera_id].stop()

        stream = CameraStream(
            camera_id = camera_id,
            name      = name,
            source    = source,
            detector  = self._detector,
            config    = self._config,
            reconnect = reconnect,
        )
        stream.start(self._loop)
        self._streams[camera_id] = stream
        if _persist:
            self._save()
        logger.info(f"[Manager] Added stream {camera_id!r} ({name})")
        return stream

    def remove_stream(self, camera_id: str) -> bool:
        stream = self._streams.pop(camera_id, None)
        if stream:
            stream.stop()
            self._save()
            return True
        return False

    def get_stream(self, camera_id: str) -> CameraStream | None:
        return self._streams.get(camera_id)

    def list_streams(self) -> list[dict[str, Any]]:
        result = []
        for s in self._streams.values():
            st = s.stats()
            result.append({
                "id":               s.id,
                "name":             s.name,
                "source":           str(s.source),
                "is_running":       s.is_running,
                "fps":              round(st.fps, 1),
                "total_frames":     st.total_frames,
                "dropped_frames":   st.dropped_frames,
                "reconnect_count":  st.reconnect_count,
                "using_gpu":        st.using_gpu,
                "decoder_type":     st.decoder_type,
                "error":            s.error,
            })
        return result

    def shutdown(self) -> None:
        for s in list(self._streams.values()):
            s.stop()
        self._streams.clear()
        logger.info("[Manager] All streams stopped")
