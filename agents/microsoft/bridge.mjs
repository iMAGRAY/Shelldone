#!/usr/bin/env node
// Shelldone ↔ Microsoft Agent SDK bridge.

import readline from "node:readline";
import process from "node:process";

let MicrosoftAgents;
try {
  MicrosoftAgents = await import("@microsoft/agents-sdk");
} catch (error) {
  const message =
    "Пакет @microsoft/agents-sdk не установлен. Выполните `npm ci` в agents/microsoft.";
  process.stdout.write(
    JSON.stringify({ status: "error", error: message }) + "\n",
  );
  process.exit(1);
}

const DEFAULT_SYSTEM_PROMPT =
  "You are a Microsoft Copilot agent operating inside Shelldone Terminal.";

class MicrosoftAdapter {
  constructor({ instructions, model }) {
    this.apiKey = process.env.MICROSOFT_AGENT_API_KEY;
    if (!this.apiKey) {
      throw new Error(
        "Переменная окружения MICROSOFT_AGENT_API_KEY не задана",
      );
    }

    this.instructions = instructions || DEFAULT_SYSTEM_PROMPT;
    this.model = model || "gpt-4o-mini";
    this.sessions = new Map();

    this.sdk = MicrosoftAgents ?? {};
    this.client = this._maybeCreateClient();
  }

  _maybeCreateClient() {
    try {
      if (this.sdk?.createClient) {
        return this.sdk.createClient({ apiKey: this.apiKey, model: this.model });
      }
      if (this.sdk?.MicrosoftAgentClient) {
        return new this.sdk.MicrosoftAgentClient({
          apiKey: this.apiKey,
          defaultModel: this.model,
        });
      }
    } catch (error) {
      throw new Error(`Инициализация Microsoft Agent SDK не удалась: ${error}`);
    }
    return null;
  }

  _history(sessionId) {
    if (!sessionId) {
      return [];
    }
    if (!this.sessions.has(sessionId)) {
      this.sessions.set(sessionId, []);
    }
    return this.sessions.get(sessionId);
  }

  async run(input, sessionId) {
    if (typeof input !== "string" || input.length === 0) {
      throw new Error("Команда run требует строковый input");
    }

    const history = this._history(sessionId);

    if (this.client?.messages?.create) {
      const response = await this.client.messages.create({
        instructions: this.instructions,
        input,
        session: sessionId,
      });
      const text =
        response?.output?.text ?? response?.content ?? "(empty response)";
      if (sessionId) {
        history.push({ role: "user", content: input });
        history.push({ role: "assistant", content: text });
      }
      return typeof text === "string" ? text : JSON.stringify(text);
    }

    // Fallback behaviour if SDK surface is unknown.
    const echo = `[MS-Agent] ${input}`;
    if (sessionId) {
      history.push({ role: "user", content: input });
      history.push({ role: "assistant", content: echo });
    }
    return echo;
  }
}

function emit(payload) {
  process.stdout.write(JSON.stringify(payload) + "\n");
}

async function main() {
  let adapter;
  try {
    adapter = new MicrosoftAdapter({});
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
