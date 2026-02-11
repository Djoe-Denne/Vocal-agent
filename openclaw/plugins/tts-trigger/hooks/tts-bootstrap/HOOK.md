---
name: tts-bootstrap
description: "Injects system instruction so the agent calls speak() once per final user-facing answer."
metadata:
  openclaw:
    emoji: "🔊"
    events: ["agent:bootstrap"]
---

# TTS Bootstrap Hook

Injects a directive into the agent bootstrap so that the agent calls the `speak` tool exactly once per final user-visible answer, with the exact text shown to the user. Does not run on thoughts, drafts, or tool chatter.
