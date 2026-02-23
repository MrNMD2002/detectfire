import cv2
import time
import argparse
import requests
import json
import os
import threading
from datetime import datetime
from ultralytics import YOLO
import telebot  # pip install pyTelegramBotAPI
from queue import Queue

# Configuration (YOLOv26 SalahALHaismawi: 0=fire, 1=other, 2=smoke)
CONFIG = {
    "model_path": "models/best.pt",
    "rtsp_url": "rtsp://root:Admin!123@10.1.1.174:554/cam/realmonitor?channel=1&subtype=0",
    "conf_fire": 0.5,
    "conf_smoke": 0.4,
    "conf_other": 0.4,  # Fire-related indicators (class 2)
    "camera_id": "cam-01",
    "site_id": "site-main",
    "api_url": "http://localhost:8080/api/events",  # Push to local mock API or Rust API
    "telegram_token": "8507965917:AAFQ3E66xzT4dDo5GbKXc2p7rW3EbHk0CgU",
    "telegram_chat_id": "8507965917",  # User provided ID
    "snapshot_dir": "apps/web/public/snapshots",  # Save directly to web public folder
}

# Force FFmpeg to use TCP for RTSP (more stable)
os.environ["OPENCV_FFMPEG_CAPTURE_OPTIONS"] = "rtsp_transport;tcp"

# Ensure snapshot dir exists
os.makedirs(CONFIG["snapshot_dir"], exist_ok=True)

# Initialize Telegram
bot = telebot.TeleBot(CONFIG["telegram_token"])


def send_telegram_alert(image_path, text):
    try:
        # If chat_id not set, try to get updates
        if not CONFIG["telegram_chat_id"]:
            updates = bot.get_updates()
            if updates:
                CONFIG["telegram_chat_id"] = updates[-1].message.chat.id
                print(
                    f"✅ Auto-detected Telegram Chat ID: {CONFIG['telegram_chat_id']}"
                )
            else:
                print(
                    "⚠️ Telegram Chat ID not set and no updates found. Send a message to the bot first!"
                )
                return

        with open(image_path, "rb") as photo:
            bot.send_photo(CONFIG["telegram_chat_id"], photo, caption=text)
        print(f"🚀 Telegram alert sent: {text}")
    except Exception as e:
        print(f"❌ Telegram Error: {e}")


def post_to_api(event_data):
    try:
        # Mock API requires POST /events (we need to implement this in mock-api or use existing hack)
        # For now, just print
        print(
            f"📡 Posting event to API: {event_data['event_type']} ({event_data['confidence']:.2f})"
        )

        # If using real API, uncomment:
        # requests.post(CONFIG["api_url"], json=event_data)
    except Exception as e:
        print(f"❌ API Error: {e}")


def run_inference():
    print(f"🔄 Loading model: {CONFIG['model_path']}...")
    try:
        model = YOLO(CONFIG["model_path"], task="detect")
    except Exception as e:
        print(
            f"❌ Model load failed. Ensure ultralytics is installed and model path is correct. Error: {e}"
        )
        return

    print(f"🎥 Connecting to RTSP: {CONFIG['rtsp_url']}...")
    cap = cv2.VideoCapture(CONFIG["rtsp_url"])

    if not cap.isOpened():
        print("❌ Cannot open RTSP stream. Check URL and network.")
        return

    print("✅ System Ready! analyzing stream...")

    last_alert = 0
    cooldown = 10  # seconds

    while True:
        ret, frame = cap.read()
        if not ret:
            print("⚠️ Stream disconnected, reconnecting...")
            time.sleep(2)
            cap.release()
            cap = cv2.VideoCapture(CONFIG["rtsp_url"])
            continue

        # Skip frames to reduce load (process every 5th frame)
        # For demo, just run

        # Inference
        results = model(frame, verbose=False, conf=0.4)[0]

        detected = False
        event_type = ""
        max_conf = 0.0

        for box in results.boxes:
            cls_id = int(box.cls[0])
            conf = float(box.conf[0])
            label = model.names[cls_id]  # fire, smoke, or other (YOLOv26)

            above_thresh = (
                (label == "fire" and conf >= CONFIG["conf_fire"])
                or (label == "smoke" and conf >= CONFIG["conf_smoke"])
                or (label == "other" and conf >= CONFIG["conf_other"])
            )
            if above_thresh:
                detected = True
                event_type = label
                max_conf = conf

                x1, y1, x2, y2 = map(int, box.xyxy[0])
                color = (0, 0, 255) if label == "fire" else (128, 128, 128) if label == "smoke" else (0, 165, 255)  # fire=red, smoke=gray, other=orange
                cv2.rectangle(frame, (x1, y1), (x2, y2), color, 2)
                cv2.putText(
                    frame,
                    f"{label} {conf:.2f}",
                    (x1, y1 - 10),
                    cv2.FONT_HERSHEY_SIMPLEX,
                    0.5,
                    color,
                    2,
                )

        # Show preview window
        cv2.imshow("Fire & Smoke Detector", frame)

        # Alert logic
        current_time = time.time()
        if detected and (current_time - last_alert > cooldown):
            timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
            filename = f"event_{timestamp}.jpg"
            save_path = os.path.join(CONFIG["snapshot_dir"], filename)

            # Save snapshot
            cv2.imwrite(save_path, frame)
            print(f"📸 Snapshot saved: {save_path}")

            # Send Telegram
            msg = f"🚨 DETECTED: {event_type.upper()}!\nCONFIDENCE: {max_conf:.2f}\nCAMERA: {CONFIG['camera_id']}\nTIME: {datetime.now().strftime('%H:%M:%S')}"
            threading.Thread(target=send_telegram_alert, args=(save_path, msg)).start()

            # Post to API
            event_payload = {
                "event_type": event_type,
                "camera_id": CONFIG["camera_id"],
                "site_id": CONFIG["site_id"],
                "confidence": max_conf,
                "snapshot_path": filename,
                "timestamp": datetime.now().isoformat(),
            }
            threading.Thread(target=post_to_api, args=(event_payload,)).start()

            last_alert = current_time

        if cv2.waitKey(1) & 0xFF == ord("q"):
            break

    cap.release()
    cv2.destroyAllWindows()


if __name__ == "__main__":
    run_inference()
