# Команды SDK для конкретного проекта.
# Обновите массивы, чтобы привязать SDK к стеку разрабатываемого решения.

SDK_CAPSULE_ROOT="agentcontrol"

_ensure_venv_cmd='(cd "$SDK_CAPSULE_ROOT" && [ -d .venv ] || python3 -m venv .venv)'
_upgrade_cmd='(cd "$SDK_CAPSULE_ROOT" && .venv/bin/pip install --upgrade pip)'
_requirements_cmd='(cd "$SDK_CAPSULE_ROOT" && .venv/bin/pip install --upgrade -r requirements.txt)'
_ensure_reports_cmd='(cd "$SDK_CAPSULE_ROOT" && mkdir -p reports/python)'
_pytest_cmd='(cd "$SDK_CAPSULE_ROOT" && mkdir -p reports/python && PYTHONPATH="$PWD/..:$PYTHONPATH" COVERAGE_FILE=reports/python/.coverage .venv/bin/python -m pytest --cov=agentcontrol --cov=agentcontrol.scripts --cov-report=term --cov-report=xml:reports/python/coverage.xml --maxfail=1 -q || [[ $? -eq 5 ]])'

SDK_DEV_COMMANDS=(
  "$_ensure_venv_cmd"
  "$_upgrade_cmd"
  "$_requirements_cmd"
  "$_ensure_reports_cmd"
  "$_pytest_cmd"
)

SDK_VERIFY_COMMANDS=(
  "$_ensure_venv_cmd"
  "$_upgrade_cmd"
  "$_requirements_cmd"
  "$_ensure_reports_cmd"
  "$_pytest_cmd"
)

SDK_FIX_COMMANDS=()
SDK_SHIP_COMMANDS=()

SDK_TEST_COMMAND="$_pytest_cmd"
SDK_COVERAGE_FILE="reports/python/coverage.xml"
