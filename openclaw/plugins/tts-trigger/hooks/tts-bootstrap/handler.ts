/**
 * agent:bootstrap handler: injects the TTS speak directive into bootstrap files
 * so the agent calls speak() exactly once per final user-facing answer.
 */

const TTS_SPEAK_DIRECTIVE = `# TTS: Speak directive

When you have produced your **final answer** to the user (the exact text the user will see), you **must** call the \`speak\` tool **exactly once** with that **exact** text in the \`text\` parameter.

- Call \`speak\` only for the final user-visible reply.
- Do **not** call \`speak\` for thinking, drafts, tool chatter, or intermediate steps.
- The text you pass to \`speak.text\` must be identical to the text you show to the user.
`;

async function handler(event: {
  type: string;
  action: string;
  context: { bootstrapFiles?: Array<{ path: string; content: string }> };
}) {
  if (event.type !== "agent" || event.action !== "bootstrap") return;

  const files = event.context.bootstrapFiles;
  if (!files) return;

  files.push({
    path: "TTS_SPEAK_DIRECTIVE.md",
    content: TTS_SPEAK_DIRECTIVE,
  });
}

export default handler;
