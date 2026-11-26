#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only OR Jinn-Commercial-1.0
"""
web/syndicate_broadcast.py

Wrapper de compatibilidade em web/ para o pipeline canônico em scripts/syndicate_broadcast.py,
mantendo uma única implementação compartilhada sem duplicação de lógica.
"""

import runpy
from pathlib import Path

CANONICAL_SCRIPT = Path(__file__).resolve().parent.parent / "scripts" / "syndicate_broadcast.py"

if __name__ == "__main__":
    runpy.run_path(str(CANONICAL_SCRIPT), run_name="__main__")
