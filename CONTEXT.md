# AIKeeper

AIKeeper manages references to sessions owned by external AI agent runtimes.

## Language

**Agent Session**:
A reference to a session owned by an AI agent runtime, identified by its owning Agent Runtime and that runtime's UUID. AIKeeper discovers it from the runtime's session store rather than creating or persisting its own record.
_Avoid_: run, conversation, execution

**Agent Runtime**:
An external system that owns Agent Sessions and their UUIDs. The initial supported runtimes are Claude Code and Pi.
_Avoid_: provider, backend

**Working Directory**:
The filesystem path in which an Agent Session runs, as recorded by its Agent Runtime.
_Avoid_: folder, project
