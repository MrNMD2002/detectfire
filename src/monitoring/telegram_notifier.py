"""
Telegram alert notifier for fire/smoke detection events.

Sends a photo with bounding boxes + caption to a Telegram chat
when fire or smoke is detected.

Config (env vars — set in .env):
    TELEGRAM_BOT_TOKEN  — Bot API token from @BotFather
    TELEGRAM_CHAT_ID    — Target chat ID
    telegram_cooldown_sec — Min seconds between alerts per class (default 60)
"""
from __future__ import annotations

import os
import time
from typing import Optional

import cv2
import httpx
import numpy as np

from src.core.logger import get_logger

logger = get_logger()

_API_BASE = "https://api.telegram.org/bot{token}/{method}"

# Per-class cooldown tracker (shared across all camera streams)
_last_sent: dict[str, float] = {}


class TelegramNotifier:
    """
    Thread-safe Telegram alert sender.
    Instantiate once in StreamManager, passed to each CameraStream.
    """

    def __init__(
        self,
        bot_token: Optional[str] = None,
        chat_id: Optional[str] = None,
        cooldown_sec: int = 60,
    ) -> None:
        self.bot_token    = bot_token or os.environ.get("TELEGRAM_BOT_TOKEN", "")
        self.chat_id      = chat_id   or os.environ.get("TELEGRAM_CHAT_ID", "")
        self.cooldown_sec = cooldown_sec
        self.enabled      = bool(self.bot_token and self.chat_id)

        if self.enabled:
            logger.info(f"[Telegram] Notifier enabled → chat_id={self.chat_id}")
        else:
            logger.info("[Telegram] Notifier disabled — set TELEGRAM_BOT_TOKEN + TELEGRAM_CHAT_ID in .env")

    # ── Public API ────────────────────────────────────────────────────────

    def send_alert(
        self,
        class_name:  str,
        confidence:  float,
        camera_name: str,
        frame:       Optional[np.ndarray] = None,  # annotated BGR frame
    ) -> None:
        """
        Send a fire/smoke alert with optional annotated photo.
        Respects per-class cooldown to avoid spam.
        Safe to call from any thread.
        """
        if not self.enabled:
            return

        now  = time.monotonic()
        last = _last_sent.get(class_name, 0.0)
        if now - last < self.cooldown_sec:
            return  # still in cooldown

        _last_sent[class_name] = now

        emoji   = "🔥" if class_name == "fire" else "🌫️"
        caption = (
            f"{emoji} *CẢNH BÁO PHÁT HIỆN {class_name.upper()}*\n\n"
            f"📷 Camera: `{camera_name}`\n"
            f"📊 Độ tin cậy: `{confidence:.0%}`\n"
            f"⏰ Thời gian: `{time.strftime('%Y-%m-%d %H:%M:%S')}`\n\n"
            f"_Hệ thống Fire Detection tự động_"
        )

        if frame is not None:
            self._send_photo(caption, frame)
        else:
            self._send_message(caption)

    # ── Internal helpers ──────────────────────────────────────────────────

    def _send_photo(self, caption: str, frame: np.ndarray) -> None:
        """Encode frame as JPEG and send via sendPhoto API."""
        try:
            ok, buf = cv2.imencode(".jpg", frame, [cv2.IMWRITE_JPEG_QUALITY, 85])
            if not ok:
                logger.warning("[Telegram] Failed to encode frame — falling back to text")
                self._send_message(caption)
                return

            url  = _API_BASE.format(token=self.bot_token, method="sendPhoto")
            resp = httpx.post(
                url,
                data={
                    "chat_id":    self.chat_id,
                    "caption":    caption,
                    "parse_mode": "Markdown",
                },
                files={"photo": ("alert.jpg", buf.tobytes(), "image/jpeg")},
                timeout=10.0,
            )
            if resp.status_code == 200:
                logger.info(f"[Telegram] Photo alert sent → chat {self.chat_id}")
            else:
                logger.warning(f"[Telegram] sendPhoto failed: {resp.status_code} {resp.text[:120]}")

        except Exception as exc:
            logger.warning(f"[Telegram] sendPhoto error: {exc}")

    def _send_message(self, text: str) -> None:
        """Fallback text-only message."""
        try:
            url  = _API_BASE.format(token=self.bot_token, method="sendMessage")
            resp = httpx.post(
                url,
                json={
                    "chat_id":    self.chat_id,
                    "text":       text,
                    "parse_mode": "Markdown",
                },
                timeout=5.0,
            )
            if resp.status_code != 200:
                logger.warning(f"[Telegram] sendMessage failed: {resp.status_code} {resp.text[:120]}")
        except Exception as exc:
            logger.warning(f"[Telegram] sendMessage error: {exc}")
