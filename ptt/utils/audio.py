"""
PTT Audio Utilities

Audio-related helper functions for the PTT application.
"""

import sys
from typing import Optional

from .logging import get_logger


def play_beep(frequency: int = 800, duration_ms: int = 200) -> None:
    """
    Play a beep sound on Windows.
    
    Args:
        frequency: Beep frequency in Hz (default: 800)
        duration_ms: Beep duration in milliseconds (default: 200)
    """
    log = get_logger("audio")
    
    try:
        import winsound
        winsound.Beep(frequency, duration_ms)
    except ImportError:
        # Not on Windows, try terminal bell
        log.debug("winsound not available, using terminal bell")
        print("\a", end="", flush=True)
    except Exception as e:
        log.debug(f"Beep failed: {e}")
        print("\a", end="", flush=True)


def get_audio_devices() -> list[dict]:
    """
    List available audio input devices.
    
    Returns:
        List of dictionaries with device information
    """
    log = get_logger("audio")
    devices = []
    
    try:
        import sounddevice as sd
        device_list = sd.query_devices()
        
        for i, dev in enumerate(device_list):
            if dev["max_input_channels"] > 0:
                devices.append({
                    "index": i,
                    "name": dev["name"],
                    "channels": dev["max_input_channels"],
                    "sample_rate": dev["default_samplerate"],
                })
    except Exception as e:
        log.error(f"Failed to query audio devices: {e}")
    
    return devices


def validate_audio_file(path) -> Optional[tuple[int, float]]:
    """
    Validate an audio file and return its properties.
    
    Args:
        path: Path to audio file
        
    Returns:
        Tuple of (sample_rate, duration_seconds) or None if invalid
    """
    log = get_logger("audio")
    
    try:
        from pathlib import Path
        from scipy.io import wavfile
        import numpy as np
        
        path = Path(path)
        if not path.exists():
            log.error(f"Audio file not found: {path}")
            return None
        
        if path.stat().st_size == 0:
            log.error(f"Audio file is empty: {path}")
            return None
        
        sample_rate, data = wavfile.read(str(path))
        duration = len(data) / sample_rate
        
        return (sample_rate, duration)
        
    except Exception as e:
        log.error(f"Failed to validate audio file: {e}")
        return None
