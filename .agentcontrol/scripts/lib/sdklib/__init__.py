"""Внутренняя библиотека SDK для высокопроизводительных операций."""

__all__ = ["task_main"]


def task_main(argv: list[str] | None = None) -> int:
    """Ленивая прокладка к основному CLI (для удобства импорта)."""

    from .task_cli import main  # локальный импорт, чтобы избежать предупреждений runpy

    return main(argv)
