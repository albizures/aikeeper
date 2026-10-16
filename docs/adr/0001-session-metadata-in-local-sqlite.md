# Store session metadata in local SQLite

AIKeeper will retain user-owned Session Metadata in one SQLite database, keyed by Agent Runtime and runtime UUID. Its path is `$XDG_DATA_HOME/aikeeper/aikeeper.sqlite3`, or `~/.local/share/aikeeper/aikeeper.sqlite3` when `XDG_DATA_HOME` is unset; this preserves metadata across working directories without changing session stores owned by external Agent Runtimes.
