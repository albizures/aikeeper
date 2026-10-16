# Pi session-store discovery

## Findings

- Pi stores default sessions under `~/.pi/agent/sessions/<encoded-cwd>/`. The encoded directory is `--` + the absolute working directory with separators replaced by `-` + `--`.
- Each persisted session is a `.jsonl` file. Its first record is a `type: "session"` header containing at least `id`, `timestamp`, and `cwd`.
- The Agent Session ID is the header's `id`; the folder is the header's `cwd`. Old sessions may have a missing or empty `cwd`.
- Pi's own `SessionManager.listAll()` enumerates every immediate directory under the sessions root, reads `.jsonl` files, and returns sessions sorted descending by `modified`.
- Pi derives `modified` from the latest user or assistant message activity. It uses the message timestamp when present, then the entry timestamp, then falls back to the session-header timestamp, then filesystem modification time.
- Discovery is best-effort: absent session roots return no sessions; unreadable, malformed, oversized-header, or otherwise invalid files are skipped without preventing discovery of other files.

## Implementation guidance

Use the first JSONL record to obtain `id` and `cwd`; scan each session file for the latest valid user/assistant message timestamp to implement the requested date ordering. Preserve Pi's fallback sequence above, or use the file's modification time when no runtime timestamp is usable. Treat a missing `~/.pi/agent/sessions` directory as an unavailable Pi store, not an error.

## Primary sources

- Installed Pi source: `/home/a/.volta/tools/image/packages/@earendil-works/pi-coding-agent/lib/node_modules/@earendil-works/pi-coding-agent/dist/core/session-manager.js`, `getDefaultSessionDirPath` (lines 290–301): default directory construction.
- Same source, `SessionManager.newSession` (lines 665–687): session header fields and filename structure.
- Same source, `SessionManager.listAll` (lines 1445–1495): global directory enumeration and best-effort error behavior.
- Same source, `buildSessionInfo` and `sortSessionInfos` (lines 492–586): metadata parsing, modified-time fallbacks, and descending sort.
- Local Pi session file inspected: `~/.pi/agent/sessions/--home-a-projects-zubale-skill-origin--/2026-09-14T17-23-53-301Z_01a0a0f2-5d54-7541-bf30-53920661270a.jsonl`; its header has `type`, `version`, `id`, `timestamp`, and `cwd`.
