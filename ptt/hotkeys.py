"""
PTT Hotkey Manager

Keyboard hotkey handling using pynput.
Provides a clean interface for registering and handling hotkey combinations.
"""

import threading
from typing import Callable, Optional

from pynput import keyboard

from .utils.logging import get_logger


class HotkeyManager:
    """
    Manages keyboard hotkeys using pynput.
    
    Provides a clean interface for:
    - Registering hotkey combinations with callbacks
    - Handling key press/release events
    - Graceful shutdown on Ctrl+C or Escape
    
    Usage:
        manager = HotkeyManager()
        
        manager.register("<ctrl>+<alt>+<space>", on_toggle)
        manager.register("<ctrl>+<alt>+i", on_attach)
        
        manager.start()  # Blocks until stopped
    """
    
    def __init__(self):
        """Initialize the HotkeyManager."""
        self._log = get_logger("hotkeys")
        
        # Registered hotkeys: {frozenset of keys -> (callback, triggered flag)}
        self._hotkeys: dict[frozenset, tuple[Callable, bool]] = {}
        
        # Currently pressed keys
        self._current_keys: set[str] = set()
        self._keys_lock = threading.Lock()
        
        # Listener
        self._listener: Optional[keyboard.Listener] = None
        self._running = False
        
        # Shutdown callback
        self._on_shutdown: Optional[Callable[[], None]] = None
    
    def register(self, hotkey_str: str, callback: Callable[[], None]) -> None:
        """
        Register a hotkey combination.
        
        Args:
            hotkey_str: Hotkey string like "<ctrl>+<alt>+<space>"
            callback: Function to call when hotkey is pressed
        """
        keys = self._parse_hotkey(hotkey_str)
        self._hotkeys[keys] = (callback, False)
        self._log.debug(f"Registered hotkey: {hotkey_str} -> {keys}")
    
    def set_shutdown_callback(self, callback: Callable[[], None]) -> None:
        """
        Set a callback to be called when shutting down.
        
        Args:
            callback: Function to call on shutdown
        """
        self._on_shutdown = callback
    
    def start(self) -> None:
        """
        Start listening for hotkeys.
        
        This method blocks until the listener is stopped
        (by Escape, Ctrl+C, or calling stop()).
        """
        self._running = True
        self._log.info("Listening for hotkeys... (Escape or Ctrl+C to exit)")
        
        try:
            with keyboard.Listener(
                on_press=self._on_press,
                on_release=self._on_release,
                suppress=False
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
        """
        Start listening for hotkeys in a background thread.
        
        Use stop() to stop the listener.
        """
        thread = threading.Thread(target=self.start, daemon=True)
        thread.start()
    
    def stop(self) -> None:
        """Stop the listener."""
        self._running = False
        if self._listener:
            self._listener.stop()
    
    def _parse_hotkey(self, hotkey_str: str) -> frozenset:
        """
        Parse a hotkey string into a set of normalized key names.
        
        Args:
            hotkey_str: String like "<ctrl>+<alt>+<space>"
            
        Returns:
            Frozenset of key names
        """
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
        """
        Get a normalized key name from a pynput key.
        
        Args:
            key: pynput key object
            
        Returns:
            Normalized key name or None
        """
        try:
            if hasattr(key, 'char') and key.char:
                return key.char.lower()
            elif hasattr(key, 'name'):
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
        """
        Handle key press event.
        
        Args:
            key: pynput key object
            
        Returns:
            False to stop the listener, None to continue
        """
        key_name = self._get_key_name(key)
        if not key_name:
            return None
        
        with self._keys_lock:
            self._current_keys.add(key_name)
            
            # Check for Escape to exit
            if key_name == "esc":
                self._log.info("Escape pressed - shutting down...")
                return False
            
            # Check for Ctrl+C to exit
            if "ctrl" in self._current_keys and key_name == "c":
                self._log.info("Ctrl+C pressed - shutting down...")
                return False
            
            # Check registered hotkeys
            for hotkey_keys, (callback, triggered) in list(self._hotkeys.items()):
                if hotkey_keys.issubset(self._current_keys) and not triggered:
                    # Mark as triggered to prevent repeat
                    self._hotkeys[hotkey_keys] = (callback, True)
                    
                    # Run callback in thread to not block key handling
                    threading.Thread(target=self._safe_callback, args=(callback,), daemon=True).start()
        
        return None
    
    def _on_release(self, key) -> None:
        """
        Handle key release event.
        
        Args:
            key: pynput key object
        """
        key_name = self._get_key_name(key)
        if not key_name:
            return
        
        with self._keys_lock:
            self._current_keys.discard(key_name)
            
            # Reset triggered flags for hotkeys containing this key
            for hotkey_keys in self._hotkeys:
                if key_name in hotkey_keys:
                    callback, _ = self._hotkeys[hotkey_keys]
                    self._hotkeys[hotkey_keys] = (callback, False)
    
    def _safe_callback(self, callback: Callable[[], None]) -> None:
        """
        Call a callback safely, catching any exceptions.
        
        Args:
            callback: Function to call
        """
        try:
            callback()
        except Exception as e:
            self._log.error(f"Hotkey callback error: {e}")
            import traceback
            self._log.debug(traceback.format_exc())
