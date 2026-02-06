"""
PTT Audio Utilities

Audio-related helper functions for the PTT application.
"""

import sys
from typing import Optional

import numpy as np

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


def prepare_audio(audio_data: np.ndarray, sample_rate: int) -> np.ndarray:
    """
    Prepare audio data for transcription.

    Converts to float32 normalised to [-1, 1], ensures mono, and
    resamples to 16 kHz.
    """
    if audio_data.dtype == np.int16:
        audio_float = audio_data.astype(np.float32) / 32768.0
    elif audio_data.dtype == np.float32:
        audio_float = audio_data
    else:
        audio_float = audio_data.astype(np.float32)

    if len(audio_float.shape) > 1:
        audio_float = audio_float.mean(axis=1)

    if sample_rate != 16000:
        from scipy import signal

        num_samples = int(len(audio_float) * 16000 / sample_rate)
        audio_float = signal.resample(audio_float, num_samples)

    return audio_float


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
