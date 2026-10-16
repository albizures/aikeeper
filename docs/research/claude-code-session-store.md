# Claude Code session-store discovery

## Findings

- Claude Code stores primary session transcripts at `~/.claude/projects/<encoded-project-path>/<session-id>.jsonl`. The project directory encodes the absolute project path by replacing path separators with `-`.
- The filename stem is the session ID. Records containing `sessionId` repeat that ID; a transcript can begin with non-message records, so discovery must not assume the first record contains folder or timestamp metadata.
- The first normal user record observed supplies `sessionId`, `cwd`, `timestamp`, and `isSidechain: false`. `cwd` is the folder to display.
- For last activity, scan valid JSONL records and select the latest parseable `timestamp`. This also covers non-message events written after the final user/assistant message. Fall back to the transcript's filesystem modification time when no usable timestamp exists.
- `subagents/agent-*.jsonl` files can appear below a primary session directory. They repeat the parent `sessionId` and have `isSidechain: true`; do not treat them as separately listable primary Agent Sessions, or the parent ID is duplicated.
- A missing `~/.claude/projects` directory means the Claude Code store is unavailable and should produce no Claude Code rows. Malformed JSONL files or files without a usable session ID should be skipped.

## Implementation guidance

Enumerate only immediate `.jsonl` files in each immediate project directory below `~/.claude/projects`; do not recurse into `subagents`. Use the filename stem as the ID after validating that a matching `sessionId` is found in the file. Obtain `cwd` and the maximum record timestamp while streaming the file; show `-` for a missing `cwd`.

## Primary sources

- Local Claude Code primary transcript inspected: `~/.claude/projects/-home-a-projects-zubale/05e122c7-a6bc-4102-bbc9-5ee7b59be1a9.jsonl`. The filename and record `sessionId` are both `05e122c7-a6bc-4102-bbc9-5ee7b59be1a9`; its first metadata-bearing user record contains `cwd`, `timestamp`, and `isSidechain: false`.
- Local Claude Code subagent transcript inspected: `~/.claude/projects/-home-a-projects-zubale/737ac227-77d5-4d42-8144-16db532e36da/subagents/agent-afb8cb8c337251529.jsonl`. It has the parent `sessionId` and `isSidechain: true`, establishing that recursive discovery duplicates the primary ID.
- Local store layout inspected under `~/.claude/projects/`; primary transcripts are direct children of encoded project directories.
