# AIKeeper

AIKeeper manages references to sessions owned by external AI agent runtimes.

## Language

**Agent Session**:
A reference to a session owned by an AI agent runtime, identified by the pair of its owning Agent Runtime and that runtime's UUID rather than by an AIKeeper-created record.
_Avoid_: run, conversation, execution

**Agent Runtime**:
An external system that owns Agent Sessions and their UUIDs. The initial supported runtimes are Claude Code and Pi.
_Avoid_: provider, backend
