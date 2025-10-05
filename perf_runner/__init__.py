"""Compatibility layer forwarding to scripts.perf_runner."""
from importlib import import_module

_module = import_module("scripts.perf_runner")

for name, value in vars(_module).items():
    if not name.startswith("__"):
        globals()[name] = value

__path__ = getattr(_module, "__path__", [])
