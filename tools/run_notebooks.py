#!/usr/bin/env python3
"""Execute Polaroid notebooks headlessly and log results under historia/.

Designed to be deterministic and to never hide output. It will:
- execute each notebook with nbclient
- write an executed copy to historia/notebook_runs/
- write a per-notebook log summary

Usage:
  python polaroid/tools/run_notebooks.py
  python polaroid/tools/run_notebooks.py polaroid/notebooks/phase2_operations_test.ipynb
"""

from __future__ import annotations

import argparse
import datetime as _dt
import json
from pathlib import Path
from typing import Any

import nbformat
from nbclient import NotebookClient
from nbclient.exceptions import CellExecutionError


def _timestamp() -> str:
    return _dt.datetime.now().strftime("%Y%m%d_%H%M%S")


def _workspace_root() -> Path:
    # polaroid/tools/run_notebooks.py -> <root>/polaroid/tools/run_notebooks.py
    return Path(__file__).resolve().parents[2]


def _historia_runs_dir(root: Path) -> Path:
    path = root / "historia" / "notebook_runs"
    path.mkdir(parents=True, exist_ok=True)
    return path


def _default_notebooks(root: Path) -> list[Path]:
    nb_dir = root / "polaroid" / "notebooks"
    return sorted([p for p in nb_dir.glob("*.ipynb") if p.is_file()])


def _execute_one(nb_path: Path, out_dir: Path, timeout_s: int, kernel_name: str | None) -> dict[str, Any]:
    started = _dt.datetime.now().isoformat(timespec="seconds")

    nb = nbformat.read(str(nb_path), as_version=4)

    client_kwargs: dict[str, Any] = {
        "timeout": timeout_s,
        "allow_errors": False,
    }
    if kernel_name:
        client_kwargs["kernel_name"] = kernel_name

    client = NotebookClient(nb, **client_kwargs)

    ok = True
    error: str | None = None

    try:
        client.execute()
    except CellExecutionError as e:
        ok = False
        error = str(e)

    executed_path = out_dir / f"{nb_path.stem}__executed__{_timestamp()}.ipynb"
    nbformat.write(nb, str(executed_path))

    summary_path = out_dir / f"{nb_path.stem}__summary__{_timestamp()}.json"
    summary = {
        "notebook": str(nb_path),
        "started": started,
        "ok": ok,
        "error": error,
        "executed_notebook": str(executed_path),
    }
    summary_path.write_text(json.dumps(summary, indent=2), encoding="utf-8")

    return summary


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("notebooks", nargs="*", help="Notebook paths. If omitted, runs all polaroid/notebooks/*.ipynb")
    parser.add_argument("--timeout", type=int, default=300, help="Per-cell timeout in seconds")
    parser.add_argument("--kernel", default=None, help="Kernel name (optional)")
    args = parser.parse_args()

    root = _workspace_root()
    out_dir = _historia_runs_dir(root)

    if args.notebooks:
        notebooks = [Path(p).resolve() for p in args.notebooks]
    else:
        notebooks = _default_notebooks(root)

    if not notebooks:
        print("No notebooks found to run.")
        return 2

    failures = 0
    for nb_path in notebooks:
        print(f"\n=== Running: {nb_path} ===")
        summary = _execute_one(nb_path, out_dir, timeout_s=args.timeout, kernel_name=args.kernel)
        print(json.dumps(summary, indent=2))
        if not summary["ok"]:
            failures += 1

    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
