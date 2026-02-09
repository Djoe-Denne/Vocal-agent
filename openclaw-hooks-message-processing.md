# OpenClaw Plugin — Hooks (Per-Message Processing)

> **Use case:** A task that runs on each message to be displayed to the user, allowing transformation, logging, or side effects before delivery.

---

## Approach: Hooks System (event-driven)

OpenClaw's hooks system provides event-driven automation that fires during command processing, agent lifecycle, and message flow. Hooks can be standalone directories or bundled inside a plugin.

**Two ways to implement:**

| Method                              | Best for                                         |
| ----------------------------------- | ------------------------------------------------ |
| Standalone hook directory           | Simple, single-purpose hook                      |
| Plugin-bundled hooks via `registerPluginHooksFromDir` | Part of a larger plugin with services/tools |

---

## Standalone Hook Structure

```
~/.openclaw/hooks/message-processor/
├── HOOK.md          # Metadata + documentation (YAML frontmatter)
└── handler.ts       # Handler implementation
```

### Hook Discovery Directories (precedence order)

| Location                              | Scope                          | Precedence |
| ------------------------------------- | ------------------------------ | ---------- |
| `<workspace>/hooks/`                  | Per-agent                      | Highest    |
| `~/.openclaw/hooks/`                  | Shared across workspaces       | Middle     |
| `<openclaw>/dist/hooks/bundled/`      | Shipped with OpenClaw          | Lowest     |
| `hooks.internal.load.extraDirs` paths | Additional directories         | Configurable |

---

## HOOK.md Format

YAML frontmatter + Markdown documentation.

```markdown
---
name: message-processor
description: "Processes each message before display to the user"
metadata:
  {
    "openclaw": {
      "emoji": "📨",
      "events": ["message:sent", "message:received"],
      "requires": {
        "bins": [],
        "env": [],
        "config": []
      }
    }
  }
---

# Message Processor Hook

Intercepts and processes messages before they are displayed to the user.

## What It Does

- Listens for message events
- Transforms or annotates message content
- Optionally injects additional messages
```

### `metadata.openclaw` Fields

| Field      | Type     | Description                                           |
| ---------- | -------- | ----------------------------------------------------- |
| `emoji`    | string   | Display emoji for CLI (e.g. `"📨"`)                   |
| `events`   | string[] | Array of events to listen for                         |
| `export`   | string   | Named export to use (default: `"default"`)            |
| `homepage` | string   | Documentation URL                                     |
| `always`   | boolean  | Bypass eligibility checks                             |
| `install`  | array    | Installation methods                                  |
| `requires` | object   | Requirements object (see below)                       |

### `requires` Sub-fields

| Field     | Type     | Description                                              |
| --------- | -------- | -------------------------------------------------------- |
| `bins`    | string[] | Required binaries on PATH (e.g. `["node", "git"]`)      |
| `anyBins` | string[] | At least one of these binaries must be present           |
| `env`     | string[] | Required environment variables                           |
| `config`  | string[] | Required config paths (e.g. `["workspace.dir"]`)         |
| `os`      | string[] | Required platforms (e.g. `["darwin", "linux"]`)           |

---

## Event Types

### Currently Available

| Event              | Type      | Fires when                                              |
| ------------------ | --------- | ------------------------------------------------------- |
| `command`          | command   | Any command event (general listener)                    |
| `command:new`      | command   | `/new` command issued                                   |
| `command:reset`    | command   | `/reset` command issued                                 |
| `command:stop`     | command   | `/stop` command issued                                  |
| `agent:bootstrap`  | agent     | Before workspace bootstrap files are injected           |
| `gateway:startup`  | gateway   | After channels start and hooks are loaded               |

### Planned / Future Events

| Event              | Fires when                                 |
| ------------------ | ------------------------------------------ |
| `message:sent`     | When a message is sent to the user         |
| `message:received` | When a message is received from the user   |
| `session:start`    | When a new session begins                  |
| `session:end`      | When a session ends                        |
| `agent:error`      | When an agent encounters an error          |

> **Important:** `message:sent` and `message:received` are listed as **planned** in the docs. Check the current OpenClaw release for availability. If not yet available, use `agent:bootstrap` to mutate `context.bootstrapFiles` before the system prompt is assembled, or use the **Plugin API's `tool_result_persist` hook** (synchronous, transforms tool results before they are written to the transcript).

---

## Event Context Object

Every hook handler receives this event object:

```typescript
interface HookEvent {
  type: "command" | "session" | "agent" | "gateway";
  action: string;              // e.g. "new", "reset", "stop"
  sessionKey: string;          // Session identifier
  timestamp: Date;             // When the event occurred
  messages: string[];          // Push messages here to send to user
  context: {
    sessionEntry?: SessionEntry;
    sessionId?: string;
    sessionFile?: string;
    commandSource?: string;    // e.g. "whatsapp", "telegram"
    senderId?: string;
    workspaceDir?: string;
    bootstrapFiles?: WorkspaceBootstrapFile[];  // Mutable on agent:bootstrap
    cfg?: OpenClawConfig;
  };
}
```

### Key Context Fields

| Field                       | Available on              | Description                                 |
| --------------------------- | ------------------------- | ------------------------------------------- |
| `event.messages`            | All events                | Push strings here → sent to user            |
| `context.bootstrapFiles`    | `agent:bootstrap`         | Mutable array of workspace files (SOUL.md, etc.) |
| `context.sessionEntry`      | command events            | Pre-reset session entry (for transcript access) |
| `context.commandSource`     | command events            | Channel name: `"whatsapp"`, `"telegram"`, etc. |
| `context.senderId`          | command events            | Sender's ID                                  |
| `context.workspaceDir`      | when workspace configured | Path to agent workspace                      |
| `context.cfg`               | All events                | Current OpenClaw config                      |

---

## Handler Implementation — `handler.ts`

```typescript
import type { HookHandler } from "../../src/hooks/hooks.js";

const handler: HookHandler = async (event) => {
  // 1. Filter events early — return if not relevant
  if (event.type !== "command" || event.action !== "new") {
    return;
  }

  // 2. Your processing logic
  console.log(`[message-processor] Event: ${event.type}:${event.action}`);
  console.log(`  Session: ${event.sessionKey}`);
  console.log(`  Source: ${event.context.commandSource}`);
  console.log(`  Sender: ${event.context.senderId}`);

  // 3. Optionally push messages to user
  event.messages.push("📨 Message processed by hook!");

  // 4. Optionally mutate bootstrapFiles (only on agent:bootstrap)
  if (event.context.bootstrapFiles) {
    event.context.bootstrapFiles.push({
      path: "CUSTOM_CONTEXT.md",
      content: "# Extra context injected by hook\n\nSome data here.",
    });
  }
};

export default handler;
```

---

## Tool Result Hook — `tool_result_persist` (Plugin API only)

This is a **synchronous** hook that transforms tool results before they are written to the session transcript. Not an event-stream listener.

This is only available via the **Plugin API** (not standalone hooks).

```typescript
// Inside a plugin's register function
export default function (api: any) {
  api.registerToolResultHook?.("tool_result_persist", (toolResult) => {
    // Must be synchronous
    // Return the updated tool result payload, or undefined to keep as-is

    if (toolResult.content?.[0]?.text) {
      // Example: annotate all tool results
      toolResult.content[0].text += "\n[Processed by message-processor]";
    }

    return toolResult;
  });
}
```

---

## `agent:bootstrap` — Mutating System Prompt Content

The most powerful currently-available hook for per-message customization. Fires before workspace bootstrap files are injected into the system prompt.

```typescript
const handler: HookHandler = async (event) => {
  if (event.type !== "agent" || event.action !== "bootstrap") return;

  const files = event.context.bootstrapFiles;
  if (!files) return;

  // Modify existing files
  const soul = files.find((f) => f.path === "SOUL.md");
  if (soul) {
    soul.content += "\n\n## Extra Directive\nAlways greet the user warmly.";
  }

  // Inject a new file
  files.push({
    path: "CUSTOM_INJECTION.md",
    content: `# Dynamic Context\nTimestamp: ${new Date().toISOString()}\nSender: ${event.context.senderId ?? "unknown"}`,
  });
};

export default handler;
```

---

## Plugin-Bundled Hooks

If your hook is part of a larger plugin, bundle it using `registerPluginHooksFromDir`:

```typescript
import { registerPluginHooksFromDir } from "openclaw/plugin-sdk";

export default function register(api: any) {
  // Register hooks from a directory inside the plugin
  registerPluginHooksFromDir(api, "./hooks");

  // Also register services, tools, etc.
  api.registerService({ id: "my-service", start: () => {}, stop: () => {} });
}
```

Plugin hook directory structure:

```
my-plugin/
├── openclaw.plugin.json
├── index.ts
└── hooks/
    └── message-processor/
        ├── HOOK.md
        └── handler.ts
```

### Plugin-Bundled Hook Notes

- Hook directories follow the normal hook structure (`HOOK.md` + `handler.ts`).
- Eligibility rules still apply (OS/bins/env/config).
- Show up in `openclaw hooks list` with `plugin:<id>` prefix.
- Cannot be independently enabled/disabled via `openclaw hooks` — enable/disable the parent plugin instead.

---

## Configuration (`~/.openclaw/openclaw.json`)

### Enable Hooks System

```json5
{
  hooks: {
    internal: {
      enabled: true,                    // Master toggle
      entries: {
        "message-processor": {
          enabled: true,
          env: {                        // Optional per-hook env vars
            MY_CUSTOM_VAR: "value"
          }
        }
      },
      load: {
        extraDirs: ["/path/to/more/hooks"]  // Optional extra directories
      }
    }
  }
}
```

### Per-Hook Config Fields

| Field     | Type    | Description                        |
| --------- | ------- | ---------------------------------- |
| `enabled` | boolean | Toggle this hook on/off            |
| `env`     | object  | Key-value env vars for this hook   |

---

## CLI Commands

```bash
# List all discovered hooks
openclaw hooks list

# List only eligible hooks
openclaw hooks list --eligible

# Verbose (show missing requirements)
openclaw hooks list --verbose

# JSON output
openclaw hooks list --json

# Detailed info about a hook
openclaw hooks info message-processor

# Check eligibility summary
openclaw hooks check

# Enable / Disable
openclaw hooks enable message-processor
openclaw hooks disable message-processor

# Install a hook pack from npm
openclaw hooks install <path-or-spec>
```

---

## Hook Packs (npm/archives)

Hook packs are npm packages that export one or more hooks:

**`package.json`:**

```json
{
  "name": "@acme/my-hooks",
  "version": "0.1.0",
  "openclaw": {
    "hooks": ["./hooks/message-processor", "./hooks/other-hook"]
  }
}
```

Each entry points to a hook directory containing `HOOK.md` and `handler.ts`.

Install with:

```bash
openclaw hooks install @acme/my-hooks
```

---

## Best Practices

### Keep Handlers Fast

Hooks run during command processing. Don't block:

```typescript
// ✅ Good — fire and forget
const handler: HookHandler = async (event) => {
  void processInBackground(event);
};

// ❌ Bad — blocks command processing
const handler: HookHandler = async (event) => {
  await slowDatabaseQuery(event);
  await evenSlowerAPICall(event);
};
```

### Handle Errors Gracefully

Never throw — let other handlers run:

```typescript
const handler: HookHandler = async (event) => {
  try {
    await riskyOperation(event);
  } catch (err) {
    console.error("[message-processor] Failed:",
      err instanceof Error ? err.message : String(err));
    // Don't throw
  }
};
```

### Filter Events Early

Return immediately if the event isn't relevant:

```typescript
const handler: HookHandler = async (event) => {
  if (event.type !== "command" || event.action !== "new") return;
  // ...
};
```

### Use Specific Event Keys

```yaml
# ✅ Specific — less overhead
metadata: { "openclaw": { "events": ["command:new"] } }

# ⚠️ General — fires on ALL command events
metadata: { "openclaw": { "events": ["command"] } }
```

---

## Testing

### Unit Test Hooks

```typescript
import { test } from "vitest";
import { createHookEvent } from "./src/hooks/hooks.js";
import myHandler from "./hooks/message-processor/handler.js";

test("handler processes message", async () => {
  const event = createHookEvent("command", "new", "test-session", {
    senderId: "+1234567890",
    commandSource: "whatsapp",
  });

  await myHandler(event);

  // Assert side effects
  expect(event.messages).toContain("📨 Message processed by hook!");
});
```

### Gateway Logs

```bash
# Monitor hook execution
tail -f ~/.openclaw/gateway.log | grep hook

# macOS
./scripts/clawlog.sh -f
```

---

## Debugging

```bash
# Check discovery
openclaw hooks list --verbose

# Check eligibility
openclaw hooks info message-processor

# In handler, log when triggered
console.log("[message-processor] Triggered:", event.type, event.action);
```

### Common Issues

| Problem               | Fix                                                    |
| --------------------- | ------------------------------------------------------ |
| Hook not discovered   | Check directory structure: needs `HOOK.md` + `handler.ts` |
| Hook not eligible     | `openclaw hooks info <name>` — check missing bins/env/config |
| Hook not executing    | Verify enabled + restart Gateway                       |
| Handler errors        | Check TypeScript/import errors, wrap in try/catch      |

---

## Summary — Which Mechanism to Use

| What you want to intercept                           | Mechanism                    | Status    |
| ---------------------------------------------------- | ---------------------------- | --------- |
| Mutate system prompt before each agent turn          | `agent:bootstrap` hook       | Available |
| Transform tool results before transcript persistence | `tool_result_persist` (Plugin API) | Available |
| React to `/new`, `/reset`, `/stop`                   | `command:*` hooks            | Available |
| React on Gateway start                               | `gateway:startup` hook       | Available |
| Intercept each inbound message                       | `message:received` hook      | Planned   |
| Intercept each outbound message                      | `message:sent` hook          | Planned   |

> If `message:sent`/`message:received` are not yet available in your version, use `agent:bootstrap` to inject per-turn context, or `tool_result_persist` to transform tool outputs before they reach the transcript.
