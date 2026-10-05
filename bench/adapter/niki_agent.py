"""
NIKI installed agent adapter for Harbor (Terminal-Bench 2.x).

Installs the static musl-compiled NIKI binary into the task container,
executes the headless `niki agent` entry point, captures execution logs,
and extracts ATIF trajectories.
"""

import json
import os
import shutil
from pathlib import Path
from typing import Any, Dict, Optional

try:
    from harbor.agents.base import BaseInstalledAgent
except ImportError:
    # Standalone fallback when Harbor is not directly installed in the host python env
    class BaseInstalledAgent:
        def __init__(self, *args, **kwargs):
            pass


class NikiInstalledAgent(BaseInstalledAgent):
    """Harbor installed agent adapter for NIKI."""

    def __init__(
        self,
        binary_path: Optional[str] = None,
        model: Optional[str] = None,
        max_time_sec: int = 1800,
        max_cost_usd: float = 1.0,
        **kwargs,
    ):
        super().__init__(**kwargs)
        self.binary_path = binary_path or os.environ.get("NIKI_BIN", "/usr/local/bin/niki")
        self.model = model or os.environ.get("NIKI_MODEL", "qwen2.5-coder:3b")
        self.max_time_sec = max_time_sec
        self.max_cost_usd = max_cost_usd

    def install(self, container: Any) -> None:
        """
        Installs the single static NIKI binary into the task container at /usr/local/bin/niki.
        Requires no host runtime, Node.js, or Python inside the container.
        """
        if not os.path.exists(self.binary_path):
            raise FileNotFoundError(f"NIKI binary not found at {self.binary_path}")

        # Copy the static engine binary into the container
        container.copy_to(self.binary_path, "/usr/local/bin/niki")
        container.exec(["chmod", "+x", "/usr/local/bin/niki"])

    def run(self, task: str, container: Any, out_dir: Path) -> Dict[str, Any]:
        """
        Executes headless `niki agent` on the task inside the container.
        Produces trajectory.json in ATIF format.
        """
        atif_path = out_dir / "trajectory.json"
        in_container_atif = "/tmp/trajectory.json"

        cmd = [
            "/usr/local/bin/niki",
            "agent",
            task,
            "--atif-out",
            in_container_atif,
            "--max-time",
            str(self.max_time_sec),
            "--max-cost",
            str(self.max_cost_usd),
        ]
        if self.model:
            cmd.extend(["--model", self.model])

        result = container.exec(cmd)

        # Retrieve the ATIF trajectory if generated
        try:
            container.copy_from(in_container_atif, str(atif_path))
        except Exception as e:
            # Trajectory failed to copy or was not generated
            pass

        return {
            "exit_code": result.exit_code,
            "stdout": result.stdout,
            "stderr": result.stderr,
            "trajectory_path": str(atif_path) if atif_path.exists() else None,
        }
