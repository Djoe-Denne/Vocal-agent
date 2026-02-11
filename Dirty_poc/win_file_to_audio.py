import os
import time
import glob
import traceback

# pip install simpleaudio
import simpleaudio as sa

WATCH_DIR = os.environ.get("AUDIO_DROP_DIR", r"C:\temp\openclaw-audio")
POLL_MS = int(os.environ.get("AUDIO_POLL_MS", "200"))

def wait_until_stable(path: str, checks: int = 3, delay: float = 0.08) -> None:
    """
    Avoid playing while the file is still being written.
    """
    last = -1
    stable = 0
    while stable < checks:
        try:
            size = os.path.getsize(path)
        except FileNotFoundError:
            return
        if size == last:
            stable += 1
        else:
            stable = 0
            last = size
        time.sleep(delay)

def play_and_delete(path: str) -> None:
    wait_until_stable(path)
    try:
        wave = sa.WaveObject.from_wave_file(path)
        play = wave.play()
        play.wait_done()
    finally:
        try:
            os.remove(path)
        except FileNotFoundError:
            pass

def main():
    os.makedirs(WATCH_DIR, exist_ok=True)
    print(f"[WIN] Watching: {WATCH_DIR}")
    while True:
        try:
            files = sorted(glob.glob(os.path.join(WATCH_DIR, "*.wav")))
            if not files:
                time.sleep(POLL_MS / 1000.0)
                continue

            # Play oldest first
            path = files[0]
            print(f"[WIN] Playing: {os.path.basename(path)}")
            play_and_delete(path)
        except KeyboardInterrupt:
            print("\n[WIN] Stopped.")
            return
        except Exception:
            traceback.print_exc()
            time.sleep(0.5)

if __name__ == "__main__":
    main()
