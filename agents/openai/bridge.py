"""Shelldone ↔ OpenAI Agents SDK bridge.

The bridge expects JSON-encoded commands on stdin and writes JSON responses
to stdout.  Each command must be a single line object with the following
shape:

```
{"type": "run", "input": "Hello", "session": "optional-id"}
{"type": "shutdown"}
```

Configuration is supplied via CLI flags or a JSON file.  Example usage:

```
python bridge.py --instructions "Ты полезный ассистент" --model gpt-4.1-mini
```

The adapter requires the `openai-agents` package.  If it is missing the
bridge will return structured errors rather than crashing.
"""

from __future__ import annotations

import argparse
import json
import logging
import sys
from dataclasses import dataclass
from typing import Any, Dict, Optional

try:  # pragma: no cover - импорт зависит от внешнего пакета
    from agents import Agent, Runner  # type: ignore
except ModuleNotFoundError:  # pragma: no cover - fallback
    Agent = None  # type: ignore
    Runner = None  # type: ignore


LOG = logging.getLogger("shelldone.openai.adapter")


class SDKNotInstalled(RuntimeError):
    """Raised when `openai-agents` is missing."""


@dataclass
class AdapterConfig:
    instructions: str
    model: str = "gpt-4.1-mini"
    session_backend: str = "memory"  # or "sqlite", "redis" in будущих версиях


class OpenAIAgentAdapter:
    """Thin wrapper around the OpenAI Agents SDK."""

    def __init__(self, config: AdapterConfig):
        self.config = config
        self._agent = None
        self._ensure_sdk()

    def _ensure_sdk(self) -> None:
        if Agent is None or Runner is None:
            raise SDKNotInstalled(
                "openai-agents не установлен. Выполните `pip install -r "
                "agents/openai/requirements.lock` в виртуальном окружении."
            )

    def _ensure_agent(self) -> Any:
        if self._agent is None:
            self._agent = Agent(
                name="Shelldone",
                instructions=self.config.instructions,
                model=self.config.model,
            )
        return self._agent

    def run(self, prompt: str, session: Optional[str] = None) -> str:
        agent = self._ensure_agent()
        if session:
            kwargs = {"session": session}
        else:
            kwargs = {}
        result = Runner.run_sync(agent, prompt, **kwargs)
        return getattr(result, "final_output", "")


def parse_args(argv: Optional[list[str]] = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Shelldone OpenAI bridge")
    parser.add_argument(
        "--instructions",
        help="Системные инструкции для агента",
        default="You are a helpful terminal assistant.",
    )
    parser.add_argument(
        "--model",
        help="Имя модели OpenAI",
        default="gpt-4.1-mini",
    )
    parser.add_argument(
        "--log-level",
        default="INFO",
        choices=["DEBUG", "INFO", "WARNING", "ERROR"],
    )
    return parser.parse_args(argv)


def emit(response: Dict[str, Any]) -> None:
    sys.stdout.write(json.dumps(response, ensure_ascii=False) + "\n")
    sys.stdout.flush()


def main(argv: Optional[list[str]] = None) -> int:
    args = parse_args(argv)
    logging.basicConfig(level=getattr(logging, args.log_level))

    try:
        adapter = OpenAIAgentAdapter(
            AdapterConfig(instructions=args.instructions, model=args.model)
        )
        emit({"status": "ready", "model": args.model})
    except SDKNotInstalled as exc:
        emit({"status": "error", "error": str(exc)})
        return 1

    for raw_line in sys.stdin:
        raw_line = raw_line.strip()
        if not raw_line:
            continue
        try:
            payload = json.loads(raw_line)
        except json.JSONDecodeError as exc:
            emit({"status": "error", "error": f"invalid JSON: {exc}"})
            continue

        command = payload.get("type")
        if command == "shutdown":
            emit({"status": "ok", "message": "shutdown"})
            return 0
        if command != "run":
            emit({"status": "error", "error": f"unknown command {command!r}"})
            continue

        message = payload.get("input")
        if not isinstance(message, str):
            emit({"status": "error", "error": "run command requires string 'input'"})
            continue

        session = payload.get("session")
        if session is not None and not isinstance(session, str):
            emit({"status": "error", "error": "session must be a string"})
            continue

        try:
            output = adapter.run(message, session=session)
            emit({"status": "ok", "output": output, "session": session})
        except Exception as exc:  # pragma: no cover - защитный слой
            LOG.exception("adapter failure")
            emit({"status": "error", "error": str(exc)})

    return 0


if __name__ == "__main__":  # pragma: no cover
    sys.exit(main())
