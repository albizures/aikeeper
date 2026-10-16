# AIKeeper

AIKeeper manages references to sessions owned by external AI agent runtimes.

## Language

**Agent Session**:
A reference to a session owned by an AI agent runtime, identified by its owning Agent Runtime and that runtime's UUID. AIKeeper discovers it from the runtime's session store; its optional Session Metadata is stored separately and does not make AIKeeper the session owner.
_Avoid_: run, conversation, execution

**Session Metadata**:
User-owned information attached to an Agent Session by its Agent Runtime and runtime UUID, independently of the runtime's session store.
_Avoid_: session record, annotation

**Session Title**:
An optional, trimmed, nonempty user-defined label for one Agent Session. Its case and internal whitespace are preserved; it contains no tab or line-break characters.
_Avoid_: name, runtime title

**Tag**:
A trimmed, nonempty, lowercase user-defined label that may be attached to zero or more Agent Sessions; an Agent Session may have zero or more distinct Tags. It contains no comma, tab, or line-break characters.
_Avoid_: category, label

**Agent Runtime**:
An external system that owns Agent Sessions and their UUIDs. The initial supported runtimes are Claude Code and Pi.
_Avoid_: provider, backend

**Working Directory**:
The filesystem path in which an Agent Session runs, as recorded by its Agent Runtime.
_Avoid_: folder, project
