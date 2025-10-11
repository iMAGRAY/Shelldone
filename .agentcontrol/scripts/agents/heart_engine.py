#!/usr/bin/env python3
"""Lightweight local memory index for SDK agents."""
from __future__ import annotations

import argparse
import json
import math
import os
import re
import subprocess
import sys
import textwrap
from collections import Counter
from datetime import datetime, timezone
from fnmatch import fnmatch
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path
from typing import Dict, Iterable, List, Tuple
from urllib.parse import parse_qs, urlparse

ROOT = Path(__file__).resolve().parents[2]
CONFIG_PATH = ROOT / "config" / "heart.json"
DEFAULT_CONFIG: Dict[str, object] = {
    "index_dir": "context/heart",
    "chunk_chars": 1200,
    "chunk_overlap": 200,
    "max_file_bytes": 524_288,
    "include_globs": [
        "**/*.py",
        "**/*.rs",
        "**/*.ts",
        "**/*.tsx",
        "**/*.js",
        "**/*.jsx",
        "**/*.sh",
        "**/*.md",
        "**/*.yaml",
        "**/*.yml",
        "**/*.json",
    ],
    "exclude_globs": [
        "vendor/**",
        "scripts/bin/**",
        "scripts/__pycache__/**",
        "node_modules/**",
        ".git/**",
        ".venv/**",
        "context/heart/**",
        "reports/**",
        "sbom/**",
    ],
    "stop_words": [
        "the",
        "and",
        "for",
        "from",
        "with",
        "this",
        "that",
        "into",
        "about",
        "shall",
        "should",
        "todo",
    ],
    "top_k": 6,
    "max_results": 10,
    "max_snippet_chars": 320,
}
TOKEN_REGEX = re.compile(r"[A-Za-z0-9_]{2,}")


def load_config() -> Dict[str, object]:
    if CONFIG_PATH.exists():
        with CONFIG_PATH.open("r", encoding="utf-8") as fh:
            cfg = json.load(fh)
        merged = {**DEFAULT_CONFIG, **cfg}
        return merged
    return dict(DEFAULT_CONFIG)


def ensure_dirs(cfg: Dict[str, object]) -> Dict[str, Path]:
    index_dir = ROOT / cfg["index_dir"]
    index_dir.mkdir(parents=True, exist_ok=True)
    paths = {
        "index_dir": index_dir,
        "chunks": index_dir / "chunks.jsonl",
        "manifest": index_dir / "manifest.json",
        "summary": index_dir / "summary.json",
    }
    return paths


def iter_files(cfg: Dict[str, object]) -> Iterable[Path]:
    include = cfg.get("include_globs", [])
    exclude = cfg.get("exclude_globs", [])
    include = [str(pattern) for pattern in include]
    exclude = [str(pattern) for pattern in exclude]
    for path in ROOT.rglob("*"):
        if not path.is_file():
            continue
        rel = path.relative_to(ROOT)
        rel_str = rel.as_posix()
        if include and not any(fnmatch(rel_str, pattern) for pattern in include):
            continue
        if any(fnmatch(rel_str, pattern) for pattern in exclude):
            continue
        if path.stat().st_size > int(cfg["max_file_bytes"]):
            continue
        yield path


def chunk_text(text: str, *, chunk_chars: int, chunk_overlap: int) -> Iterable[Tuple[int, int, str, int, int]]:
    length = len(text)
    if length == 0:
        return
    step = max(1, chunk_chars - chunk_overlap)
    start = 0
    while start < length:
        end = min(length, start + chunk_chars)
        chunk = text[start:end]
        prefix = text[:start]
        start_line = prefix.count("\n") + 1
        end_line = start_line + chunk.count("\n")
        yield start, end, chunk, start_line, end_line
        if end == length:
            break
        start += step


def tokenize(text: str, stop_words: Iterable[str]) -> List[str]:
    stop = {word.lower() for word in stop_words}
    tokens = [tok.lower() for tok in TOKEN_REGEX.findall(text)]
    return [tok for tok in tokens if tok not in stop]


def build_index(cfg: Dict[str, object]) -> None:
    paths = ensure_dirs(cfg)
    chunk_chars = int(cfg["chunk_chars"])
    chunk_overlap = int(cfg["chunk_overlap"])
    stop_words = cfg.get("stop_words", [])
    snippets = []
    vectors = []
    df = Counter()

    files = list(iter_files(cfg))
    total_files = len(files)
    if total_files == 0:
        (paths["chunks"]).write_text("", encoding="utf-8")
        manifest = {
            "generated_at": utc_now_iso(),
            "chunks": 0,
            "files": total_files,
            "config": cfg,
        }
        paths["manifest"].write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
        paths["summary"].write_text(json.dumps([], ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
        return

    chunk_records = []
    for path in files:
        try:
            text = path.read_text(encoding="utf-8")
        except UnicodeDecodeError:
            text = path.read_text(encoding="utf-8", errors="ignore")
        if not text.strip():
            continue
        for start, end, chunk, start_line, end_line in chunk_text(
            text, chunk_chars=chunk_chars, chunk_overlap=chunk_overlap
        ):
            tokens = tokenize(chunk, stop_words)
            if not tokens:
                continue
            counts = Counter(tokens)
            df.update(counts.keys())
            chunk_records.append(
                {
                    "path": path.relative_to(ROOT).as_posix(),
                    "start": start,
                    "end": end,
                    "start_line": start_line,
                    "end_line": end_line,
                    "text": chunk,
                    "counts": counts,
                    "token_total": sum(counts.values()),
                }
            )
    if not chunk_records:
        warn("Heart: нет пригодных чанков для индекса")
        return

    total_chunks = len(chunk_records)
    chunk_lines: List[str] = []
    summaries: Dict[str, dict] = {}

    max_snippet = int(cfg.get("max_snippet_chars", 320))
    for record in chunk_records:
        counts: Counter = record.pop("counts")
        token_total = record.pop("token_total") or 1
        weights: Dict[str, float] = {}
        for token, freq in counts.items():
            tf = freq / token_total
            idf = math.log((total_chunks + 1) / (df[token] + 1)) + 1.0
            weights[token] = tf * idf
        if not weights:
            continue
        # оставляем топ-64 токена
        top_tokens = sorted(weights.items(), key=lambda item: item[1], reverse=True)[:64]
        weights = {token: weight for token, weight in top_tokens}
        norm = math.sqrt(sum(value * value for value in weights.values())) or 1e-9
        chunk_id = f"{record['path']}:{record['start_line']}-{record['end_line']}"
        snippet = textwrap.shorten(record["text"].replace("\n", " "), width=max_snippet, placeholder="…")
        summaries.setdefault(
            record["path"],
            {
                "path": record["path"],
                "summary": snippet,
                "first_chunk_lines": f"{record['start_line']}-{record['end_line']}",
            },
        )
        entry = {
            "id": chunk_id,
            "path": record["path"],
            "start": record["start"],
            "end": record["end"],
            "start_line": record["start_line"],
            "end_line": record["end_line"],
            "weights": weights,
            "norm": norm,
            "snippet": snippet,
            "text": record["text"],
        }
        chunk_lines.append(json.dumps(entry, ensure_ascii=False))

    paths["chunks"].write_text("\n".join(chunk_lines) + "\n", encoding="utf-8")
    paths["summary"].write_text(json.dumps(list(summaries.values()), ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    manifest = {
        "generated_at": utc_now_iso(),
        "chunks": len(chunk_lines),
        "files": total_files,
        "config": cfg,
    }
    paths["manifest"].write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def utc_now_iso() -> str:
    return datetime.now(timezone.utc).replace(microsecond=0).isoformat()


def warn(message: str) -> None:
    print(f"[Heart] {message}", file=sys.stderr)


def load_chunks(cfg: Dict[str, object]) -> List[dict]:
    paths = ensure_dirs(cfg)
    chunks_path = paths["chunks"]
    if not chunks_path.exists():
        return []
    entries: List[dict] = []
    with chunks_path.open("r", encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            entries.append(json.loads(line))
    return entries


def query_chunks(cfg: Dict[str, object], text: str, top_k: int | None = None) -> List[dict]:
    if top_k is None:
        top_k = int(cfg.get("top_k", 5))
    entries = load_chunks(cfg)
    if not entries:
        return []
    stop_words = cfg.get("stop_words", [])
    tokens = tokenize(text, stop_words)
    if not tokens:
        return []
    counts = Counter(tokens)
    total = sum(counts.values()) or 1
    query_weights: Dict[str, float] = {}
    for token, freq in counts.items():
        query_weights[token] = freq / total
    query_norm = math.sqrt(sum(value * value for value in query_weights.values())) or 1e-9

    scored: List[Tuple[float, dict]] = []
    for entry in entries:
        weights: Dict[str, float] = entry.get("weights", {})
        score = 0.0
        for token, q_weight in query_weights.items():
            score += q_weight * weights.get(token, 0.0)
        score /= (query_norm * entry.get("norm", 1e-9))
        if score <= 0:
            continue
        scored.append((score, entry))
    scored.sort(key=lambda item: item[0], reverse=True)
    max_results = int(cfg.get("max_results", top_k))
    return [
        {
            "score": round(score, 6),
            "id": entry["id"],
            "path": entry["path"],
            "start_line": entry["start_line"],
            "end_line": entry["end_line"],
            "snippet": entry.get("snippet", ""),
            "text": entry.get("text", ""),
        }
        for score, entry in scored[: max(top_k, max_results)]
    ]


def print_table(title: str, rows: List[List[str]], headers: List[str]) -> str:
    widths = [len(header) for header in headers]
    for row in rows:
        for idx, cell in enumerate(row):
            widths[idx] = max(widths[idx], len(cell))

    def border(fill: str) -> str:
        return "+" + "+".join(fill * (w + 2) for w in widths) + "+"

    def render_row(cells: List[str]) -> str:
        parts = [f" {cell.ljust(widths[idx])} " for idx, cell in enumerate(cells)]
        return "|" + "|".join(parts) + "|"

    lines = [title, border("-"), render_row(headers), border("=")]
    for row in rows:
        lines.append(render_row(row))
    lines.append(border("-"))
    return "\n".join(lines)


def cmd_sync(args: argparse.Namespace) -> None:
    cfg = load_config()
    build_index(cfg)
    print("Memory Heart index updated")


def cmd_refresh(args: argparse.Namespace) -> None:
    # текущее упрощение: refresh == sync
    cmd_sync(args)


def cmd_install(args: argparse.Namespace) -> None:
    requirements = ROOT / "vendor" / "memory-heart" / "requirements.txt"
    if requirements.exists():
        pip = ROOT / ".venv" / "bin" / "pip"
        if not pip.exists():
            warn(".venv/bin/pip не найден — выполните agentcall setup")
        else:
            subprocess.run([str(pip), "install", "-r", str(requirements)], check=False)
    print("Memory Heart install complete (requirements applied if available)")


def cmd_query(args: argparse.Namespace) -> None:
    cfg = load_config()
    results = query_chunks(cfg, args.query, top_k=args.top_k)
    if args.format == "json":
        print(json.dumps(results, ensure_ascii=False, indent=2))
        return
    rows = []
    for item in results:
        rows.append(
            [
                f"{item['score']:.4f}",
                item["path"],
                f"{item['start_line']}-{item['end_line']}",
                textwrap.shorten(item.get("snippet", ""), width=60, placeholder="…"),
            ]
        )
    if not rows:
        print("Нет подходящих фрагментов")
        return
    print(
        print_table(
            "Heart Query Results",
            rows,
            ["Score", "Path", "Lines", "Snippet"],
        )
    )


class HeartHandler(BaseHTTPRequestHandler):
    cfg = load_config()
    index_cache = load_chunks(cfg)

    def do_GET(self) -> None:  # noqa: N802
        parsed = urlparse(self.path)
        if parsed.path != "/query":
            self.send_response(404)
            self.end_headers()
            self.wfile.write(b"Not Found")
            return
        params = parse_qs(parsed.query)
        query = params.get("q") or params.get("query")
        if not query:
            self.send_response(400)
            self.end_headers()
            self.wfile.write(b"query parameter required")
            return
        text = query[0]
        results = query_chunks(self.cfg, text)
        payload = json.dumps(results, ensure_ascii=False).encode("utf-8")
        self.send_response(200)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)

    def log_message(self, format: str, *args: object) -> None:  # noqa: A003
        return


def cmd_serve(args: argparse.Namespace) -> None:
    cfg = load_config()
    handler = HeartHandler
    handler.cfg = cfg
    handler.index_cache = load_chunks(cfg)
    server = HTTPServer((args.host, args.port), handler)
    print(f"Heart server running on http://{args.host}:{args.port}")
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("Heartbeat server stopped")


def cmd_update(args: argparse.Namespace) -> None:
    subprocess.run(["git", "submodule", "update", "--init", "--recursive", "--remote", "vendor/memory-heart"], check=False)
    print("vendor/memory-heart обновлён")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Memory Heart utility")
    sub = parser.add_subparsers(dest="command", required=True)

    sub.add_parser("install", help="Install runtime dependencies")
    sub.add_parser("sync", help="Index source files into memory heart")
    sub.add_parser("refresh", help="Refresh summaries (alias sync)")
    q = sub.add_parser("query", help="Search the heart index")
    q.add_argument("query", help="Text query")
    q.add_argument("--top-k", type=int, default=None)
    q.add_argument("--format", choices=["table", "json"], default="table")
    srv = sub.add_parser("serve", help="Start lightweight HTTP query server")
    srv.add_argument("--host", default="127.0.0.1")
    srv.add_argument("--port", type=int, default=8765)
    sub.add_parser("update", help="Update memory-heart submodule")
    return parser


def main(argv: List[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    if args.command == "install":
        cmd_install(args)
    elif args.command == "sync":
        cmd_sync(args)
    elif args.command == "refresh":
        cmd_refresh(args)
    elif args.command == "query":
        cmd_query(args)
    elif args.command == "serve":
        cmd_serve(args)
    elif args.command == "update":
        cmd_update(args)
    else:
        parser.print_help()
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
