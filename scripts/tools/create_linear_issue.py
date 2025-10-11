#!/usr/bin/env python3
"""
Utility to create Linear issues via GraphQL.

Usage (environment):
    export LINEAR_API_KEY=...
    python3 scripts/tools/create_linear_issue.py --team <team-id> --title "Issue title"

The script accepts UUIDs or existing base64 global IDs for team/project/state/labels.
"""

from __future__ import annotations

import argparse
import json
import os
import sys
from typing import Iterable

import requests

API_URL = "https://api.linear.app/graphql"


def _normalize_id(raw: str) -> str:
    token = raw.strip()
    if not token:
        raise ValueError(f"{prefix} identifier must be non-empty")

    # UUID format check
    def looks_like_uuid(value: str) -> bool:
        parts = value.split("-")
        if len(parts) != 5:
            return False
        lengths = [8, 4, 4, 4, 12]
        return all(len(part) == expected for part, expected in zip(parts, lengths, strict=False))

    if looks_like_uuid(token):
        return token

    # Support base64 global id like "Team:uuid" etc.
    import base64

    try:
        decoded = base64.b64decode(token, validate=True).decode("utf-8")
    except (ValueError, UnicodeDecodeError) as exc:
        raise ValueError(f"Identifier must be UUID or global ID, got `{raw}`") from exc

    if ":" not in decoded:
        raise ValueError(f"Global ID must contain prefix: `{decoded}`")
    _, uuid_candidate = decoded.split(":", 1)
    if not looks_like_uuid(uuid_candidate):
        raise ValueError(f"Decoded value is not UUID: `{decoded}`")
    return uuid_candidate


def _parse_label_ids(values: str | None) -> list[str]:
    if not values:
        return []
    labels: list[str] = []
    for item in values.split(","):
        trimmed = item.strip()
        if trimmed:
            labels.append(_normalize_id(trimmed))
    return labels


def _build_payload(args: argparse.Namespace) -> dict:
    mutation = """
    mutation IssueCreate($input: IssueCreateInput!) {
      issueCreate(input: $input) {
        success
        issue {
          id
          identifier
          url
        }
      }
    }
    """
    input_data = {
        "teamId": _normalize_id(args.team),
        "title": args.title,
    }
    description = args.description
    if args.description_file:
        try:
            with open(args.description_file, "r", encoding="utf-8") as handle:
                description = handle.read()
        except OSError as exc:  # noqa: PERF203
            raise RuntimeError(f"Failed to read description file {args.description_file}: {exc}") from exc
    if description:
        input_data["description"] = description
    if args.project:
        input_data["projectId"] = _normalize_id(args.project)
    if args.state:
        input_data["stateId"] = _normalize_id(args.state)
    if args.assignee:
        input_data["assigneeId"] = _normalize_id(args.assignee)
    label_ids = _parse_label_ids(args.labels)
    if label_ids:
        input_data["labelIds"] = label_ids
    return {"query": mutation, "variables": {"input": input_data}}


def _request(api_key: str, payload: dict) -> dict:
    headers = {
        "Authorization": api_key,
        "Content-Type": "application/json",
    }
    response = requests.post(API_URL, headers=headers, json=payload, timeout=30)
    if response.status_code != 200:
        raise RuntimeError(f"Linear API returned HTTP {response.status_code}: {response.text}")
    body = response.json()
    if "errors" in body:
        raise RuntimeError(json.dumps(body["errors"], indent=2))
    return body


def main(argv: Iterable[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Create a Linear issue via GraphQL.")
    parser.add_argument("--team", required=True, help="Linear team ID (UUID or base64 global ID).")
    parser.add_argument("--title", required=True, help="Issue title.")
    parser.add_argument("--description", help="Issue description (Markdown supported).")
    parser.add_argument(
        "--description-file",
        help="Path to file with Markdown description (overrides --description if both provided).",
    )
    parser.add_argument("--labels", help="Comma-separated Linear label IDs.")
    parser.add_argument("--project", help="Project ID (UUID or global ID).")
    parser.add_argument("--state", help="Workflow state ID (UUID or global ID).")
    parser.add_argument("--assignee", help="Assignee ID (UUID or global ID).")
    args = parser.parse_args(argv)

    api_key = os.getenv("LINEAR_API_KEY")
    if not api_key:
        print("LINEAR_API_KEY environment variable is not set", file=sys.stderr)
        return 2

    payload = _build_payload(args)
    try:
        result = _request(api_key, payload)
    except Exception as exc:  # noqa: BLE001
        print(f"[linear] issueCreate failed: {exc}", file=sys.stderr)
        return 1

    issue = result["data"]["issueCreate"]["issue"]
    print(json.dumps(issue, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
