// render.rs — All ratatui rendering logic.

use crate::app::{App, ToolStatus};
use crate::dialogs::render_permission_dialog;
use crate::notifications::render_notification_banner;
use crate::overlays::{
    render_help_overlay, render_history_search_overlay, render_rewind_flow,
};
use crate::plugin_views::render_plugin_hints;
use crate::privacy_screen::render_privacy_screen;
use crate::settings_screen::render_settings_screen;
use crate::theme_screen::render_theme_screen;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;
use unicode_width::UnicodeWidthStr;

// Braille spinner sequence
const SPINNER: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

fn spinner_char(frame_count: u64) -> char {
    SPINNER[(frame_count as usize) % SPINNER.len()]
}

// -----------------------------------------------------------------------
// Top-level layout
// -----------------------------------------------------------------------

/// Render the entire application into the current frame.
pub fn render_app(frame: &mut Frame, app: &App) {
    let size = frame.area();

    // Four-row vertical layout: messages | input | keybinding hints | status bar
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),
            Constraint::Length(5),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(size);

    render_messages(frame, app, chunks[0]);
    render_input(frame, app, chunks[1]);
    render_keybinding_hints(frame, app, chunks[2]);
    render_status_bar(frame, app, chunks[3]);

    // Overlays (rendered on top in Z-order)

    // Permission dialog (highest priority)
    if let Some(ref pr) = app.permission_request {
        render_permission_dialog(frame, pr, size);
    }

    // Rewind flow (takes over screen)
    if app.rewind_flow.visible {
        render_rewind_flow(frame, &app.rewind_flow, size);
    }

    // New help overlay
    if app.help_overlay.visible {
        render_help_overlay(frame, &app.help_overlay, size);
    } else if app.show_help {
        // Legacy fallback — render the simple help overlay
        render_simple_help_overlay(frame, size);
    }

    // History search overlay
    if app.history_search_overlay.visible {
        render_history_search_overlay(
            frame,
            &app.history_search_overlay,
            &app.input_history,
            size,
        );
    } else if let Some(ref hs) = app.history_search {
        // Legacy history search rendering
        render_legacy_history_search(frame, hs, app, size);
    }

    // Settings screen (highest-priority full-screen overlay)
    if app.settings_screen.visible {
        render_settings_screen(frame, &app.settings_screen, size);
    }

    // Theme picker overlay
    if app.theme_screen.visible {
        render_theme_screen(frame, &app.theme_screen, size);
    }

    // Privacy settings dialog
    if app.privacy_screen.visible {
        render_privacy_screen(frame, &app.privacy_screen, size);
    }

    // Notification banner (bottom of overlays stack so it's always visible)
    if !app.notifications.is_empty() {
        render_notification_banner(frame, &app.notifications, size);
    }
}

// -----------------------------------------------------------------------
// Messages pane
// -----------------------------------------------------------------------

fn render_messages(frame: &mut Frame, app: &App, area: Rect) {
    // Reserve space at the top for plugin hint banners
    let hint_height = if app.plugin_hints.iter().any(|h| h.is_visible()) {
        3u16
    } else {
        0
    };

    let (hint_area, msg_area) = if hint_height > 0 && area.height > hint_height + 2 {
        let splits = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(hint_height), Constraint::Min(1)])
            .split(area);
        (Some(splits[0]), splits[1])
    } else {
        (None, area)
    };

    // Render plugin hint banner if there is one
    if let Some(ha) = hint_area {
        render_plugin_hints(frame, &app.plugin_hints, ha);
    }

    let mut lines: Vec<Line> = Vec::new();

    for msg in &app.messages {
        let (prefix, style) = match msg.role {
            cc_core::types::Role::User => (
                "You",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            cc_core::types::Role::Assistant => (
                "Claude",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
        };

        // Role header
        lines.push(Line::from(vec![
            Span::styled(format!("  {} ", prefix), style),
            Span::styled(
                "\u{2500}".repeat(
                    msg_area
                        .width
                        .saturating_sub(prefix.len() as u16 + 4) as usize,
                ),
                Style::default().fg(Color::DarkGray),
            ),
        ]));

        let text = msg.get_all_text();
        let rendered = render_markdown_lines(&text, msg_area.width as usize);

        // Truncate very long outputs with a "... N more lines" notice
        const MAX_LINES_PER_MSG: usize = 200;
        if rendered.len() > MAX_LINES_PER_MSG {
            lines.extend(rendered[..MAX_LINES_PER_MSG].iter().cloned());
            lines.push(Line::from(vec![Span::styled(
                format!(
                    "  … {} more lines (scroll up to see all)",
                    rendered.len() - MAX_LINES_PER_MSG
                ),
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::ITALIC),
            )]));
        } else {
            lines.extend(rendered);
        }
        lines.push(Line::from(""));
    }

    // Active tool-use blocks
    for block in &app.tool_use_blocks {
        let (icon, icon_style) = match block.status {
            ToolStatus::Running => (
                format!("{}", spinner_char(app.frame_count)),
                Style::default().fg(Color::Yellow),
            ),
            ToolStatus::Done => ("\u{2713}".to_string(), Style::default().fg(Color::Green)),
            ToolStatus::Error => ("\u{2717}".to_string(), Style::default().fg(Color::Red)),
        };

        let suffix = match block.status {
            ToolStatus::Running => " running…".to_string(),
            ToolStatus::Done => " done".to_string(),
            ToolStatus::Error => " error".to_string(),
        };

        let spans = vec![
            Span::styled(format!("  {} ", icon), icon_style),
            Span::styled(
                format!("[{}]", block.name),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(suffix, Style::default().fg(Color::DarkGray)),
        ];
        lines.push(Line::from(spans));

        if let Some(ref preview) = block.output_preview {
            let trimmed = preview.lines().next().unwrap_or("").to_string();
            if !trimmed.is_empty() {
                lines.push(Line::from(vec![
                    Span::raw("    "),
                    Span::styled(trimmed, Style::default().fg(Color::DarkGray)),
                ]));
            }
        }
    }

    // In-flight streaming text
    if !app.streaming_text.is_empty() {
        lines.push(Line::from(vec![
            Span::styled(
                format!("  {} Claude ", spinner_char(app.frame_count)),
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "\u{2500}".repeat(msg_area.width.saturating_sub(12) as usize),
                Style::default().fg(Color::DarkGray),
            ),
        ]));
        for raw_line in app.streaming_text.lines() {
            lines.push(Line::from(vec![Span::styled(
                format!("  {}", raw_line),
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::ITALIC),
            )]));
        }
    }

    // Compute total virtual height and apply scroll clamping
    let content_height = lines.len() as u16;
    let visible_height = msg_area.height.saturating_sub(2);
    let max_scroll = content_height.saturating_sub(visible_height) as usize;
    let scroll = app.scroll_offset.min(max_scroll);

    // Build title (optionally include session title)
    let title = match &app.session_title {
        Some(t) => format!(" Claude Code — {} ", t),
        None => " Claude Code ".to_string(),
    };

    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .wrap(Wrap { trim: false })
        .scroll((scroll as u16, 0));

    frame.render_widget(paragraph, msg_area);
}

// -----------------------------------------------------------------------
// Markdown-aware line renderer
// -----------------------------------------------------------------------

/// Convert a Markdown-ish string into a list of styled `Line` values.
fn render_markdown_lines(text: &str, width: usize) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut in_code_block = false;
    let mut code_lang = String::new();

    for raw in text.lines() {
        // Fenced code block open/close
        if raw.trim_start().starts_with("```") {
            if in_code_block {
                lines.push(Line::from(vec![Span::styled(
                    "  \u{2514}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}".to_string(),
                    Style::default().fg(Color::Yellow),
                )]));
                in_code_block = false;
                code_lang.clear();
            } else {
                in_code_block = true;
                code_lang = raw.trim_start().trim_start_matches('`').trim().to_string();
                let lang_label = if code_lang.is_empty() {
                    String::new()
                } else {
                    format!(" {} ", code_lang)
                };
                lines.push(Line::from(vec![Span::styled(
                    format!("  \u{250c}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}{}", lang_label),
                    Style::default().fg(Color::Yellow),
                )]));
            }
            continue;
        }

        if in_code_block {
            lines.push(Line::from(vec![
                Span::styled("  \u{2502} ", Style::default().fg(Color::Yellow)),
                Span::styled(raw.to_string(), Style::default().fg(Color::White)),
            ]));
            continue;
        }

        // Heading
        if raw.starts_with("### ") {
            lines.push(Line::from(vec![Span::styled(
                format!("  {}", &raw[4..]),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            )]));
            continue;
        }
        if raw.starts_with("## ") {
            lines.push(Line::from(vec![Span::styled(
                format!("  {}", &raw[3..]),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            )]));
            continue;
        }
        if raw.starts_with("# ") {
            lines.push(Line::from(vec![Span::styled(
                format!("  {}", &raw[2..]),
                Style::default()
                    .fg(Color::Blue)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            )]));
            continue;
        }

        // Plain text — parse inline markup and word-wrap
        let padded = format!("  {}", raw);
        let effective_width = width.saturating_sub(4);
        for wrapped_line in word_wrap(&padded, effective_width) {
            let spans = parse_inline_spans(wrapped_line);
            lines.push(Line::from(spans));
        }
    }

    // Unclosed code block (defensive)
    if in_code_block {
        lines.push(Line::from(vec![Span::styled(
            "  \u{2514}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}".to_string(),
            Style::default().fg(Color::Yellow),
        )]));
    }

    lines
}

/// Parse inline markup (`**bold**`, `` `code` ``) into styled spans.
fn parse_inline_spans(text: String) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut remaining = text.as_str();

    while !remaining.is_empty() {
        let bold_pos = remaining.find("**");
        let code_pos = remaining.find('`');

        match (bold_pos, code_pos) {
            (None, None) => {
                spans.push(Span::raw(remaining.to_string()));
                break;
            }
            (Some(b), Some(c)) if c < b => {
                if c > 0 {
                    spans.push(Span::raw(remaining[..c].to_string()));
                }
                let after_tick = &remaining[c + 1..];
                if let Some(end) = after_tick.find('`') {
                    spans.push(Span::styled(
                        after_tick[..end].to_string(),
                        Style::default().fg(Color::Yellow),
                    ));
                    remaining = &after_tick[end + 1..];
                } else {
                    spans.push(Span::raw(remaining[c..].to_string()));
                    break;
                }
            }
            (Some(b), _) => {
                if b > 0 {
                    spans.push(Span::raw(remaining[..b].to_string()));
                }
                let after_stars = &remaining[b + 2..];
                if let Some(end) = after_stars.find("**") {
                    spans.push(Span::styled(
                        after_stars[..end].to_string(),
                        Style::default().add_modifier(Modifier::BOLD),
                    ));
                    remaining = &after_stars[end + 2..];
                } else {
                    spans.push(Span::raw(remaining[b..].to_string()));
                    break;
                }
            }
            (None, Some(c)) => {
                if c > 0 {
                    spans.push(Span::raw(remaining[..c].to_string()));
                }
                let after_tick = &remaining[c + 1..];
                if let Some(end) = after_tick.find('`') {
                    spans.push(Span::styled(
                        after_tick[..end].to_string(),
                        Style::default().fg(Color::Yellow),
                    ));
                    remaining = &after_tick[end + 1..];
                } else {
                    spans.push(Span::raw(remaining[c..].to_string()));
                    break;
                }
            }
        }
    }

    if spans.is_empty() {
        spans.push(Span::raw(String::new()));
    }
    spans
}

/// Naive word-wrap.
fn word_wrap(text: &str, width: usize) -> Vec<String> {
    if width == 0 || UnicodeWidthStr::width(text) <= width {
        return vec![text.to_string()];
    }

    let mut result = Vec::new();
    let mut current_line = String::new();
    let mut current_width = 0usize;

    for word in text.split_whitespace() {
        let word_w = UnicodeWidthStr::width(word);
        if current_width == 0 {
            current_line.push_str(word);
            current_width = word_w;
        } else if current_width + 1 + word_w <= width {
            current_line.push(' ');
            current_line.push_str(word);
            current_width += 1 + word_w;
        } else {
            result.push(std::mem::take(&mut current_line));
            current_line.push_str(word);
            current_width = word_w;
        }
    }
    if !current_line.is_empty() {
        result.push(current_line);
    }
    if result.is_empty() {
        result.push(text.to_string());
    }
    result
}

// -----------------------------------------------------------------------
// Input pane
// -----------------------------------------------------------------------

fn render_input(frame: &mut Frame, app: &App, area: Rect) {
    let border_style = if app.is_streaming {
        Style::default().fg(Color::DarkGray)
    } else if crate::input::is_slash_command(&app.input) {
        Style::default().fg(Color::Magenta)
    } else {
        Style::default().fg(Color::Cyan)
    };

    let title = if app.is_streaming {
        " Streaming\u{2026} (Ctrl+C to cancel) "
    } else if app.history_index.is_some() {
        " Input (history) "
    } else {
        " Input "
    };

    let prompt_prefix = if app.is_streaming { " \u{2026} " } else { " > " };

    let hint_line = if crate::input::is_slash_command(&app.input) {
        let (cmd, args) = crate::input::parse_slash_command(&app.input);
        format!("  /{} {}", cmd, args)
    } else {
        String::new()
    };

    let mut input_lines: Vec<Line> = Vec::new();
    input_lines.push(Line::from(vec![
        Span::styled(prompt_prefix, Style::default().fg(Color::Cyan)),
        Span::raw(app.input.clone()),
    ]));
    if !hint_line.is_empty() {
        input_lines.push(Line::from(vec![Span::styled(
            hint_line,
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::DIM),
        )]));
    }

    let paragraph = Paragraph::new(input_lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(border_style),
        )
        .wrap(Wrap { trim: false });

    frame.render_widget(paragraph, area);

    // Position the terminal cursor inside the input box
    if !app.is_streaming
        && app.permission_request.is_none()
        && app.history_search.is_none()
        && !app.history_search_overlay.visible
    {
        let cursor_col = " > ".len() + UnicodeWidthStr::width(app.input.as_str());
        let cursor_x = area.x
            + 1
            + cursor_col.min((area.width as usize).saturating_sub(2)) as u16;
        let cursor_y = area.y + 1;
        frame.set_cursor_position((cursor_x, cursor_y));
    }
}

// -----------------------------------------------------------------------
// Keybinding hints footer
// -----------------------------------------------------------------------

fn render_keybinding_hints(frame: &mut Frame, app: &App, area: Rect) {
    let hints: Vec<(&str, &str)> = if app.settings_screen.visible {
        vec![
            ("Tab", "next tab"),
            ("↑↓", "scroll"),
            ("Enter", "edit field"),
            ("Esc", "close"),
        ]
    } else if app.theme_screen.visible {
        vec![
            ("↑↓", "navigate"),
            ("Enter", "select"),
            ("Esc", "cancel"),
        ]
    } else if app.privacy_screen.visible {
        vec![
            ("↑↓", "navigate"),
            ("Space/Enter", "toggle"),
            ("Esc", "close"),
        ]
    } else if app.help_overlay.visible {
        vec![
            ("↑↓", "scroll"),
            ("type", "filter"),
            ("Esc/?", "close"),
        ]
    } else if app.history_search_overlay.visible {
        vec![
            ("↑↓", "navigate"),
            ("Enter", "select"),
            ("Esc", "cancel"),
        ]
    } else if app.is_streaming {
        vec![
            ("Ctrl+C", "cancel"),
        ]
    } else {
        vec![
            ("?", "help"),
            ("Ctrl+R", "history"),
            ("/", "command"),
            ("Enter", "submit"),
            ("PgUp/Dn", "scroll"),
        ]
    };

    let mut spans: Vec<Span> = Vec::new();
    spans.push(Span::raw(" "));
    for (i, (key, desc)) in hints.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(
                "  ",
                Style::default().fg(Color::DarkGray),
            ));
        }
        spans.push(Span::styled(
            format!("[{}]", key),
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            (*desc).to_string(),
            Style::default().fg(Color::DarkGray),
        ));
    }

    let line = Line::from(spans);
    let bar = Paragraph::new(vec![line])
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(bar, area);
}

// -----------------------------------------------------------------------
// Status bar
// -----------------------------------------------------------------------

fn render_status_bar(frame: &mut Frame, app: &App, area: Rect) {
    let spinner = if app.is_streaming {
        format!("{} ", spinner_char(app.frame_count))
    } else {
        String::new()
    };

    let model = &app.model_name;
    // Format cost as $0.0000
    let cost_str = format!("${:.4}", app.cost_usd);
    let tokens = app.token_count;

    let right_info = format!("{}{} | {} | {} tok", spinner, model, cost_str, tokens);

    let left_info = if let Some(ref msg) = app.status_message {
        format!(" {}", msg)
    } else {
        String::new()
    };

    // Bridge state badge
    let bridge_badge = app.bridge_state.status_badge(app.frame_count);
    let bridge_text = bridge_badge
        .as_ref()
        .map(|b| b.content.to_string())
        .unwrap_or_default();

    let total_width = area.width as usize;
    let right_len = right_info.len();
    let left_len = left_info.len();
    let bridge_len = bridge_text.len();
    let gap = total_width
        .saturating_sub(right_len + left_len + bridge_len)
        .saturating_sub(1);

    // Build spans for the status bar
    let mut spans: Vec<Span> = Vec::new();

    // Left status message
    if !left_info.is_empty() {
        spans.push(Span::raw(left_info.clone()));
    }

    // Gap fill
    spans.push(Span::raw(" ".repeat(gap)));

    // Bridge badge (if visible)
    if let Some(badge) = bridge_badge {
        spans.push(badge);
        spans.push(Span::raw(" "));
    }

    // Right info
    spans.push(Span::raw(format!("{} ", right_info)));

    let line = Line::from(spans);
    let bar = Paragraph::new(vec![line])
        .style(Style::default().bg(Color::DarkGray).fg(Color::White));

    frame.render_widget(bar, area);
}

// -----------------------------------------------------------------------
// Legacy simple help overlay (fallback when help_overlay is not open)
// -----------------------------------------------------------------------

fn render_simple_help_overlay(frame: &mut Frame, area: Rect) {
    let help_width = 50u16.min(area.width.saturating_sub(4));
    let help_height = 20u16.min(area.height.saturating_sub(4));
    let help_area = crate::overlays::centered_rect(help_width, help_height, area);

    frame.render_widget(Clear, help_area);

    let lines = vec![
        Line::from(vec![Span::styled(
            " Key Bindings",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        )]),
        Line::from(""),
        kb_line("Enter", "Submit message"),
        kb_line("Ctrl+C", "Cancel streaming / Quit"),
        kb_line("Ctrl+D", "Quit (empty input)"),
        kb_line("Up / Down", "Navigate input history"),
        kb_line("Ctrl+R", "Search input history"),
        kb_line("PageUp / PageDown", "Scroll messages"),
        kb_line("F1 / ?", "Toggle this help"),
        Line::from(""),
        Line::from(vec![Span::styled(
            " Permission Dialog",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        )]),
        Line::from(""),
        kb_line("1 / 2 / 3", "Select option"),
        kb_line("y / a / n", "Allow / Always / Deny"),
        kb_line("Enter", "Confirm selection"),
        kb_line("Esc", "Deny (close dialog)"),
        Line::from(""),
        Line::from(vec![Span::styled(
            " press F1 or ? to close ",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        )]),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Help ")
        .border_style(Style::default().fg(Color::Cyan));

    let para = Paragraph::new(lines)
        .block(block)
        .alignment(Alignment::Left);
    frame.render_widget(para, help_area);
}

fn kb_line<'a>(key: &str, desc: &str) -> Line<'a> {
    Line::from(vec![
        Span::raw("  "),
        Span::styled(
            format!("{:<20}", key),
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(desc.to_string()),
    ])
}

// -----------------------------------------------------------------------
// Legacy history search overlay (used when history_search_overlay is not open)
// -----------------------------------------------------------------------

fn render_legacy_history_search(
    frame: &mut Frame,
    hs: &crate::app::HistorySearch,
    app: &App,
    area: Rect,
) {
    let dialog_width = 60u16.min(area.width.saturating_sub(4));
    let visible_matches = 8usize;
    let dialog_height =
        (4 + visible_matches.min(hs.matches.len().max(1)) as u16).min(area.height.saturating_sub(4));
    let dialog_area = crate::overlays::centered_rect(dialog_width, dialog_height, area);

    frame.render_widget(Clear, dialog_area);

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(vec![
        Span::raw("  Search: "),
        Span::styled(
            hs.query.clone(),
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        ),
        Span::styled("\u{2588}", Style::default().fg(Color::White)),
    ]));
    lines.push(Line::from(""));

    if hs.matches.is_empty() {
        lines.push(Line::from(vec![Span::styled(
            "  (no matches)",
            Style::default().fg(Color::DarkGray),
        )]));
    } else {
        let start = hs.selected.saturating_sub(visible_matches / 2);
        let end = (start + visible_matches).min(hs.matches.len());
        let start = end.saturating_sub(visible_matches).min(start);

        for (display_idx, &hist_idx) in hs.matches[start..end].iter().enumerate() {
            let real_idx = start + display_idx;
            let is_selected = real_idx == hs.selected;
            let entry = app
                .input_history
                .get(hist_idx)
                .map(String::as_str)
                .unwrap_or("");

            let truncated = if UnicodeWidthStr::width(entry) > (dialog_width as usize - 6) {
                let mut s = entry.to_string();
                s.truncate(dialog_width as usize - 9);
                format!("{}\u{2026}", s)
            } else {
                entry.to_string()
            };

            let (prefix, style) = if is_selected {
                (
                    "  \u{25BA} ",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                ("    ", Style::default().fg(Color::White))
            };
            lines.push(Line::from(vec![
                Span::raw(prefix),
                Span::styled(truncated, style),
            ]));
        }
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" History Search (Esc to cancel) ")
        .border_style(Style::default().fg(Color::Cyan));

    let para = Paragraph::new(lines).block(block);
    frame.render_widget(para, dialog_area);
}
