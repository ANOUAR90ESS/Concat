"""The Concat API over a pipe: `concat-cli api`, one JSON-RPC call a line.

The process is kept for the whole edit, so the project stays open in one
session between calls, the way the window holds it.
"""
from __future__ import annotations

import json
import os
import pathlib
import shutil
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[3]


class ConcatError(RuntimeError):
    """A call the API refused or failed, with the API's own sentence."""


def find_cli(explicit: str | None = None) -> str:
    """The concat-cli binary: the one named, $CONCAT_CLI, PATH, or a build
    under src/target, newest profile first."""
    for candidate in (explicit, os.environ.get("CONCAT_CLI")):
        if candidate:
            return candidate
    on_path = shutil.which("concat-cli")
    if on_path:
        return on_path
    for profile in ("app", "release", "quick", "debug"):
        built = ROOT / "src" / "target" / profile / "concat-cli"
        if built.exists():
            return str(built)
    raise ConcatError(
        "concat-cli not found: build it with `cargo build --profile quick -p concat-cli` "
        "in src/, or pass --concat-cli / set CONCAT_CLI"
    )


class Concat:
    """One `concat-cli api` process. Use as a context manager."""

    def __init__(self, cli: str | None = None, verbose: bool = False):
        self.cli = find_cli(cli)
        self.verbose = verbose
        self.next_id = 1
        self.events: list[dict] = []
        self.proc: subprocess.Popen | None = None

    def __enter__(self) -> "Concat":
        self.proc = subprocess.Popen(
            [self.cli, "api"],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            text=True,
            bufsize=1,
        )
        return self

    def __exit__(self, *_exc) -> None:
        if self.proc:
            self.proc.stdin.close()
            self.proc.wait(timeout=60)

    def _read(self) -> dict:
        line = self.proc.stdout.readline()
        if not line:
            raise ConcatError("concat-cli exited")
        return json.loads(line)

    def call(self, method: str, **params) -> dict:
        """One call; the result, or ConcatError with the API's message.
        Events that arrive while waiting are kept in `events`."""
        call_id = self.next_id
        self.next_id += 1
        request = {"jsonrpc": "2.0", "id": call_id, "method": method, "params": params}
        if self.verbose:
            print(">", json.dumps(request, ensure_ascii=False), file=sys.stderr)
        self.proc.stdin.write(json.dumps(request) + "\n")
        self.proc.stdin.flush()
        while True:
            message = self._read()
            if message.get("id") == call_id:
                if "error" in message:
                    raise ConcatError(f"{method}: {message['error'].get('message')}")
                return message.get("result")
            self.events.append(message)

    def edit(self, project: str, command: dict) -> str | None:
        """Applies one edit command; the id it minted, if any."""
        view = self.call("edit.apply", path=project, command=command)
        return view.get("createdId")

    def wait_for(self, job: str, on_progress=None) -> dict:
        """Reads events until the job ends; the done event's params."""
        pending, self.events = self.events, []
        while True:
            message = pending.pop(0) if pending else self._read()
            params = message.get("params") or {}
            if params.get("job") != job:
                continue
            method = message.get("method")
            if method == "export.progress" and on_progress:
                on_progress(params)
            elif method == "export.done":
                return params
            elif method == "export.failed":
                raise ConcatError(f"export failed: {(params.get('error') or {}).get('message', params)}")
