# Ratatui interactive-mode integration

## Findings

- Ratatui's official getting-started tutorial uses `ratatui::run(app)` and a `DefaultTerminal`; it also documents `ratatui::init()` and `ratatui::restore()` for applications that manage terminal setup explicitly. Use the explicit lifecycle behind one CLI entry point so terminal restoration can be attempted on every return path.
- Ratatui's `TableState` exposes selection methods including `select`, `select_first`, `select_last`, `select_next`, and `select_previous`. It is suitable for the selected-row and keyboard-navigation state; keep the stable Agent Session identity separately so a reload can reselect by `(Agent Runtime, runtime UUID)` rather than row index.
- Crossterm's official event documentation provides synchronous `poll` and `read` for keyboard, mouse, and resize events. It guarantees `read` will not block immediately after a successful `poll`, but says not to mix those functions across threads or with `EventStream`. A single-threaded draw/event loop is therefore the simplest fit.
- `arboard::Clipboard` is an OS-independent clipboard handle with `Clipboard::new` and `set_text`. Its documentation describes platform-specific behavior, including Linux dependencies. Clipboard construction or write may therefore fail at runtime and must remain a nonfatal UI action that displays the UUID for manual copying.
- The CLI crate currently has only workspace crates as dependencies. It owns argument dispatch, discovery, persistence wiring, and the existing mutation validation, so it is the appropriate dependency boundary for Ratatui/Crossterm/clipboard crates. Existing app parsing helpers and `MetadataStore` calls should remain the source of Title/Tag normalization and persistence.

## Recommendation

Add the interactive dependencies only to `crates/cli`. Implement a small terminal-lifecycle wrapper that enters raw/alternate-screen mode before the event loop and restores the terminal in a finally-style path. Use a pure UI state reducer where feasible: it receives a domain event (key, resize, reload result, mutation result, clipboard result), changes selection/modal/status state, and emits an effect request. Keep discovery and store calls outside rendering so reducer and rendering behavior can be tested without a terminal.

Adopt the current compatible Ratatui release and its Crossterm backend when implementation begins, and pin the resolved versions in `Cargo.lock`. Add `arboard` only for copy support; treat `Clipboard::new` and `set_text` errors as status/modal outcomes, not application errors.

## Risks and implications

- A panic or early return after entering raw mode can leave a user's terminal unusable. The lifecycle wrapper must restore the terminal on normal error paths; consider a panic hook only if its restoration behavior can be tested safely.
- Terminal dimensions must be checked before the normal table layout. Render only the agreed minimum-size message until sufficient space returns.
- A text-input modal needs key handling distinct from global shortcuts, including Esc dismissal and validation feedback. It should consume its own keys before table navigation.
- Reload must preserve selection by Agent Session identity, not the row number, and should select a sensible adjacent row if the selected session disappears.

## Primary sources

- Ratatui official tutorial, [Hello Ratatui](https://ratatui.rs/tutorials/hello-ratatui/) — `DefaultTerminal`, `run`, and documented `init`/`restore` lifecycle.
- Ratatui API documentation, [`TableState`](https://docs.rs/ratatui/latest/ratatui/widgets/struct.TableState.html) — selection and navigation methods.
- Crossterm API documentation, [`crossterm::event`](https://docs.rs/crossterm/latest/crossterm/event/index.html) — `poll`/`read` contract and threading restriction.
- Arboard API documentation, [`Clipboard`](https://docs.rs/arboard/latest/arboard/struct.Clipboard.html) — OS-independent clipboard, `new`, `set_text`, and platform-specific behavior.
- Local source, `crates/cli/Cargo.toml` and `crates/cli/src/main.rs` — current dependency boundary, command dispatch, validation, discovery, and store integration.
