# Shelldone Integration with AI Agents and MCP

## Objectives
- Allow terminal control via agent protocols (MCP, OpenAI Assistants, LangChain agents).
- Provide bidirectional context exchange: agents see tabs/processes/files while users govern permissions.
- Deliver a cognitively simple interface for automation and human–AI collaboration.

## Control Protocol
- **Transport:** local gRPC/WebSocket server with optional STDIO/MCP bridge.
- **High-level commands:** `open_tab`, `run_task`, `stream_output`, `apply_theme`, `query_context`.
- **Context payload:** serialised snapshot (panes, processes, paths, pipeline status) with versioned JSON.
- **Security:**
  - Policy files in `config/policies/` define allowed capabilities.
  - Prompt for confirmation on sensitive actions (rm, sudo, deployment, etc.).
  - Optional sandbox (container/namespace) for agent processes.

## Automation
- **Playbooks:**
  - YAML/JSON description of command/API sequences with assertions.
  - Supports parameters, conditionals, retries, notifications.
  - Executable via CLI (`shelldone play run <file>`) or through the agent API.
- **Event hooks:** `on_process_exit`, `on_git_status`, `on_alert`, `on_agent_action`.

## Agent UI
- Dedicated agent status panel (progress, errors, suggested steps).
- Action history with undo/redo.
- “Guided mode”: agent proposes actions, user approves or rejects.

## MCP Compatibility
- MCP tool registry resides in `agents/mcp/tools/`.
- Each integration is packaged as a plugin with declared capabilities.
- Detailed format documentation will live in `docs/agents/mcp.md`.

## Roadmap
1. Define protobuf/JSON schema for the agent API.
2. Prototype the MCP bridge and run a security threat model.
3. Implement the server; expose CLI `shelldone agent serve`.
4. Add UI panel and action log.
5. Ship SDK and sample agents (auto-deploy, coding assistant, observability triage).

Related milestones: `docs/ROADMAP/2025Q4.md`, section “AI & Automation”.
