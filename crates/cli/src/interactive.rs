//! Pure state and transitions for `aikeeper interactive`.
//!
//! Terminal input, discovery, persistence, and the clipboard are deliberately
//! represented as effects/events so this module can be exercised without them.

use std::{
    io::{self, IsTerminal},
    time::Duration,
};

use aikeeper_app::{ListOptions, list_sessions, parse_session_title, parse_tag};
use aikeeper_domain::{AgentRuntime, AgentSession};
use aikeeper_store::{MetadataStore, discover_default};
use arboard::Clipboard;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{
        Block, Borders, Cell, Clear, List, ListItem, Paragraph, Row, Table, TableState, Wrap,
    },
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionKey {
    pub runtime: AgentRuntime,
    pub id: String,
}

impl From<&AgentSession> for SessionKey {
    fn from(session: &AgentSession) -> Self {
        Self {
            runtime: session.runtime,
            id: session.id.clone(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Key {
    Up,
    Down,
    Home,
    End,
    Enter,
    Escape,
    Backspace,
    Character(char),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    EditTitle,
    ClearTitle,
    AddTag,
    RemoveTag,
    CopyId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Modal {
    Help,
    Actions { selected: usize },
    EditTitle { value: String },
    AddTag { value: String },
    RemoveTag { selected: usize },
    ManualCopy { id: String },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Status {
    Info(String),
    Error(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct State {
    pub sessions: Vec<AgentSession>,
    pub selected: Option<usize>,
    pub modal: Option<Modal>,
    pub status: Option<Status>,
}

impl State {
    pub fn new(sessions: Vec<AgentSession>) -> Self {
        Self {
            selected: (!sessions.is_empty()).then_some(0),
            sessions,
            modal: None,
            status: None,
        }
    }

    pub fn selected_session(&self) -> Option<&AgentSession> {
        self.selected.and_then(|index| self.sessions.get(index))
    }

    pub fn selected_key(&self) -> Option<SessionKey> {
        self.selected_session().map(SessionKey::from)
    }

    /// Replaces the filtered discovery result, retaining the selected runtime
    /// UUID when possible and otherwise selecting the nearest remaining row.
    pub fn replace_sessions(&mut self, sessions: Vec<AgentSession>) {
        let selected = self.selected_key();
        let previous_index = self.selected.unwrap_or(0);
        self.selected = selected
            .and_then(|key| {
                sessions
                    .iter()
                    .position(|session| session.runtime == key.runtime && session.id == key.id)
            })
            .or_else(|| (!sessions.is_empty()).then_some(previous_index.min(sessions.len() - 1)));
        self.sessions = sessions;
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Intent {
    MovePrevious,
    MoveNext,
    MoveFirst,
    MoveLast,
    OpenActions,
    CloseModal,
    ShowHelp,
    Reload,
    Quit,
    SelectAction,
    MoveModalPrevious,
    MoveModalNext,
    SubmitText,
    DeleteCharacter,
    Type(char),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Effect {
    Quit,
    Reload,
    SetTitle { session: SessionKey, title: String },
    ClearTitle { session: SessionKey },
    AddTag { session: SessionKey, tag: String },
    RemoveTag { session: SessionKey, tag: String },
    CopyId { id: String },
}

/// Converts a key to an intent based only on the visible modal state.
pub fn intent_for_key(state: &State, key: Key) -> Option<Intent> {
    match state.modal {
        Some(Modal::EditTitle { .. }) | Some(Modal::AddTag { .. }) => match key {
            Key::Escape => Some(Intent::CloseModal),
            Key::Enter => Some(Intent::SubmitText),
            Key::Backspace => Some(Intent::DeleteCharacter),
            Key::Character(character) => Some(Intent::Type(character)),
            _ => None,
        },
        Some(Modal::Actions { .. }) | Some(Modal::RemoveTag { .. }) => match key {
            Key::Escape => Some(Intent::CloseModal),
            Key::Enter => Some(Intent::SelectAction),
            Key::Up | Key::Character('k') => Some(Intent::MoveModalPrevious),
            Key::Down | Key::Character('j') => Some(Intent::MoveModalNext),
            _ => None,
        },
        Some(Modal::Help) | Some(Modal::ManualCopy { .. }) => match key {
            Key::Escape | Key::Enter | Key::Character('?') => Some(Intent::CloseModal),
            _ => None,
        },
        None => match key {
            Key::Up | Key::Character('k') => Some(Intent::MovePrevious),
            Key::Down | Key::Character('j') => Some(Intent::MoveNext),
            Key::Home => Some(Intent::MoveFirst),
            Key::End => Some(Intent::MoveLast),
            Key::Enter => Some(Intent::OpenActions),
            Key::Character('?') => Some(Intent::ShowHelp),
            Key::Character('r') => Some(Intent::Reload),
            Key::Character('q') | Key::Escape => Some(Intent::Quit),
            _ => None,
        },
    }
}

/// Applies an intent and returns an effect for the impure boundary, if any.
pub fn apply(state: &mut State, intent: Intent) -> Option<Effect> {
    match intent {
        Intent::MovePrevious => move_selection(state, false),
        Intent::MoveNext => move_selection(state, true),
        Intent::MoveFirst => state.selected = (!state.sessions.is_empty()).then_some(0),
        Intent::MoveLast => state.selected = state.sessions.len().checked_sub(1),
        Intent::OpenActions => {
            if state.selected.is_some() {
                state.modal = Some(Modal::Actions { selected: 0 });
            }
        }
        Intent::CloseModal => state.modal = None,
        Intent::ShowHelp => state.modal = Some(Modal::Help),
        Intent::Reload => return Some(Effect::Reload),
        Intent::Quit => return Some(Effect::Quit),
        Intent::MoveModalPrevious => move_modal_selection(state, false),
        Intent::MoveModalNext => move_modal_selection(state, true),
        Intent::SelectAction => return select_modal_item(state),
        Intent::DeleteCharacter => edit_text(state, None),
        Intent::Type(character) => edit_text(state, Some(character)),
        Intent::SubmitText => return submit_text(state),
    }
    None
}

/// Reports the result of an external effect. Failed mutations keep their input
/// modal open so the user can correct or retry it.
pub fn complete_effect(state: &mut State, result: Result<(), String>) {
    match result {
        Ok(()) => {
            state.modal = None;
            state.status = Some(Status::Info("Saved session metadata.".to_owned()));
        }
        Err(error) => state.status = Some(Status::Error(error)),
    }
}

/// A clipboard failure is non-fatal: give the UUID to the user to copy.
pub fn complete_copy(state: &mut State, result: Result<(), String>) {
    match result {
        Ok(()) => {
            state.modal = None;
            state.status = Some(Status::Info("Runtime UUID copied to clipboard.".to_owned()));
        }
        Err(error) => {
            let id = state.selected_session().map(|session| session.id.clone());
            state.modal = id.map(|id| Modal::ManualCopy { id });
            state.status = Some(Status::Error(format!("Clipboard unavailable: {error}")));
        }
    }
}

fn move_selection(state: &mut State, forward: bool) {
    let Some(index) = state.selected else { return };
    let last = state.sessions.len().saturating_sub(1);
    state.selected = Some(if forward {
        (index + 1).min(last)
    } else {
        index.saturating_sub(1)
    });
}

pub fn available_actions(state: &State) -> Vec<Action> {
    let mut actions = vec![Action::EditTitle, Action::ClearTitle, Action::AddTag];
    if state
        .selected_session()
        .is_some_and(|session| !session.metadata.tags.is_empty())
    {
        actions.push(Action::RemoveTag);
    }
    actions.push(Action::CopyId);
    actions
}

fn move_modal_selection(state: &mut State, forward: bool) {
    let action_count = available_actions(state).len();
    let tag_count = state
        .selected_session()
        .map_or(0, |session| session.metadata.tags.len());
    let Some(modal) = state.modal.as_mut() else {
        return;
    };
    match modal {
        Modal::Actions { selected } => {
            *selected = if forward {
                (*selected + 1).min(action_count - 1)
            } else {
                selected.saturating_sub(1)
            };
        }
        Modal::RemoveTag { selected } if tag_count > 0 => {
            *selected = if forward {
                (*selected + 1).min(tag_count - 1)
            } else {
                selected.saturating_sub(1)
            };
        }
        _ => {}
    }
}

fn select_modal_item(state: &mut State) -> Option<Effect> {
    match state.modal.clone() {
        Some(Modal::Actions { selected }) => {
            match available_actions(state).get(selected).copied() {
                Some(Action::EditTitle) => {
                    state.modal = Some(Modal::EditTitle {
                        value: state
                            .selected_session()
                            .and_then(|session| session.metadata.title.clone())
                            .unwrap_or_default(),
                    })
                }
                Some(Action::ClearTitle) => {
                    return state
                        .selected_key()
                        .map(|session| Effect::ClearTitle { session });
                }
                Some(Action::AddTag) => {
                    state.modal = Some(Modal::AddTag {
                        value: String::new(),
                    })
                }
                Some(Action::RemoveTag) => state.modal = Some(Modal::RemoveTag { selected: 0 }),
                Some(Action::CopyId) => {
                    return state.selected_session().map(|session| Effect::CopyId {
                        id: session.id.clone(),
                    });
                }
                None => {}
            }
        }
        Some(Modal::RemoveTag { selected }) => {
            let tag = state
                .selected_session()?
                .metadata
                .tags
                .get(selected)?
                .clone();
            return state
                .selected_key()
                .map(|session| Effect::RemoveTag { session, tag });
        }
        _ => {}
    }
    None
}

fn edit_text(state: &mut State, character: Option<char>) {
    let Some(Modal::EditTitle { value } | Modal::AddTag { value }) = state.modal.as_mut() else {
        return;
    };
    match character {
        Some(character) => value.push(character),
        None => {
            value.pop();
        }
    }
}

fn submit_text(state: &mut State) -> Option<Effect> {
    let session = state.selected_key()?;
    let (value, title) = match state.modal.as_ref()? {
        Modal::EditTitle { value } => (value, true),
        Modal::AddTag { value } => (value, false),
        _ => return None,
    };
    let parsed = if title {
        parse_session_title(value)
    } else {
        parse_tag(value)
    };
    match parsed {
        Ok(value) if title => Some(Effect::SetTitle {
            session,
            title: value,
        }),
        Ok(tag) => Some(Effect::AddTag { session, tag }),
        Err(error) => {
            state.status = Some(Status::Error(error));
            None
        }
    }
}

/// Runs the terminal UI after checking that both standard streams are terminals.
pub fn run(options: ListOptions) -> Result<(), String> {
    ensure_terminal(io::stdin().is_terminal(), io::stdout().is_terminal())?;
    let sessions = load_sessions(&options)?;
    with_terminal(|terminal| event_loop(terminal, options, sessions))
}

fn ensure_terminal(stdin_is_tty: bool, stdout_is_tty: bool) -> Result<(), String> {
    if !stdin_is_tty || !stdout_is_tty {
        return Err("interactive requires both stdin and stdout to be terminals".to_owned());
    }
    Ok(())
}

fn load_sessions(options: &ListOptions) -> Result<Vec<AgentSession>, String> {
    let mut sessions = discover_default();
    MetadataStore::open_default()?.hydrate_sessions(&mut sessions)?;
    Ok(list_sessions(sessions, options.clone()))
}

fn with_terminal<T>(
    work: impl FnOnce(&mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<T, String>,
) -> Result<T, String> {
    enable_raw_mode().map_err(|error| format!("enable raw terminal mode: {error}"))?;
    let mut stdout = io::stdout();
    if let Err(error) = execute!(stdout, EnterAlternateScreen) {
        let _ = disable_raw_mode();
        return Err(format!("enter alternate screen: {error}"));
    }
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = match Terminal::new(backend) {
        Ok(terminal) => terminal,
        Err(error) => {
            let mut stdout = io::stdout();
            let _ = execute!(stdout, LeaveAlternateScreen);
            let _ = disable_raw_mode();
            return Err(format!("create terminal: {error}"));
        }
    };
    let result = work(&mut terminal);
    let restore = restore_terminal(&mut terminal);
    match (result, restore) {
        (Err(error), _) => Err(error),
        (Ok(_), Err(error)) => Err(error),
        (Ok(value), Ok(())) => Ok(value),
    }
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<(), String> {
    let cursor = terminal
        .show_cursor()
        .map_err(|error| format!("restore cursor: {error}"));
    let screen = execute!(terminal.backend_mut(), LeaveAlternateScreen)
        .map_err(|error| format!("leave alternate screen: {error}"));
    let raw = disable_raw_mode().map_err(|error| format!("disable raw terminal mode: {error}"));
    cursor.and(screen).and(raw)
}

fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    options: ListOptions,
    sessions: Vec<AgentSession>,
) -> Result<(), String> {
    let mut state = State::new(sessions);
    loop {
        terminal
            .draw(|frame| render(frame, &state))
            .map_err(|error| format!("draw interactive UI: {error}"))?;
        if !event::poll(Duration::from_millis(250))
            .map_err(|error| format!("poll terminal event: {error}"))?
        {
            continue;
        }
        let Event::Key(key) =
            event::read().map_err(|error| format!("read terminal event: {error}"))?
        else {
            continue;
        };
        if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
            continue;
        }
        let Some(key) = map_key(key.code) else {
            continue;
        };
        let Some(intent) = intent_for_key(&state, key) else {
            continue;
        };
        let Some(effect) = apply(&mut state, intent) else {
            continue;
        };
        if matches!(effect, Effect::Quit) {
            return Ok(());
        }
        perform_effect(&mut state, effect, &options);
    }
}

fn map_key(key: KeyCode) -> Option<Key> {
    match key {
        KeyCode::Up => Some(Key::Up),
        KeyCode::Down => Some(Key::Down),
        KeyCode::Home => Some(Key::Home),
        KeyCode::End => Some(Key::End),
        KeyCode::Enter => Some(Key::Enter),
        KeyCode::Esc => Some(Key::Escape),
        KeyCode::Backspace => Some(Key::Backspace),
        KeyCode::Char(character) => Some(Key::Character(character)),
        _ => None,
    }
}

fn perform_effect(state: &mut State, effect: Effect, options: &ListOptions) {
    match effect {
        Effect::Quit => {}
        Effect::Reload => match load_sessions(options) {
            Ok(sessions) => {
                state.replace_sessions(sessions);
                state.status = Some(Status::Info("Reloaded sessions.".to_owned()));
            }
            Err(error) => state.status = Some(Status::Error(error)),
        },
        Effect::CopyId { id } => complete_copy(state, copy_id(&id)),
        effect => {
            let result = mutate_effect(effect);
            let succeeded = result.is_ok();
            complete_effect(state, result);
            if succeeded {
                match load_sessions(options) {
                    Ok(sessions) => state.replace_sessions(sessions),
                    Err(error) => {
                        state.status = Some(Status::Error(format!(
                            "Saved metadata, but reload failed: {error}"
                        )))
                    }
                }
            }
        }
    }
}

fn mutate_effect(effect: Effect) -> Result<(), String> {
    let session = match &effect {
        Effect::SetTitle { session, .. }
        | Effect::ClearTitle { session }
        | Effect::AddTag { session, .. }
        | Effect::RemoveTag { session, .. } => session,
        Effect::Quit | Effect::Reload | Effect::CopyId { .. } => return Ok(()),
    };
    if !discover_default()
        .iter()
        .any(|candidate| candidate.runtime == session.runtime && candidate.id == session.id)
    {
        return Err(format!(
            "session {}:{} is not currently discoverable",
            session.runtime, session.id
        ));
    }
    let store = MetadataStore::open_default()?;
    match effect {
        Effect::SetTitle { session, title } => {
            store.set_title(session.runtime, &session.id, &title)
        }
        Effect::ClearTitle { session } => store.clear_title(session.runtime, &session.id),
        Effect::AddTag { session, tag } => store.add_tag(session.runtime, &session.id, &tag),
        Effect::RemoveTag { session, tag } => store.remove_tag(session.runtime, &session.id, &tag),
        Effect::Quit | Effect::Reload | Effect::CopyId { .. } => Ok(()),
    }
}

fn copy_id(id: &str) -> Result<(), String> {
    let mut clipboard = Clipboard::new().map_err(|error| error.to_string())?;
    clipboard.set_text(id).map_err(|error| error.to_string())
}

fn render(frame: &mut ratatui::Frame, state: &State) {
    let area = frame.area();
    if area.width < 80 || area.height < 12 {
        frame.render_widget(
            Paragraph::new("Terminal too small. Resize to at least 80×12.")
                .alignment(Alignment::Center),
            area,
        );
        return;
    }
    let areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(4),
            Constraint::Length(2),
            Constraint::Length(1),
        ])
        .split(area);
    frame.render_widget(
        Paragraph::new("AIKeeper interactive sessions")
            .style(Style::default().add_modifier(Modifier::BOLD)),
        areas[0],
    );
    let header = Row::new([
        "Runtime",
        "Working Directory",
        "Session Title",
        "Tags",
        "Runtime UUID",
    ])
    .style(
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    );
    let rows = state.sessions.iter().map(|session| {
        Row::new(vec![
            Cell::from(session.runtime.to_string()),
            Cell::from(
                session
                    .cwd
                    .as_deref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| "-".to_owned()),
            ),
            Cell::from(
                session
                    .metadata
                    .title
                    .clone()
                    .unwrap_or_else(|| "-".to_owned()),
            ),
            Cell::from(if session.metadata.tags.is_empty() {
                "-".to_owned()
            } else {
                session.metadata.tags.join(",")
            }),
            Cell::from(session.id.clone()),
        ])
    });
    let table = Table::new(
        rows,
        [
            Constraint::Length(12),
            Constraint::Percentage(30),
            Constraint::Percentage(20),
            Constraint::Percentage(15),
            Constraint::Percentage(20),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title("Filtered sessions"),
    )
    .row_highlight_style(Style::default().bg(Color::Blue));
    let mut table_state = TableState::default();
    table_state.select(state.selected);
    frame.render_stateful_widget(table, areas[1], &mut table_state);
    let status = match &state.status {
        Some(Status::Info(message)) => {
            Paragraph::new(message.clone()).style(Style::default().fg(Color::Green))
        }
        Some(Status::Error(message)) => {
            Paragraph::new(message.clone()).style(Style::default().fg(Color::Red))
        }
        None => Paragraph::new(""),
    };
    frame.render_widget(status.wrap(Wrap { trim: true }), areas[2]);
    frame.render_widget(
        Paragraph::new("↑/↓ or j/k move  Home/End  Enter actions  r reload  ? help  q/Esc quit"),
        areas[3],
    );
    if let Some(modal) = &state.modal {
        render_modal(frame, area, modal, state);
    }
}

fn render_modal(frame: &mut ratatui::Frame, area: Rect, modal: &Modal, state: &State) {
    let popup = centered_rect(70, 60, area);
    frame.render_widget(Clear, popup);
    let (title, lines) = match modal {
        Modal::Help => (
            "Help",
            vec![
                "Enter opens actions; choose title, tags, or copy UUID.".to_owned(),
                "Esc closes this dialog. Reload retains the selected session when possible."
                    .to_owned(),
            ],
        ),
        Modal::Actions { selected } => (
            "Actions",
            available_actions(state)
                .iter()
                .enumerate()
                .map(|(index, action)| {
                    format!(
                        "{} {}",
                        if index == *selected { ">" } else { " " },
                        action_label(*action)
                    )
                })
                .collect(),
        ),
        Modal::EditTitle { value } => (
            "Edit title",
            vec![format!("{value}_"), "Enter saves; Esc cancels.".to_owned()],
        ),
        Modal::AddTag { value } => (
            "Add tag",
            vec![format!("{value}_"), "Enter saves; Esc cancels.".to_owned()],
        ),
        Modal::RemoveTag { selected } => (
            "Remove tag",
            state
                .selected_session()
                .map(|session| {
                    session
                        .metadata
                        .tags
                        .iter()
                        .enumerate()
                        .map(|(index, tag)| {
                            format!("{} {tag}", if index == *selected { ">" } else { " " })
                        })
                        .collect()
                })
                .unwrap_or_default(),
        ),
        Modal::ManualCopy { id } => (
            "Copy runtime UUID",
            vec![
                id.clone(),
                "Clipboard unavailable; select this UUID and copy it manually. Esc closes."
                    .to_owned(),
            ],
        ),
    };
    let items = lines.into_iter().map(ListItem::new);
    frame.render_widget(
        List::new(items).block(Block::default().borders(Borders::ALL).title(title)),
        popup,
    );
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}

fn action_label(action: Action) -> &'static str {
    match action {
        Action::EditTitle => "Edit title",
        Action::ClearTitle => "Clear title",
        Action::AddTag => "Add tag",
        Action::RemoveTag => "Remove tag",
        Action::CopyId => "Copy runtime UUID",
    }
}

#[cfg(test)]
mod tests {
    use aikeeper_domain::{AgentRuntime, AgentSession, SessionMetadata};

    use super::*;

    fn session(id: &str, tags: &[&str]) -> AgentSession {
        AgentSession {
            runtime: AgentRuntime::Pi,
            id: id.to_owned(),
            cwd: None,
            last_activity: None,
            metadata: SessionMetadata {
                title: None,
                tags: tags.iter().map(ToString::to_string).collect(),
            },
            discovery_order: 0,
        }
    }

    #[test]
    fn rejects_non_terminal_streams_before_setup() {
        assert!(ensure_terminal(false, true).is_err());
        assert!(ensure_terminal(true, false).is_err());
        assert!(ensure_terminal(true, true).is_ok());
    }

    #[test]
    fn navigates_and_opens_help_without_terminal() {
        let mut state = State::new(vec![session("one", &[]), session("two", &[])]);
        let intent = intent_for_key(&state, Key::Down).unwrap();
        assert_eq!(apply(&mut state, intent), None);
        assert_eq!(state.selected, Some(1));
        apply(&mut state, Intent::MoveFirst);
        assert_eq!(state.selected, Some(0));
        let intent = intent_for_key(&state, Key::Character('?')).unwrap();
        apply(&mut state, intent);
        assert_eq!(state.modal, Some(Modal::Help));
    }

    #[test]
    fn title_validation_keeps_editor_open_and_valid_title_emits_effect() {
        let mut state = State::new(vec![session("one", &[])]);
        state.modal = Some(Modal::EditTitle {
            value: " \t ".to_owned(),
        });
        assert_eq!(apply(&mut state, Intent::SubmitText), None);
        assert!(matches!(state.status, Some(Status::Error(_))));
        assert!(matches!(state.modal, Some(Modal::EditTitle { .. })));

        state.modal = Some(Modal::EditTitle {
            value: "  A title  ".to_owned(),
        });
        assert_eq!(
            apply(&mut state, Intent::SubmitText),
            Some(Effect::SetTitle {
                session: SessionKey {
                    runtime: AgentRuntime::Pi,
                    id: "one".to_owned()
                },
                title: "A title".to_owned()
            })
        );
    }

    #[test]
    fn actions_distinguish_title_clear_tag_removal_and_copy_fallback() {
        let mut state = State::new(vec![session("one", &["alpha", "beta"])]);
        state.modal = Some(Modal::Actions { selected: 1 });
        assert!(matches!(
            apply(&mut state, Intent::SelectAction),
            Some(Effect::ClearTitle { .. })
        ));
        state.modal = Some(Modal::RemoveTag { selected: 1 });
        assert!(
            matches!(apply(&mut state, Intent::SelectAction), Some(Effect::RemoveTag { tag, .. }) if tag == "beta")
        );
        complete_copy(&mut state, Err("no display".to_owned()));
        assert_eq!(
            state.modal,
            Some(Modal::ManualCopy {
                id: "one".to_owned()
            })
        );
    }

    #[test]
    fn reload_reselects_identity_then_nearest_remaining_row() {
        let mut state = State::new(vec![
            session("one", &[]),
            session("two", &[]),
            session("three", &[]),
        ]);
        state.selected = Some(1);
        state.replace_sessions(vec![session("three", &[]), session("two", &[])]);
        assert_eq!(state.selected, Some(1));
        state.replace_sessions(vec![session("three", &[])]);
        assert_eq!(state.selected, Some(0));
    }
}
