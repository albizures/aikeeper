---
name: aikeeper
description: Manage the current agent session's title and tags with AIKeeper.
disable-model-invocation: true
---

# AIKeeper Current-Session Metadata

Manage the **Session Title** or **Tags** of the **Current Agent Session** with AIKeeper.

AIKeeper metadata is user-owned and separate from the agent runtime's session store. Never modify runtime session files.

## Scope and identity

Operate on exactly one session: the Current Agent Session. Never ask for, accept, infer from a path, or use a user-supplied session UUID. Do not list sessions to choose a target.

Resolve the runtime and UUID from this host's native current-session context immediately before a mutation:

- **Pi:** use `PI_SESSION_ID`, which Pi injects into LLM-callable shell tools. The runtime argument is `pi`. If `PI_SESSION_ID` is empty or unavailable, stop and explain that Pi did not provide current-session metadata.
- **Claude Code:** use the current Claude Code session UUID supplied in Claude Code's native current-session context. The runtime argument is `claude-code`. If that context does not expose a UUID, stop and explain that it cannot safely target the Current Agent Session.
- **Any other host:** stop; this skill supports only Pi and Claude Code.

Use the concrete UUID just resolved, not a placeholder or a value from the user. Do not fall back to searching `~/.pi`, `~/.claude`, transcripts, or another session store.

Before any mutation, verify `aikeeper` is available on `PATH` (for example, with `command -v aikeeper`). If unavailable, explain the prerequisite and make no change.

## Request handling

- Persist an explicit, unambiguous title or tag request directly.
- Clarify an ambiguous explicit request before changing metadata.
- When the user asks to organize, summarize, or otherwise infer metadata, show the proposed Session Title and/or Tags and wait for approval before writing.
- Replacing all Tags always requires showing the exact proposed resulting Tags and receiving confirmation, even when the requested tags are explicit.
- Preserve exact user-provided Session Title text. AIKeeper validates Title and Tag constraints; report its validation error rather than silently changing values.

Construct each command as an argument vector: quote literal user text, use `--` before a title, and never construct a shell command with interpolation, `eval`, or concatenated command text.

## Mutations

After resolving the current runtime and UUID, issue the applicable AIKeeper command with those concrete values:

- Set a Session Title: `aikeeper title set --agent AGENT --id CURRENT_UUID -- TITLE`
- Clear a Session Title: `aikeeper title clear --agent AGENT --id CURRENT_UUID`
- Add a Tag without removing existing Tags: `aikeeper tag add --agent AGENT --id CURRENT_UUID TAG`
- Remove a Tag: `aikeeper tag remove --agent AGENT --id CURRENT_UUID TAG`

`AGENT`, `CURRENT_UUID`, `TITLE`, and `TAG` above describe values that the host must supply as separate, safely quoted command arguments; they are not user-facing inputs or automatic shell interpolation. `AGENT` must be either `pi` or `claude-code`, and `CURRENT_UUID` must be the just-resolved native current-session UUID.

For a confirmed Tag replacement:

1. Determine the Current Agent Session's existing Tags using AIKeeper's normal discovery output, matching only the already-resolved `(Agent Runtime, runtime UUID)` pair. Do not take the existing Tags or target UUID from the user.
2. Show the proposed final Tag set and wait for confirmation.
3. Remove each existing Tag not in that final set with `aikeeper tag remove`, then add each requested Tag not already present with `aikeeper tag add`.
4. Stop at the first failed command. Report which changes completed and which command failed; do not attempt rollback or issue remaining mutations.

For any request involving several commands, follow the same stop-on-first-failure rule and report the persisted changes accurately. For a successful command, state the change that AIKeeper persisted.
