#!/usr/bin/env python3
"""
Push-to-Talk Speech-to-Text for Windows

Simple launcher script for the PTT application.
Run directly or use: python -m ptt

Usage:
    python ptt.py           Normal mode
    python ptt.py -d        Debug mode (confirm before sending)
    python ptt.py --debug   Same as -d
"""

import sys
from pathlib import Path

# Add the parent directory to path for imports
sys.path.insert(0, str(Path(__file__).parent))

from ptt.__main__ import main

if __name__ == "__main__":
    main()
