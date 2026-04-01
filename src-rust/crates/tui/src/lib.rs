// cc-tui: Terminal UI using ratatui + crossterm for the Claude Code Rust port.
//
// This crate provides the interactive terminal interface including:
// - Message display with syntax highlighting
// - Input prompt with history
// - Streaming response rendering
// - Tool execution progress display
// - Permission dialogs
// - Cost/token tracking display
// - Notification banners
// - Help, history-search, message-selector, and rewind overlays
// - Bridge connection status badge
// - Plugin hint banners

use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io::{self, Stdout};

// ---------------------------------------------------------------------------
// Sub-modules
// ---------------------------------------------------------------------------

/// Application state and main event loop.
pub mod app;
/// Input helpers: slash command parsing.
pub mod input;
/// All ratatui rendering logic.
pub mod render;
/// Permission dialogs and confirmation dialogs.
pub mod dialogs;
/// Notification / banner system.
pub mod notifications;
/// Help overlay, history search, message selector, rewind flow.
pub mod overlays;
/// Bridge connection state and status badge.
pub mod bridge_state;
/// Plugin hint/recommendation UI.
pub mod plugin_views;
/// Full-screen tabbed settings interface.
pub mod settings_screen;
/// Theme picker overlay.
pub mod theme_screen;
/// Privacy settings dialog.
pub mod privacy_screen;

// ---------------------------------------------------------------------------
// Public re-exports
// ---------------------------------------------------------------------------

pub use app::App;
pub use input::{is_slash_command, parse_slash_command};

// ---------------------------------------------------------------------------
// Terminal initialization / teardown helpers (public API)
// ---------------------------------------------------------------------------

/// Set up the terminal for TUI mode (raw mode + alternate screen).
pub fn setup_terminal() -> io::Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;
    Ok(terminal)
}

/// Restore the terminal to its original state.
pub fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> io::Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use app::{App, HistorySearch, ToolStatus, ToolUseBlock};
    use dialogs::PermissionRequest;
    use cc_core::config::Config;
    use cc_core::cost::CostTracker;
    use cc_core::types::Role;

    fn make_app() -> App {
        App::new(Config::default(), CostTracker::new())
    }

    // ---- input helpers ---------------------------------------------------

    #[test]
    fn test_is_slash_command() {
        assert!(input::is_slash_command("/help"));
        assert!(input::is_slash_command("/compact args"));
        assert!(!input::is_slash_command("//comment"));
        assert!(!input::is_slash_command("hello"));
        assert!(!input::is_slash_command(""));
    }

    #[test]
    fn test_parse_slash_command_no_args() {
        let (cmd, args) = input::parse_slash_command("/help");
        assert_eq!(cmd, "help");
        assert_eq!(args, "");
    }

    #[test]
    fn test_parse_slash_command_with_args() {
        let (cmd, args) = input::parse_slash_command("/compact  --force ");
        assert_eq!(cmd, "compact");
        assert_eq!(args, "--force");
    }

    #[test]
    fn test_parse_slash_command_non_slash() {
        let (cmd, args) = input::parse_slash_command("hello world");
        assert_eq!(cmd, "");
        assert_eq!(args, "");
    }

    // ---- App::take_input ------------------------------------------------

    #[test]
    fn test_take_input_pushes_history() {
        let mut app = make_app();
        app.input = "hello".to_string();
        let result = app.take_input();
        assert_eq!(result, "hello");
        assert_eq!(app.input, "");
        assert_eq!(app.input_history, vec!["hello"]);
        assert_eq!(app.cursor_pos, 0);
    }

    #[test]
    fn test_take_input_empty_does_not_push_history() {
        let mut app = make_app();
        let result = app.take_input();
        assert_eq!(result, "");
        assert!(app.input_history.is_empty());
    }

    // ---- add_message / set_model ----------------------------------------

    #[test]
    fn test_add_message() {
        let mut app = make_app();
        app.add_message(Role::User, "hi".to_string());
        assert_eq!(app.messages.len(), 1);
        assert_eq!(app.messages[0].role, Role::User);
    }

    #[test]
    fn test_set_model() {
        let mut app = make_app();
        app.set_model("claude-opus-4-5".to_string());
        assert_eq!(app.model_name, "claude-opus-4-5");
    }

    // ---- key handling ----------------------------------------------------

    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    fn ctrl(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    #[test]
    fn test_ctrl_c_quits_when_idle() {
        let mut app = make_app();
        app.handle_key_event(ctrl(KeyCode::Char('c')));
        assert!(app.should_quit);
    }

    #[test]
    fn test_ctrl_c_cancels_streaming() {
        let mut app = make_app();
        app.is_streaming = true;
        app.streaming_text = "partial".to_string();
        app.handle_key_event(ctrl(KeyCode::Char('c')));
        assert!(!app.is_streaming);
        assert!(!app.should_quit);
        assert!(app.streaming_text.is_empty());
    }

    #[test]
    fn test_ctrl_d_quits_on_empty_input() {
        let mut app = make_app();
        app.handle_key_event(ctrl(KeyCode::Char('d')));
        assert!(app.should_quit);
    }

    #[test]
    fn test_ctrl_d_does_not_quit_with_input() {
        let mut app = make_app();
        app.input = "abc".to_string();
        app.handle_key_event(ctrl(KeyCode::Char('d')));
        assert!(!app.should_quit);
    }

    #[test]
    fn test_enter_returns_true() {
        let mut app = make_app();
        let submit = app.handle_key_event(key(KeyCode::Enter));
        assert!(submit);
    }

    #[test]
    fn test_enter_blocked_while_streaming() {
        let mut app = make_app();
        app.is_streaming = true;
        let submit = app.handle_key_event(key(KeyCode::Enter));
        assert!(!submit);
    }

    #[test]
    fn test_char_input_appends() {
        let mut app = make_app();
        app.handle_key_event(key(KeyCode::Char('h')));
        app.handle_key_event(key(KeyCode::Char('i')));
        assert_eq!(app.input, "hi");
    }

    #[test]
    fn test_backspace_removes_char() {
        let mut app = make_app();
        app.input = "hello".to_string();
        app.cursor_pos = 5;
        app.handle_key_event(key(KeyCode::Backspace));
        assert_eq!(app.input, "hell");
    }

    #[test]
    fn test_history_navigation() {
        let mut app = make_app();
        app.input_history = vec!["first".to_string(), "second".to_string()];
        app.handle_key_event(key(KeyCode::Up));
        assert_eq!(app.input, "second");
        app.handle_key_event(key(KeyCode::Up));
        assert_eq!(app.input, "first");
        app.handle_key_event(key(KeyCode::Down));
        assert_eq!(app.input, "second");
        app.handle_key_event(key(KeyCode::Down));
        assert_eq!(app.input, "");
        assert!(app.history_index.is_none());
    }

    #[test]
    fn test_page_scroll() {
        let mut app = make_app();
        app.handle_key_event(key(KeyCode::PageUp));
        assert_eq!(app.scroll_offset, 10);
        app.handle_key_event(key(KeyCode::PageDown));
        assert_eq!(app.scroll_offset, 0);
    }

    #[test]
    fn test_f1_toggles_help() {
        let mut app = make_app();
        assert!(!app.show_help);
        app.handle_key_event(key(KeyCode::F(1)));
        assert!(app.show_help);
        app.handle_key_event(key(KeyCode::F(1)));
        assert!(!app.show_help);
    }

    // ---- QueryEvent handling --------------------------------------------

    #[test]
    fn test_handle_status_event() {
        let mut app = make_app();
        app.handle_query_event(cc_query::QueryEvent::Status("working".to_string()));
        assert_eq!(app.status_message.as_deref(), Some("working"));
    }

    #[test]
    fn test_handle_error_event() {
        let mut app = make_app();
        app.is_streaming = true;
        app.handle_query_event(cc_query::QueryEvent::Error("oops".to_string()));
        assert!(!app.is_streaming);
        assert_eq!(app.messages.len(), 1);
        assert!(app.messages[0].get_all_text().contains("oops"));
    }

    #[test]
    fn test_handle_tool_start_and_end() {
        let mut app = make_app();
        app.handle_query_event(cc_query::QueryEvent::ToolStart {
            tool_name: "Bash".to_string(),
            tool_id: "t1".to_string(),
        });
        assert_eq!(app.tool_use_blocks.len(), 1);
        assert_eq!(app.tool_use_blocks[0].status, ToolStatus::Running);

        app.handle_query_event(cc_query::QueryEvent::ToolEnd {
            tool_name: "Bash".to_string(),
            tool_id: "t1".to_string(),
            result: "output".to_string(),
            is_error: false,
        });
        assert_eq!(app.tool_use_blocks[0].status, ToolStatus::Done);
    }

    #[test]
    fn test_handle_tool_end_error() {
        let mut app = make_app();
        app.tool_use_blocks.push(ToolUseBlock {
            id: "t2".to_string(),
            name: "Read".to_string(),
            status: ToolStatus::Running,
            output_preview: None,
        });
        app.handle_query_event(cc_query::QueryEvent::ToolEnd {
            tool_name: "Read".to_string(),
            tool_id: "t2".to_string(),
            result: "file not found".to_string(),
            is_error: true,
        });
        assert_eq!(app.tool_use_blocks[0].status, ToolStatus::Error);
        assert!(app.status_message.is_some());
    }

    #[test]
    fn test_turn_complete_flushes_streaming_text() {
        let mut app = make_app();
        app.is_streaming = true;
        app.streaming_text = "partial response".to_string();
        app.handle_query_event(cc_query::QueryEvent::TurnComplete {
            turn: 1,
            stop_reason: "end_turn".to_string(),
        });
        assert!(!app.is_streaming);
        assert!(app.streaming_text.is_empty());
        assert_eq!(app.messages.len(), 1);
        assert_eq!(app.messages[0].get_all_text(), "partial response");
    }

    // ---- HistorySearch --------------------------------------------------

    #[test]
    fn test_history_search_matches() {
        let history = vec![
            "git commit".to_string(),
            "git push".to_string(),
            "cargo build".to_string(),
        ];
        let mut hs = HistorySearch::new();
        hs.query = "git".to_string();
        hs.update_matches(&history);
        assert_eq!(hs.matches.len(), 2);
        assert_eq!(hs.matches[0], 0);
        assert_eq!(hs.matches[1], 1);
    }

    #[test]
    fn test_history_search_no_matches() {
        let history = vec!["hello".to_string()];
        let mut hs = HistorySearch::new();
        hs.query = "xyz".to_string();
        hs.update_matches(&history);
        assert!(hs.matches.is_empty());
    }

    // ---- PermissionRequest --------------------------------------------

    #[test]
    fn test_permission_request_standard() {
        let pr = PermissionRequest::standard(
            "tu1".to_string(),
            "Bash".to_string(),
            "Run a shell command".to_string(),
        );
        assert_eq!(pr.options.len(), 4);
        assert_eq!(pr.options[0].key, 'y');
        assert_eq!(pr.options[1].key, 'Y');
        assert_eq!(pr.options[2].key, 'p');
        assert_eq!(pr.options[3].key, 'n');
    }
}
