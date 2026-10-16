# AIKeeper

AIKeeper lists Agent Sessions discovered from Claude Code and Pi. It keeps optional user-owned Session Metadata in a separate local SQLite database; it never modifies an Agent Runtime's session store.

## Commands

```text
aikeeper list [--agent claude-code|pi] [--folder PATH] [--title TEXT] [--tag TAG]... [--sort date] [--limit POSITIVE_INTEGER]
aikeeper interactive [--agent claude-code|pi] [--folder PATH] [--title TEXT] [--tag TAG]... [--sort date] [--limit POSITIVE_INTEGER]
aikeeper title set --agent claude-code|pi --id ID [--] TITLE
aikeeper title clear --agent claude-code|pi --id ID
aikeeper tag add --agent claude-code|pi --id ID TAG
aikeeper tag remove --agent claude-code|pi --id ID TAG
```

`interactive` accepts the same filters as `list`: `--agent`, `--folder`, `--title`, repeated `--tag`, `--sort date`, and `--limit`. It only runs when both stdin and stdout are TTYs; use `list` in pipelines or redirected output.

## Interactive mode

The table shows the Agent Runtime, Working Directory, Session Title, Tags, and runtime UUID for the filtered Agent Sessions.

| Key | Action |
| --- | --- |
| Up/Down or `j`/`k` | Move selection |
| Home/End | Select first/last session |
| Enter | Open the selected session's actions |
| `?` | Show help |
| `r` | Reload the filtered discovery result |
| `q` or Escape | Quit (or close the current dialog) |

From the action dialog, edit or clear a Session Title, add a Tag, remove a selected existing Tag, or copy the runtime UUID. Title and Tag edits use the same validation as the noninteractive commands and are persisted immediately. A session must still be discoverable when metadata is changed.

Reload retains the selected `(Agent Runtime, runtime UUID)` when it is still present. Clipboard support is optional: if the system clipboard is unavailable, AIKeeper displays the UUID so it can be copied manually without leaving the UI.

## Limitations

Interactive mode uses a terminal alternate screen and needs at least an 80×12 terminal. A smaller terminal displays a resize message. AIKeeper can read only supported local Claude Code and Pi session stores, and it does not edit those runtime-owned stores.
