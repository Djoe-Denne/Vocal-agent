#!/usr/bin/env python3
import os
import json
import time
import wave
import uuid
from http.server import BaseHTTPRequestHandler, HTTPServer

# Folder shared with Windows (WSL sees it as /mnt/c/..., Windows as C:\...)
OUTPUT_DIR = os.environ.get("AUDIO_DROP_DIR", "/mnt/c/temp/openclaw-audio")
os.makedirs(OUTPUT_DIR, exist_ok=True)

def synthesize_wav(text: str, out_path: str) -> None:
    """
    MVP placeholder: generate 0.4s of silence WAV.
    Replace this with your real TTS invocation if/when available inside WSL.
    """
    sample_rate = 24000
    duration_s = 0.4
    nframes = int(sample_rate * duration_s)
    nchannels = 1
    sampwidth = 2  # 16-bit PCM

    with wave.open(out_path, "wb") as wf:
        wf.setnchannels(nchannels)
        wf.setsampwidth(sampwidth)
        wf.setframerate(sample_rate)
        wf.writeframes(b"\x00\x00" * nframes)

class Handler(BaseHTTPRequestHandler):
    def _send_json(self, code: int, payload: dict):
        body = json.dumps(payload).encode("utf-8")
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_POST(self):
        if self.path != "/v1/audio/speech":
            return self._send_json(404, {"ok": False, "error": "not_found"})

        try:
            length = int(self.headers.get("Content-Length", "0"))
            raw = self.rfile.read(length)
            data = json.loads(raw.decode("utf-8"))
            text = (data.get("input") or "").strip()
            if not text:
                return self._send_json(400, {"ok": False, "error": "empty_input"})

            req_id = uuid.uuid4().hex
            tmp_path = os.path.join(OUTPUT_DIR, f".{req_id}.wav.tmp")
            final_path = os.path.join(OUTPUT_DIR, f"{int(time.time()*1000)}_{req_id}.wav")

            synthesize_wav(text, tmp_path)
            os.replace(tmp_path, final_path)  # atomic on same filesystem

            return self._send_json(200, {"ok": True, "request_id": req_id, "file": os.path.basename(final_path)})
        except Exception as e:
            return self._send_json(500, {"ok": False, "error": f"server_error: {e}"})

def main():
    host = os.environ.get("WSL_HTTP_HOST", "127.0.0.1")
    port = int(os.environ.get("WSL_HTTP_PORT", "8009"))
    print(f"[WSL] Listening on http://{host}:{port}/v1/audio/speech")
    print(f"[WSL] Dropping wav files to: {OUTPUT_DIR}")
    HTTPServer((host, port), Handler).serve_forever()

if __name__ == "__main__":
    main()
