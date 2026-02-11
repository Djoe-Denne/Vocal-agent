WAITING FOR "message:send" COMMAND

# TTS Trigger Plugin

OpenClaw plugin that triggers Text-to-Speech when the agent produces a final user-visible response. The agent calls the `speak` tool once per answer; the plugin sends the text to the TTS HTTP endpoint fire-and-forget (no blocking, errors logged only).

## Behavior

- **Tool `speak`**: Sends `text` (and optional `session_id`) to the TTS server. Call exactly once per final user-facing answer with the exact text shown to the user.
- **Bootstrap hook**: Injects a system directive so the agent is instructed to call `speak()` only for the final reply, not for thoughts, drafts, or tool chatter.

## Configuration

- **TTS base URL**: Set via plugin config or environment.
  - Config: `plugins.entries.tts-trigger.config.baseUrl` (e.g. `http://127.0.0.1:3000`).
  - Environment: `TTS_BASE_URL` (overrides config).
- Default base URL if unset: `http://127.0.0.1:3000`.

## Enablement

1. **Load the plugin**: Add the plugin path to `plugins.load.paths` in your OpenClaw config (e.g. path to this directory).
2. **Enable the plugin**: In `plugins.entries.tts-trigger` set `enabled: true` and optionally `config: { baseUrl: "..." }`.
3. **Allow the tool**: Because the tool is registered as optional, add `speak` or `tts-trigger` to the agent tool allowlist (e.g. `agents.list[].tools.allow` or global `tools.allow`) so the agent can call it.

Restart the Gateway after config changes.

## HTTP contract (plugin → TTS)

- **Method**: POST  
- **URL**: `${baseUrl}/v1/audio/speech`  
- **Body**: `{ "input": "<text>" }` (JSON)

No audio streaming or response handling; the request is fire-and-forget.
