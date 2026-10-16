//! Pure state and transitions for `aikeeper interactive`.
//!
//! Terminal input, discovery, persistence, and the clipboard are deliberately
//! represented as effects/events so this module can be exercised without them.

use aikeeper_app::{parse_session_title, parse_tag};
use aikeeper_domain::{AgentRuntime, AgentSession};

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
