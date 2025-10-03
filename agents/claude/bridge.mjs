#!/usr/bin/env node
// Shelldone ↔ Claude Agent SDK bridge.

import readline from "node:readline";
import process from "node:process";

let Anthropic;
try {
  ({ Anthropic } = await import("@anthropic-ai/sdk"));
} catch (error) {
  // Печатаем структурированную ошибку и завершаем работу.
  const message =
    "Пакет @anthropic-ai/sdk не установлен. Выполните `npm ci` в agents/claude.";
  process.stdout.write(
    JSON.stringify({ status: "error", error: message }) + "\n",
  );
  process.exit(1);
}

class ClaudeAdapter {
  constructor({ instructions, model, maxTokens }) {
    const apiKey = process.env.ANTHROPIC_API_KEY;
    if (!apiKey) {
      throw new Error("Переменная окружения ANTHROPIC_API_KEY не задана");
    }
    this.client = new Anthropic({ apiKey });
    this.instructions =
      instructions || "You are a helpful assistant working inside Shelldone.";
    this.model = model || "claude-3-5-sonnet-latest";
    this.maxTokens = maxTokens || 4000;
    this.sessions = new Map();
  }

  _historyFor(sessionId) {
    if (!sessionId) return []; // без памяти
    if (!this.sessions.has(sessionId)) {
      this.sessions.set(sessionId, []);
    }
    return this.sessions.get(sessionId);
  }

  async run(input, sessionId) {
    if (typeof input !== "string" || input.length === 0) {
      throw new Error("Команда run требует строковый input");
    }

    const history = this._historyFor(sessionId);
    const messages = [
      { role: "system", content: this.instructions },
      ...history,
      { role: "user", content: input },
    ];

    const response = await this.client.messages.create({
      model: this.model,
      max_tokens: this.maxTokens,
      messages,
    });

    const assistantText =
      response?.content?.[0]?.text ?? "(empty response from Claude)";

    if (sessionId) {
      history.push({ role: "user", content: input });
      history.push({ role: "assistant", content: assistantText });
      this.sessions.set(sessionId, history);
    }

    return assistantText;
  }
}

function emit(payload) {
  process.stdout.write(JSON.stringify(payload) + "\n");
}

async function main() {
  let adapter;
  try {
    adapter = new ClaudeAdapter({});
  } catch (error) {
    emit({ status: "error", error: String(error.message || error) });
    return 1;
  }

  emit({ status: "ready", model: adapter.model });

  const rl = readline.createInterface({
    input: process.stdin,
    crlfDelay: Infinity,
  });

  for await (const rawLine of rl) {
    const line = rawLine.trim();
    if (!line) continue;

    let message;
    try {
      message = JSON.parse(line);
    } catch (error) {
      emit({ status: "error", error: `invalid JSON: ${error}` });
      continue;
    }

    if (message.type === "shutdown") {
      emit({ status: "ok", message: "shutdown" });
      rl.close();
      return 0;
    }

    if (message.type !== "run") {
      emit({ status: "error", error: `unknown command ${message.type}` });
      continue;
    }

    try {
      const output = await adapter.run(message.input, message.session);
      emit({ status: "ok", output, session: message.session ?? null });
    } catch (error) {
      emit({ status: "error", error: String(error.message || error) });
    }
  }

  return 0;
}

const exitCode = await main();
process.exit(exitCode);
