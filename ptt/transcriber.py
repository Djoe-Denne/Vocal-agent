"""
PTT Transcriber

Manages ASR models for speech-to-text transcription.
Supports multiple backends: OpenAI Whisper and Hugging Face models (like Qwen3-ASR).
"""

import threading
import time
import traceback
from abc import ABC, abstractmethod
from dataclasses import dataclass
from pathlib import Path
from typing import Optional, Callable, Any

import numpy as np
import torch

from .config import Config
from .utils.logging import get_logger


@dataclass
class TranscriptionResult:
    """Result of a transcription operation."""
    text: str
    segments: list[dict]
    duration: float  # Time taken to transcribe
    audio_duration: float  # Duration of audio in seconds


class BaseTranscriber(ABC):
    """Abstract base class for ASR transcribers."""
    
    def __init__(self, config: Config):
        self.config = config
        self._model: Any = None
        self._model_lock = threading.Lock()
        self._log = get_logger("transcriber")
        
        # Determine device
        self._device = "cuda" if torch.cuda.is_available() else "cpu"
        if self._device == "cpu":
            self._log.warning("No GPU detected - using CPU (will be slow!)")
    
    @property
    def is_loaded(self) -> bool:
        """Check if the model is currently loaded."""
        return self._model is not None
    
    @property
    def device(self) -> str:
        """Get the device being used (cuda or cpu)."""
        return self._device
    
    @abstractmethod
    def load_model(self) -> bool:
        """Load the model into memory."""
        pass
    
    @abstractmethod
    def unload_model(self) -> bool:
        """Unload the model from memory."""
        pass
    
    @abstractmethod
    def transcribe_file(self, audio_path: Path) -> Optional[TranscriptionResult]:
        """Transcribe an audio file."""
        pass
    
    @abstractmethod
    def transcribe_array(
        self,
        audio_data: np.ndarray,
        sample_rate: int,
        on_segment: Optional[Callable[[dict], None]] = None
    ) -> Optional[TranscriptionResult]:
        """Transcribe audio from a numpy array."""
        pass
    
    def get_gpu_info(self) -> dict:
        """Get GPU information."""
        if not torch.cuda.is_available():
            return {}
        
        try:
            return {
                "name": torch.cuda.get_device_name(0),
                "total_memory_gb": torch.cuda.get_device_properties(0).total_memory / (1024**3),
                "allocated_memory_gb": torch.cuda.memory_allocated() / (1024**3),
                "reserved_memory_gb": torch.cuda.memory_reserved() / (1024**3),
            }
        except Exception:
            return {}
    
    def _prepare_audio(self, audio_data: np.ndarray, sample_rate: int) -> np.ndarray:
        """
        Prepare audio data for transcription.
        
        Converts to float32 normalized to [-1, 1], ensures mono, and resamples to 16kHz.
        """
        # Convert to float32 if needed
        if audio_data.dtype == np.int16:
            audio_float = audio_data.astype(np.float32) / 32768.0
        elif audio_data.dtype == np.float32:
            audio_float = audio_data
        else:
            audio_float = audio_data.astype(np.float32)
        
        # Ensure mono
        if len(audio_float.shape) > 1:
            audio_float = audio_float.mean(axis=1)
        
        # Resample to 16kHz if needed
        if sample_rate != 16000:
            from scipy import signal
            num_samples = int(len(audio_float) * 16000 / sample_rate)
            audio_float = signal.resample(audio_float, num_samples)
        
        return audio_float


class WhisperTranscriber(BaseTranscriber):
    """
    Transcriber using OpenAI Whisper model.
    
    Handles:
    - Model loading and unloading
    - GPU memory management
    - Transcription of audio files and numpy arrays
    """
    
    def load_model(self) -> bool:
        """
        Load the Whisper model into memory.
        
        Returns:
            bool: True if model was loaded, False if already loaded or failed
        """
        import whisper
        
        with self._model_lock:
            if self._model is not None:
                self._log.info("Whisper model already loaded")
                return False
            
            model_name = self.config.whisper_model
            self._log.info(f"Loading Whisper model '{model_name}'...")
            start = time.time()
            
            try:
                self._model = whisper.load_model(model_name, device=self._device)
                elapsed = time.time() - start
                self._log.info(f"Whisper model loaded on {self._device.upper()} in {elapsed:.1f}s")
                
                # Log GPU memory usage
                if torch.cuda.is_available():
                    mem_allocated = torch.cuda.memory_allocated() / (1024**3)
                    mem_reserved = torch.cuda.memory_reserved() / (1024**3)
                    self._log.info(f"GPU memory: {mem_allocated:.2f} GB allocated, {mem_reserved:.2f} GB reserved")
                
                return True
                
            except Exception as e:
                self._log.error(f"Failed to load Whisper model: {e}")
                self._log.debug(traceback.format_exc())
                self._model = None
                return False
    
    def unload_model(self) -> bool:
        """
        Unload the Whisper model from memory.
        
        Returns:
            bool: True if model was unloaded, False if not loaded
        """
        with self._model_lock:
            if self._model is None:
                self._log.info("Whisper model not loaded")
                return False
            
            self._log.info("Unloading Whisper model...")
            
            try:
                del self._model
                self._model = None
                
                # Clear GPU cache
                if torch.cuda.is_available():
                    torch.cuda.empty_cache()
                    torch.cuda.synchronize()
                
                self._log.info("Whisper model unloaded, GPU memory freed")
                return True
                
            except Exception as e:
                self._log.error(f"Failed to unload Whisper model: {e}")
                self._log.debug(traceback.format_exc())
                return False
    
    def transcribe_file(self, audio_path: Path) -> Optional[TranscriptionResult]:
        """
        Transcribe an audio file.
        
        Args:
            audio_path: Path to the audio file
            
        Returns:
            TranscriptionResult or None if transcription failed
        """
        if self._model is None:
            self._log.error("Whisper model not loaded!")
            return None
        
        if not audio_path.exists():
            self._log.error(f"Audio file not found: {audio_path}")
            return None
        
        self._log.info(f"Transcribing: {audio_path.name}")
        start_time = time.time()
        
        try:
            # Transcription options
            fp16 = not self.config.whisper_force_fp16_false and torch.cuda.is_available()
            
            # Build transcription kwargs
            transcribe_kwargs = {
                "language": self.config.whisper_language,
                "fp16": fp16,
                "verbose": False,
            }
            
            # Add initial prompt if configured (helps avoid filler transcription)
            if self.config.whisper_initial_prompt:
                transcribe_kwargs["initial_prompt"] = self.config.whisper_initial_prompt
            
            # Suppress filler sounds if configured
            if self.config.whisper_suppress_fillers:
                # Higher threshold = stricter about what counts as speech
                transcribe_kwargs["no_speech_threshold"] = 0.6
                transcribe_kwargs["condition_on_previous_text"] = False
            
            result = self._model.transcribe(str(audio_path), **transcribe_kwargs)
            
            elapsed = time.time() - start_time
            text = result["text"].strip()
            
            # Calculate audio duration from segments
            audio_duration = 0.0
            if result.get("segments"):
                audio_duration = result["segments"][-1].get("end", 0.0)
            
            self._log.info(f"Transcription complete in {elapsed:.1f}s ({len(text)} chars)")
            
            return TranscriptionResult(
                text=text,
                segments=result.get("segments", []),
                duration=elapsed,
                audio_duration=audio_duration,
            )
            
        except Exception as e:
            self._log.error(f"Transcription failed: {e}")
            self._log.debug(traceback.format_exc())
            return None
    
    def transcribe_array(
        self,
        audio_data: np.ndarray,
        sample_rate: int,
        on_segment: Optional[Callable[[dict], None]] = None
    ) -> Optional[TranscriptionResult]:
        """
        Transcribe audio from a numpy array.
        
        Args:
            audio_data: Audio samples as numpy array (int16 or float32)
            sample_rate: Sample rate of the audio
            on_segment: Optional callback for each transcribed segment
            
        Returns:
            TranscriptionResult or None if transcription failed
        """
        if self._model is None:
            self._log.error("Whisper model not loaded!")
            return None
        
        audio_duration = len(audio_data) / sample_rate
        self._log.debug(f"Transcribing {audio_duration:.1f}s of audio")
        start_time = time.time()
        
        try:
            audio_float = self._prepare_audio(audio_data, sample_rate)
            
            # Transcription options
            fp16 = not self.config.whisper_force_fp16_false and torch.cuda.is_available()
            
            # Build transcription kwargs
            transcribe_kwargs = {
                "language": self.config.whisper_language,
                "fp16": fp16,
                "verbose": False,
            }
            
            # Add initial prompt if configured (helps avoid filler transcription)
            if self.config.whisper_initial_prompt:
                transcribe_kwargs["initial_prompt"] = self.config.whisper_initial_prompt
            
            # Suppress filler sounds if configured
            if self.config.whisper_suppress_fillers:
                # Higher threshold = stricter about what counts as speech
                transcribe_kwargs["no_speech_threshold"] = 0.6
                transcribe_kwargs["condition_on_previous_text"] = False
            
            result = self._model.transcribe(audio_float, **transcribe_kwargs)
            
            elapsed = time.time() - start_time
            text = result["text"].strip()
            
            # Call segment callback if provided
            if on_segment and result.get("segments"):
                for seg in result["segments"]:
                    on_segment(seg)
            
            self._log.debug(f"Transcription: {text[:100]}..." if len(text) > 100 else f"Transcription: {text}")
            
            return TranscriptionResult(
                text=text,
                segments=result.get("segments", []),
                duration=elapsed,
                audio_duration=audio_duration,
            )
            
        except Exception as e:
            self._log.error(f"Transcription failed: {e}")
            self._log.debug(traceback.format_exc())
            return None


class HuggingFaceTranscriber(BaseTranscriber):
    """
    Transcriber using Hugging Face models via the qwen-asr package (e.g., Qwen3-ASR).
    
    Handles:
    - Model loading and unloading with qwen-asr library
    - GPU memory management
    - Transcription of audio files and numpy arrays
    """
    
    def __init__(self, config: Config):
        super().__init__(config)
        self._backend: str = "transformers"  # or "vllm"
    
    def _extract_text(self, result: Any) -> str:
        """Extract plain text from qwen-asr result objects."""
        if result is None:
            return ""
        
        if isinstance(result, str):
            return result
        
        text = getattr(result, "text", None)
        if isinstance(text, str):
            return text
        
        if isinstance(result, dict) and "text" in result:
            return str(result["text"])
        
        return str(result)
    
    def _resolve_model_device(self) -> str:
        """Best-effort device detection for the underlying model."""
        if self._model is None:
            return "unknown"
        
        for attr in ("model", "asr_model", "net", "nn_model"):
            candidate = getattr(self._model, attr, None)
            if candidate is not None:
                try:
                    param = next(candidate.parameters())
                    return str(param.device)
                except Exception:
                    pass
        
        try:
            param = next(self._model.parameters())
            return str(param.device)
        except Exception:
            return "unknown"
    
    def load_model(self) -> bool:
        """
        Load the Qwen3-ASR model into memory using qwen-asr package.
        
        Returns:
            bool: True if model was loaded, False if already loaded or failed
        """
        from qwen_asr import Qwen3ASRModel
        
        with self._model_lock:
            if self._model is not None:
                self._log.info("Qwen3-ASR model already loaded")
                return False
            
            hf_config = self.config.asr.huggingface
            model_id = hf_config.model
            self._log.info(f"Loading Qwen3-ASR model '{model_id}'...")
            start = time.time()
            
            try:
                self._log.info(
                    "CUDA available=%s, torch=%s, cuda_version=%s, device=%s",
                    torch.cuda.is_available(),
                    torch.__version__,
                    torch.version.cuda,
                    self._device,
                )
                if torch.version.cuda is None:
                    self._log.warning(
                        "PyTorch CUDA build not detected (torch.version.cuda is None). "
                        "Install a CUDA-enabled torch build to use the GPU."
                    )
                if torch.cuda.is_available():
                    gpu_info = self.get_gpu_info()
                    if gpu_info:
                        self._log.info(
                            "GPU: %s (%.1f GB total)",
                            gpu_info.get("name", "unknown"),
                            gpu_info.get("total_memory_gb", 0.0),
                        )
                
                # Determine dtype from config
                dtype_map = {
                    "float16": torch.float16,
                    "bfloat16": torch.bfloat16,
                    "float32": torch.float32,
                }
                torch_dtype = dtype_map.get(hf_config.torch_dtype, torch.float16)
                
                # Fall back to float32 if no GPU available
                if not torch.cuda.is_available():
                    torch_dtype = torch.float32
                
                # Determine device_map
                device_map = "auto" if (torch.cuda.is_available() and hf_config.device_map_auto) else None
                
                # Build model kwargs for qwen-asr (forwarded to AutoModel.from_pretrained)
                model_kwargs = {
                    "torch_dtype": torch_dtype,
                    "device_map": device_map,
                }
                
                # Load model using qwen-asr's Qwen3ASRModel
                self._model = Qwen3ASRModel.from_pretrained(
                    model_id,
                    **model_kwargs,
                )
                
                # Move to device if not using device_map
                if device_map is None and torch.cuda.is_available():
                    try:
                        self._model.model = self._model.model.to(self._device)
                    except Exception as e:
                        self._log.warning("Failed to move Qwen3-ASR model to %s: %s", self._device, e)
                
                elapsed = time.time() - start
                self._log.info(f"Qwen3-ASR model loaded on {self._device.upper()} in {elapsed:.1f}s")
                self._log.info("Qwen3-ASR model device: %s", self._resolve_model_device())
                
                # Log GPU memory usage
                if torch.cuda.is_available():
                    mem_allocated = torch.cuda.memory_allocated() / (1024**3)
                    mem_reserved = torch.cuda.memory_reserved() / (1024**3)
                    self._log.info(f"GPU memory: {mem_allocated:.2f} GB allocated, {mem_reserved:.2f} GB reserved")
                
                return True
                
            except Exception as e:
                self._log.error(f"Failed to load Qwen3-ASR model: {e}")
                self._log.debug(traceback.format_exc())
                self._model = None
                return False
    
    def unload_model(self) -> bool:
        """
        Unload the Qwen3-ASR model from memory.
        
        Returns:
            bool: True if model was unloaded, False if not loaded
        """
        with self._model_lock:
            if self._model is None:
                self._log.info("Qwen3-ASR model not loaded")
                return False
            
            self._log.info("Unloading Qwen3-ASR model...")
            
            try:
                del self._model
                self._model = None
                
                # Clear GPU cache
                if torch.cuda.is_available():
                    torch.cuda.empty_cache()
                    torch.cuda.synchronize()
                
                self._log.info("Qwen3-ASR model unloaded, GPU memory freed")
                return True
                
            except Exception as e:
                self._log.error(f"Failed to unload Qwen3-ASR model: {e}")
                self._log.debug(traceback.format_exc())
                return False
    
    def transcribe_file(self, audio_path: Path) -> Optional[TranscriptionResult]:
        """
        Transcribe an audio file.
        
        Args:
            audio_path: Path to the audio file
            
        Returns:
            TranscriptionResult or None if transcription failed
        """
        if self._model is None:
            self._log.error("Qwen3-ASR model not loaded!")
            return None
        
        if not audio_path.exists():
            self._log.error(f"Audio file not found: {audio_path}")
            return None
        
        self._log.info(f"Transcribing: {audio_path.name}")
        start_time = time.time()
        
        try:
            # qwen-asr can directly accept file paths
            results = self._model.transcribe([str(audio_path)])
            
            text = ""
            if results:
                text = self._extract_text(results[0]).strip()
            
            elapsed = time.time() - start_time
            
            # Get audio duration
            import librosa
            audio_duration = librosa.get_duration(path=str(audio_path))
            
            # Create simple segments
            segments = [{
                "id": 0,
                "start": 0.0,
                "end": audio_duration,
                "text": text,
            }]
            
            self._log.debug(f"Transcription: {text[:100]}..." if len(text) > 100 else f"Transcription: {text}")
            
            return TranscriptionResult(
                text=text,
                segments=segments,
                duration=elapsed,
                audio_duration=audio_duration,
            )
            
        except Exception as e:
            self._log.error(f"Transcription failed: {e}")
            self._log.debug(traceback.format_exc())
            return None
    
    def transcribe_array(
        self,
        audio_data: np.ndarray,
        sample_rate: int,
        on_segment: Optional[Callable[[dict], None]] = None
    ) -> Optional[TranscriptionResult]:
        """
        Transcribe audio from a numpy array.
        
        Args:
            audio_data: Audio samples as numpy array (int16 or float32)
            sample_rate: Sample rate of the audio
            on_segment: Optional callback for each transcribed segment
            
        Returns:
            TranscriptionResult or None if transcription failed
        """
        if self._model is None:
            self._log.error("Qwen3-ASR model not loaded!")
            return None
        
        audio_duration = len(audio_data) / sample_rate
        self._log.debug(f"Transcribing {audio_duration:.1f}s of audio")
        start_time = time.time()
        
        try:
            audio_float = self._prepare_audio(audio_data, sample_rate)
            
            # qwen-asr accepts (np.ndarray, sample_rate) tuples
            results = self._model.transcribe([(audio_float, 16000)])
            
            text = ""
            if results:
                text = self._extract_text(results[0]).strip()
            
            elapsed = time.time() - start_time
            
            # Create simple segments
            segments = [{
                "id": 0,
                "start": 0.0,
                "end": audio_duration,
                "text": text,
            }]
            
            # Call segment callback if provided
            if on_segment:
                for seg in segments:
                    on_segment(seg)
            
            self._log.debug(f"Transcription: {text[:100]}..." if len(text) > 100 else f"Transcription: {text}")
            
            return TranscriptionResult(
                text=text,
                segments=segments,
                duration=elapsed,
                audio_duration=audio_duration,
            )
            
        except Exception as e:
            self._log.error(f"Transcription failed: {e}")
            self._log.debug(traceback.format_exc())
            return None


def create_transcriber(config: Config) -> BaseTranscriber:
    """
    Factory function to create the appropriate transcriber based on config.
    
    Args:
        config: Application configuration
        
    Returns:
        BaseTranscriber: Either WhisperTranscriber or HuggingFaceTranscriber
    """
    backend = config.asr.backend.lower()
    
    if backend == "whisper":
        return WhisperTranscriber(config)
    elif backend == "huggingface":
        return HuggingFaceTranscriber(config)
    else:
        raise ValueError(f"Unknown ASR backend: {backend}. Use 'whisper' or 'huggingface'.")


# Backward compatibility alias
Transcriber = WhisperTranscriber
