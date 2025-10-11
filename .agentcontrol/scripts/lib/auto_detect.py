#!/usr/bin/env python3
"""Автоматическое определение команд для разных стеков.

Скрипт печатает shell-скрипт, который дополняет переменные SDK_*_COMMANDS,
если в config/commands.sh оставлены значения по умолчанию.
"""

from __future__ import annotations

import shlex
import sys
from pathlib import Path

__all__ = ["build_snippet"]


def wrap(condition: str, command: str, skip: str) -> str:
    """Сформировать безопасную конструкцию if/else."""

    skip_quoted = shlex.quote(skip)
    return f"if {condition}; then {command}; else echo {skip_quoted}; fi"


def add_command(bucket: dict, key: str, snippet: str) -> None:
    values = bucket.setdefault(key, [])
    if snippet not in values:
        values.append(snippet)


def detect_yarn(root: Path, result: dict) -> bool:
    if not (root / "yarn.lock").exists():
        return False
    condition = "[ -f package.json ] && command -v yarn >/dev/null 2>&1"
    add_command(result, "dev", wrap(condition, "yarn install", "skip yarn install"))
    lint_cmd = "yarn lint || true"
    test_cmd = "yarn test || true"
    build_cmd = "yarn build || true"
    add_command(result, "verify", wrap(condition, lint_cmd, "skip yarn lint"))
    add_command(result, "verify", wrap(condition, test_cmd, "skip yarn test"))
    add_command(result, "review_linters", wrap(condition, lint_cmd, "skip yarn lint"))
    add_command(result, "ship", wrap(condition, build_cmd, "skip yarn build"))
    add_command(result, "test_candidates", wrap(condition, test_cmd, "skip yarn test"))
    coverage_file = root / "coverage" / "lcov.info"
    if coverage_file.exists():
        add_command(result, "coverage_candidates", str(coverage_file.relative_to(root)))
    return True


def detect_pnpm(root: Path, result: dict) -> bool:
    if not (root / "pnpm-lock.yaml").exists():
        return False
    condition = "[ -f package.json ] && command -v pnpm >/dev/null 2>&1"
    add_command(result, "dev", wrap(condition, "pnpm install", "skip pnpm install"))
    lint_cmd = "pnpm lint || true"
    test_cmd = "pnpm test || true"
    build_cmd = "pnpm build || true"
    add_command(result, "verify", wrap(condition, lint_cmd, "skip pnpm lint"))
    add_command(result, "verify", wrap(condition, test_cmd, "skip pnpm test"))
    add_command(result, "review_linters", wrap(condition, lint_cmd, "skip pnpm lint"))
    add_command(result, "ship", wrap(condition, build_cmd, "skip pnpm build"))
    add_command(result, "test_candidates", wrap(condition, test_cmd, "skip pnpm test"))
    coverage_file = root / "coverage" / "lcov.info"
    if coverage_file.exists():
        add_command(result, "coverage_candidates", str(coverage_file.relative_to(root)))
    return True


def detect_node(root: Path, result: dict) -> None:
    package_json = root / "package.json"
    if not package_json.exists():
        return
    if detect_yarn(root, result):
        return
    if detect_pnpm(root, result):
        return
    condition = "[ -f package.json ] && command -v npm >/dev/null 2>&1"
    add_command(result, "dev", wrap(condition, "npm install", "skip npm install (package.json/npm not available)"))
    lint_cmd = "npm run lint --if-present"
    test_cmd = "npm run test --if-present"
    build_cmd = "npm run build --if-present"

    add_command(result, "verify", wrap(condition, lint_cmd, "skip npm lint"))
    add_command(result, "verify", wrap(condition, test_cmd, "skip npm test"))
    add_command(result, "review_linters", wrap(condition, lint_cmd, "skip npm lint"))
    add_command(result, "ship", wrap(condition, build_cmd, "skip npm build"))
    add_command(result, "test_candidates", wrap(condition, test_cmd, "skip npm test"))

    coverage_file = root / "coverage" / "lcov.info"
    if coverage_file.exists():
        add_command(result, "coverage_candidates", str(coverage_file.relative_to(root)))


def detect_poetry(root: Path, result: dict) -> bool:
    pyproject = root / "pyproject.toml"
    if not pyproject.exists():
        return False

    try:
        pyproject_text = pyproject.read_text(encoding="utf-8", errors="ignore")
    except OSError:
        return False

    if "tool.poetry" not in pyproject_text:
        return False

    condition = "[ -f pyproject.toml ] && command -v poetry >/dev/null 2>&1"
    add_command(result, "dev", wrap(condition, "poetry install", "skip poetry install (missing poetry)"))
    pytest_cmd = "poetry run pytest"
    add_command(result, "verify", wrap(condition, pytest_cmd, "skip pytest (poetry)"))
    add_command(result, "test_candidates", wrap(condition, pytest_cmd, "skip pytest (poetry)"))

    if "[tool.ruff" in pyproject_text or (root / "ruff.toml").exists():
        add_command(result, "review_linters", wrap(condition, "poetry run ruff check", "skip ruff (poetry)"))

    coverage_xml = root / "coverage.xml"
    if coverage_xml.exists():
        add_command(result, "coverage_candidates", str(coverage_xml.relative_to(root)))

    add_command(result, "ship", wrap(condition, "poetry build", "skip poetry build"))
    return True


def detect_python_generic(root: Path, result: dict) -> None:
    requirements = root / "requirements.txt"
    requirements_text = ""
    if requirements.exists():
        condition = "[ -f requirements.txt ] && command -v pip >/dev/null 2>&1"
        add_command(result, "dev", wrap(condition, "pip install -r requirements.txt", "skip pip install (requirements/pip missing)"))
        try:
            requirements_text = requirements.read_text(encoding="utf-8", errors="ignore")
        except OSError:
            requirements_text = ""

    pyproject = root / "pyproject.toml"
    pyproject_text = ""
    if pyproject.exists():
        try:
            pyproject_text = pyproject.read_text(encoding="utf-8", errors="ignore")
        except OSError:
            pyproject_text = ""

    pytest_cond = "command -v pytest >/dev/null 2>&1"
    has_tests_dir = (root / "tests").exists()
    if has_tests_dir or "pytest" in requirements_text or "pytest" in pyproject_text:
        add_command(result, "verify", wrap(pytest_cond, "pytest", "skip pytest (not installed)"))
        add_command(result, "test_candidates", wrap(pytest_cond, "pytest", "skip pytest (not installed)"))

    if "ruff" in requirements_text or "[tool.ruff" in pyproject_text or (root / "ruff.toml").exists():
        add_command(result, "review_linters", wrap("command -v ruff >/dev/null 2>&1", "ruff check", "skip ruff (not installed)"))
    elif "flake8" in requirements_text or "flake8" in pyproject_text:
        add_command(result, "review_linters", wrap("command -v flake8 >/dev/null 2>&1", "flake8", "skip flake8 (not installed)"))

    coverage_xml = root / "coverage.xml"
    if coverage_xml.exists():
        add_command(result, "coverage_candidates", str(coverage_xml.relative_to(root)))


def detect_pipenv(root: Path, result: dict) -> bool:
    if not (root / "Pipfile").exists():
        return False
    condition = "[ -f Pipfile ] && command -v pipenv >/dev/null 2>&1"
    add_command(result, "dev", wrap(condition, "pipenv install --dev", "skip pipenv install"))
    test_cmd = "pipenv run pytest"
    add_command(result, "verify", wrap(condition, test_cmd, "skip pipenv pytest"))
    add_command(result, "test_candidates", wrap(condition, test_cmd, "skip pipenv pytest"))
    add_command(result, "review_linters", wrap(condition, "pipenv run ruff check", "skip pipenv ruff"))
    return True


def detect_go(root: Path, result: dict) -> None:
    if not (root / "go.mod").exists():
        return
    condition = "[ -f go.mod ] && command -v go >/dev/null 2>&1"
    add_command(result, "dev", wrap(condition, "go mod download", "skip go mod download"))
    go_test = "go test ./..."
    add_command(result, "verify", wrap(condition, go_test, "skip go test"))
    add_command(result, "test_candidates", wrap(condition, go_test, "skip go test"))
    add_command(result, "review_linters", wrap(condition + " && command -v golangci-lint >/dev/null 2>&1", "golangci-lint run", "skip golangci-lint"))


def detect_rust(root: Path, result: dict) -> None:
    if not (root / "Cargo.toml").exists():
        return
    condition = "[ -f Cargo.toml ] && command -v cargo >/dev/null 2>&1"
    add_command(result, "dev", wrap(condition, "cargo fetch", "skip cargo fetch"))
    cargo_test = "cargo test"
    add_command(result, "verify", wrap(condition, cargo_test, "skip cargo test"))
    add_command(result, "test_candidates", wrap(condition, cargo_test, "skip cargo test"))
    add_command(result, "review_linters", wrap(condition + " && command -v cargo >/dev/null 2>&1", "cargo fmt -- --check", "skip cargo fmt"))


def detect_gradle(root: Path, result: dict) -> None:
    if not any((root / name).exists() for name in ("gradlew", "gradlew.bat", "build.gradle", "build.gradle.kts")):
        return
    condition = "command -v ./gradlew >/dev/null 2>&1 && ./gradlew --version >/dev/null || command -v gradle >/dev/null 2>&1"
    add_command(result, "dev", wrap(condition, "./gradlew --quiet tasks >/dev/null 2>&1 || gradle --quiet tasks >/dev/null 2>&1", "skip gradle warmup"))
    verify_cmd = "./gradlew check || gradle check"
    add_command(result, "verify", wrap(condition, verify_cmd, "skip gradle check"))
    add_command(result, "ship", wrap(condition, "./gradlew build || gradle build", "skip gradle build"))


def detect_maven(root: Path, result: dict) -> None:
    if not (root / "pom.xml").exists():
        return
    condition = "[ -f pom.xml ] && command -v mvn >/dev/null 2>&1"
    add_command(result, "verify", wrap(condition, "mvn -B verify", "skip mvn verify"))
    add_command(result, "ship", wrap(condition, "mvn -B package", "skip mvn package"))


def detect_dotnet(root: Path, result: dict) -> None:
    projects = list(root.glob("**/*.csproj")) + list(root.glob("**/*.sln"))
    if not projects:
        return
    condition = "command -v dotnet >/dev/null 2>&1"
    add_command(result, "dev", wrap(condition, "dotnet restore", "skip dotnet restore"))
    add_command(result, "verify", wrap(condition, "dotnet test", "skip dotnet test"))
    add_command(result, "ship", wrap(condition, "dotnet build --configuration Release", "skip dotnet build"))


def detect_ruby(root: Path, result: dict) -> None:
    if not (root / "Gemfile").exists():
        return
    condition = "[ -f Gemfile ] && command -v bundle >/dev/null 2>&1"
    add_command(result, "dev", wrap(condition, "bundle install", "skip bundle install"))
    add_command(result, "verify", wrap(condition, "bundle exec rake test", "skip bundle rake"))


def build_snippet(root: Path) -> str:
    result: dict[str, list[str] | str] = {
        "dev": [],
        "verify": [],
        "ship": [],
        "review_linters": [],
        "test_candidates": [],
        "coverage_candidates": [],
    }

    detect_node(root, result)
    poetry_used = detect_poetry(root, result)
    if not poetry_used:
        detect_python_generic(root, result)
    detect_pipenv(root, result)
    detect_go(root, result)
    detect_rust(root, result)
    detect_gradle(root, result)
    detect_maven(root, result)
    detect_dotnet(root, result)
    detect_ruby(root, result)

    lines: list[str] = []
    for key in ("dev", "verify", "ship", "review_linters"):
        values = result.get(key, [])
        if not values:
            continue
        array_name = {
            "dev": "SDK_DEV_COMMANDS",
            "verify": "SDK_VERIFY_COMMANDS",
            "ship": "SDK_SHIP_COMMANDS",
            "review_linters": "SDK_REVIEW_LINTERS",
        }[key]
        joined = " ".join(shlex.quote(v) for v in values)  # type: ignore[arg-type]
        lines.append(f"if [[ ${{#{array_name}[@]}} -eq 0 ]]; then")
        lines.append(f"  {array_name}=({joined})")
        lines.append("fi")

    test_candidates = result.get("test_candidates", [])
    if test_candidates:
        test_cmd = test_candidates[0]
        lines.append("if [[ -z \"${SDK_TEST_COMMAND:-}\" ]]; then")
        lines.append(f"  SDK_TEST_COMMAND={shlex.quote(test_cmd)}")
        lines.append("fi")

    coverage_candidates = result.get("coverage_candidates", [])
    if coverage_candidates:
        cov = coverage_candidates[0]
        lines.append("if [[ -z \"${SDK_COVERAGE_FILE:-}\" ]]; then")
        lines.append(f"  SDK_COVERAGE_FILE={shlex.quote(cov)}")
        lines.append("fi")

    return "\n".join(lines)


def main() -> int:
    if len(sys.argv) < 2:
        return 0
    root = Path(sys.argv[1]).resolve()
    snippet = build_snippet(root)
    if snippet:
        print(snippet)
    return 0


if __name__ == "__main__":  # pragma: no cover
    sys.exit(main())
