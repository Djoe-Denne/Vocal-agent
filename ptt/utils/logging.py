"""
PTT Logging Utilities

Centralized logging setup for the PTT application.
Logs to both file and console with different verbosity levels.
"""

import logging
import sys
from datetime import datetime
from pathlib import Path
from typing import Optional

# Global logger instance
_logger: Optional[logging.Logger] = None


def setup_logging(log_dir: Path, name: str = "PTT") -> logging.Logger:
    """
    Set up logging to both file and console.
    
    Args:
        log_dir: Directory for log files
        name: Logger name (default: "PTT")
        
    Returns:
        logging.Logger: Configured logger instance
    """
    global _logger
    
    log_dir.mkdir(parents=True, exist_ok=True)
    log_file = log_dir / f"ptt_{datetime.now().strftime('%Y%m%d')}.log"

    # Create logger
    logger = logging.getLogger(name)
    logger.setLevel(logging.DEBUG)
    
    # Clear existing handlers
    logger.handlers.clear()

    # File handler (detailed)
    file_handler = logging.FileHandler(log_file, encoding="utf-8")
    file_handler.setLevel(logging.DEBUG)
    file_format = logging.Formatter(
        "%(asctime)s | %(levelname)-8s | %(name)s | %(message)s",
        datefmt="%Y-%m-%d %H:%M:%S"
    )
    file_handler.setFormatter(file_format)

    # Console handler (info and above)
    console_handler = logging.StreamHandler(sys.stdout)
    console_handler.setLevel(logging.INFO)
    console_format = logging.Formatter("[%(levelname)s] %(message)s")
    console_handler.setFormatter(console_format)

    logger.addHandler(file_handler)
    logger.addHandler(console_handler)
    
    _logger = logger
    return logger


def get_logger(name: Optional[str] = None) -> logging.Logger:
    """
    Get a logger instance.
    
    Args:
        name: Optional sub-logger name. If provided, creates a child logger.
              If None, returns the main PTT logger.
              
    Returns:
        logging.Logger: Logger instance
    """
    global _logger
    
    if _logger is None:
        # Fallback: create a basic logger if setup_logging wasn't called
        _logger = logging.getLogger("PTT")
        if not _logger.handlers:
            console_handler = logging.StreamHandler(sys.stdout)
            console_handler.setLevel(logging.INFO)
            console_format = logging.Formatter("[%(levelname)s] %(message)s")
            console_handler.setFormatter(console_format)
            _logger.addHandler(console_handler)
            _logger.setLevel(logging.DEBUG)
    
    if name:
        return _logger.getChild(name)
    return _logger
