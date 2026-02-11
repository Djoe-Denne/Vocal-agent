/**
 * TTS Trigger plugin: exposes the `speak` tool and injects a bootstrap directive
 * so the agent calls speak() once per final user-facing answer (fire-and-forget to TTS).
 */

const DEFAULT_TTS_BASE_URL = "http://127.0.0.1:3000";

function getTtsBaseUrl(api: { config?: Record<string, unknown> }): string {
  const fromEnv = process.env.TTS_BASE_URL?.trim();
  if (fromEnv) return fromEnv.replace(/\/$/, "");
  const entries = api.config?.plugins as Record<string, { config?: { baseUrl?: string } }> | undefined;
  const baseUrl = entries?.["tts-trigger"]?.config?.baseUrl?.trim();
  if (baseUrl) return baseUrl.replace(/\/$/, "");
  return DEFAULT_TTS_BASE_URL;
}

function toolResult(ok: boolean, error?: string) {
  const payload = error == null ? { ok: true } : { ok: false, error };
  return { content: [{ type: "text" as const, text: JSON.stringify(payload) }] };
}

export default function register(api: {
  config?: Record<string, unknown>;
  registerTool: (tool: unknown, opts?: { optional?: boolean }) => void;
  registerPluginHooksFromDir?: (path: string) => void;
  logger?: { info?: (msg: string, ...args: unknown[]) => void; error?: (msg: string, ...args: unknown[]) => void };
}) {
  const log = (msg: string, ...args: unknown[]) =>
    (api.logger?.info ?? ((m: string, ...a: unknown[]) => console.log("[tts-trigger]", m, ...a)))(msg, ...args);
  const logError = (msg: string, ...args: unknown[]) =>
    (api.logger?.error ?? ((m: string, ...a: unknown[]) => console.error("[tts-trigger]", m, ...a)))(msg, ...args);

  const ttsBaseUrl = getTtsBaseUrl(api);
  log("plugin registered; TTS base URL: %s", ttsBaseUrl);

  api.registerTool(
    {
      name: "speak",
      description:
        "Send the given text to the TTS server to be spoken aloud. Call this exactly once per final user-facing answer, with the exact text shown to the user. Do not call for thoughts, drafts, or tool chatter.",
      parameters: {
        type: "object",
        properties: {
          text: { type: "string", description: "The exact final answer text shown to the user." },
          session_id: { type: "string", description: "Optional; for correlation/logging and future routing." },
        },
        required: ["text"],
      },
      async execute(_id: unknown, params: { text?: string; session_id?: string }) {
        const text = typeof params?.text === "string" ? params.text.trim() : "";
        const sessionId = typeof params?.session_id === "string" ? params.session_id : undefined;
        log("speak called: text length=%d, session_id=%s", text.length, sessionId ?? "(none)");
        if (!text) {
          logError("speak rejected: empty text");
          return toolResult(false, "empty text");
        }

        const url = `${ttsBaseUrl}/v1/audio/speech`;

        const fireAndForget = () => {
          log("TTS request dispatched to %s", url);
          fetch(url, {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({ input: text }),
            signal: AbortSignal.timeout(5000),
          }).catch((err) => {
            logError("TTS request failed: %s", err instanceof Error ? err.message : String(err));
          });
        };

        try {
          fireAndForget();
        } catch (err) {
          logError("TTS dispatch failed: %s", err instanceof Error ? err.message : String(err));
        }

        return toolResult(true);
      },
    },
    { optional: true }
  );

  if (typeof api.registerPluginHooksFromDir === "function") {
    api.registerPluginHooksFromDir("./hooks");
  }
}
