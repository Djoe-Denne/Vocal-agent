"""
Pynput keyboard hotkey adapter.

Manages keyboard hotkeys using pynput for push-to-talk functionality.
"""

import threading
from typing import Callable, Optional

from pynput import keyboard

from ptt.utils.logging import get_logger


class HotkeyManager:
    """
    Manages keyboard hotkeys using pynput.

    Provides a clean interface for:
    - Registering hotkey combinations with callbacks
    - Handling key press/release events
    - Graceful shutdown on Ctrl+C or Escape
    """

    def __init__(self):
        self._log = get_logger("hotkeys")

        self._hotkeys: dict[frozenset, tuple[Callable, bool]] = {}
        self._current_keys: set[str] = set()
        self._keys_lock = threading.Lock()

        self._listener: Optional[keyboard.Listener] = None
        self._running = False

        self._on_shutdown: Optional[Callable[[], None]] = None

    def register(self, hotkey_str: str, callback: Callable[[], None]) -> None:
        keys = self._parse_hotkey(hotkey_str)
        self._hotkeys[keys] = (callback, False)
        self._log.debug(f"Registered hotkey: {hotkey_str} -> {keys}")

    def set_shutdown_callback(self, callback: Callable[[], None]) -> None:
        self._on_shutdown = callback

    def start(self) -> None:
        self._running = True
        self._log.info("Listening for hotkeys... (Escape or Ctrl+C to exit)")

        try:
            with keyboard.Listener(
                on_press=self._on_press,
                on_release=self._on_release,
                suppress=False,
            ) as listener:
                self._listener = listener
                listener.join()
        except KeyboardInterrupt:
            self._log.info("Keyboard interrupt received")
        finally:
            self._running = False
            self._log.info("Listener stopped")
            if self._on_shutdown:
                self._on_shutdown()

    def start_async(self) -> None:
        thread = threading.Thread(target=self.start, daemon=True)
        thread.start()

    def stop(self) -> None:
        self._running = False
        if self._listener:
            self._listener.stop()

    # -- internal helpers ------------------------------------------------------

    def _parse_hotkey(self, hotkey_str: str) -> frozenset:
        keys = set()
        parts = hotkey_str.lower().replace("<", "").replace(">", "").split("+")
        for part in parts:
            part = part.strip()
            if part in ("ctrl", "control"):
                keys.add("ctrl")
            elif part == "alt":
                keys.add("alt")
            elif part == "shift":
                keys.add("shift")
            elif part == "space":
                keys.add("space")
            else:
                keys.add(part)
        return frozenset(keys)

    def _get_key_name(self, key) -> Optional[str]:
        try:
            if hasattr(key, "char") and key.char:
                return key.char.lower()
            elif hasattr(key, "name"):
                name = key.name.lower()
                if name in ("ctrl_l", "ctrl_r"):
                    return "ctrl"
                elif name in ("alt_l", "alt_r", "alt_gr"):
                    return "alt"
                elif name in ("shift_l", "shift_r"):
                    return "shift"
                return name
        except AttributeError:
            pass
        return None

    def _on_press(self, key) -> Optional[bool]:
        key_name = self._get_key_name(key)
        if not key_name:
            return None

        with self._keys_lock:
            self._current_keys.add(key_name)

            if key_name == "esc":
                self._log.info("Escape pressed - shutting down...")
                return False

            if "ctrl" in self._current_keys and key_name == "c":
                self._log.info("Ctrl+C pressed - shutting down...")
                return False

            for hotkey_keys, (callback, triggered) in list(self._hotkeys.items()):
                if hotkey_keys.issubset(self._current_keys) and not triggered:
                    self._hotkeys[hotkey_keys] = (callback, True)
                    threading.Thread(
                        target=self._safe_callback, args=(callback,), daemon=True
                    ).start()

        return None

    def _on_release(self, key) -> None:
        key_name = self._get_key_name(key)
        if not key_name:
            return

        with self._keys_lock:
            self._current_keys.discard(key_name)
            for hotkey_keys in self._hotkeys:
                if key_name in hotkey_keys:
                    callback, _ = self._hotkeys[hotkey_keys]
                    self._hotkeys[hotkey_keys] = (callback, False)

    def _safe_callback(self, callback: Callable[[], None]) -> None:
        try:
            callback()
        except Exception as e:
            self._log.error(f"Hotkey callback error: {e}")
            import traceback

            self._log.debug(traceback.format_exc())
