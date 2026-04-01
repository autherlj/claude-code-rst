// overlays.rs — All full-screen and floating overlays:
//   - HelpOverlay (? / F1 / /help)
//   - HistorySearchOverlay (Ctrl+R)
//   - MessageSelectorOverlay (/rewind step 1)
//   - RewindFlowOverlay (/rewind full multi-step flow)

use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;
use unicode_width::UnicodeWidthStr;

// ---------------------------------------------------------------------------
// Geometry helper (shared)
// ---------------------------------------------------------------------------

/// Compute a centred `Rect` of the given `width` × `height` inside `area`.
pub fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect {
        x,
        y,
        width: width.min(area.width),
        height: height.min(area.height),
    }
}

// ============================================================================
// HelpOverlay
// ============================================================================

/// State for the full-screen help overlay (? / F1 / /help).
#[derive(Debug, Default)]
pub struct HelpOverlay {
    pub visible: bool,
    pub scroll_offset: u16,
    /// Live search filter — only commands matching this substring are shown.
    pub filter: String,
}

/// A single command entry shown in the help overlay.
#[derive(Debug, Clone)]
pub struct HelpEntry {
    pub name: &'static str,
    /// Comma-separated aliases, e.g. "h, ?"
    pub aliases: &'static str,
    pub description: &'static str,
    pub category: &'static str,
}

/// All built-in commands exposed in the help overlay.
pub const HELP_ENTRIES: &[HelpEntry] = &[
    HelpEntry { name: "help",    aliases: "h",      description: "Show this help",                              category: "General"      },
    HelpEntry { name: "clear",   aliases: "",        description: "Clear the conversation",                     category: "General"      },
    HelpEntry { name: "exit",    aliases: "quit, q", description: "Exit Claude Code",                           category: "General"      },
    HelpEntry { name: "compact", aliases: "",        description: "Compact the conversation with a summary",    category: "General"      },
    HelpEntry { name: "cost",    aliases: "",        description: "Show token usage and cost",                  category: "General"      },
    HelpEntry { name: "version", aliases: "",        description: "Show version information",                   category: "General"      },
    HelpEntry { name: "model",   aliases: "",        description: "Get or set the AI model",                    category: "Configuration" },
    HelpEntry { name: "config",  aliases: "",        description: "View or edit configuration",                 category: "Configuration" },
    HelpEntry { name: "color",   aliases: "",        description: "Set the theme color",                        category: "Configuration" },
    HelpEntry { name: "rewind",  aliases: "",        description: "Rewind the conversation to an earlier point", category: "Conversation" },
    HelpEntry { name: "resume",  aliases: "",        description: "Resume a previous session",                  category: "Conversation" },
    HelpEntry { name: "status",  aliases: "",        description: "Show session status",                        category: "Conversation" },
    HelpEntry { name: "diff",    aliases: "",        description: "Show file diffs since session start",        category: "Conversation" },
    HelpEntry { name: "memory",  aliases: "",        description: "Manage memory / CLAUDE.md files",            category: "Memory"       },
];

impl HelpOverlay {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn toggle(&mut self) {
        self.visible = !self.visible;
        if !self.visible {
            // Reset state when closing
            self.scroll_offset = 0;
            self.filter.clear();
        }
    }

    pub fn close(&mut self) {
        self.visible = false;
        self.scroll_offset = 0;
        self.filter.clear();
    }

    pub fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
    }

    pub fn scroll_down(&mut self, max: u16) {
        if self.scroll_offset + 1 < max {
            self.scroll_offset += 1;
        }
    }

    pub fn push_filter_char(&mut self, c: char) {
        self.filter.push(c);
        self.scroll_offset = 0;
    }

    pub fn pop_filter_char(&mut self) {
        self.filter.pop();
        self.scroll_offset = 0;
    }
}

/// Render the help overlay into the frame.
pub fn render_help_overlay(frame: &mut Frame, overlay: &HelpOverlay, area: Rect) {
    if !overlay.visible {
        return;
    }

    let dialog_width = 70u16.min(area.width.saturating_sub(4));
    let dialog_height = 28u16.min(area.height.saturating_sub(4));
    let dialog_area = centered_rect(dialog_width, dialog_height, area);

    frame.render_widget(Clear, dialog_area);

    let mut lines: Vec<Line> = Vec::new();

    // --- Header ----------------------------------------------------------
    lines.push(Line::from(vec![Span::styled(
        " Commands",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
    )]));
    if !overlay.filter.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("  Filter: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                overlay.filter.clone(),
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
            ),
        ]));
    }
    lines.push(Line::from(""));

    // --- Keyboard shortcuts section ---------------------------------------
    let show_kb = overlay.filter.is_empty();
    if show_kb {
        lines.push(Line::from(vec![Span::styled(
            " Keyboard Shortcuts",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        )]));
        lines.push(Line::from(""));
        for (key, desc) in &[
            ("Enter",           "Submit message"),
            ("Ctrl+C",          "Cancel streaming / Quit"),
            ("Ctrl+D",          "Quit (empty input)"),
            ("Up / Down",       "Navigate input history"),
            ("Ctrl+R",          "Search input history"),
            ("PageUp / PgDn",   "Scroll messages"),
            ("F1 / ?",          "Toggle this help"),
            ("Esc",             "Close overlay / cancel"),
        ] {
            lines.push(kb_line(key, desc));
        }
        lines.push(Line::from(""));
    }

    // --- Commands by category --------------------------------------------
    let filter_lc = overlay.filter.to_lowercase();
    let filtered: Vec<&HelpEntry> = HELP_ENTRIES
        .iter()
        .filter(|e| {
            filter_lc.is_empty()
                || e.name.contains(filter_lc.as_str())
                || e.aliases.to_lowercase().contains(filter_lc.as_str())
                || e.description.to_lowercase().contains(filter_lc.as_str())
        })
        .collect();

    let mut current_cat = "";
    for entry in &filtered {
        if entry.category != current_cat {
            current_cat = entry.category;
            if !lines.is_empty() {
                lines.push(Line::from(""));
            }
            lines.push(Line::from(vec![Span::styled(
                format!(" {}", entry.category),
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            )]));
            lines.push(Line::from(""));
        }

        let aliases_text = if entry.aliases.is_empty() {
            String::new()
        } else {
            format!(" ({})", entry.aliases)
        };
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                format!("/{:<15}", entry.name),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                aliases_text,
                Style::default().fg(Color::DarkGray),
            ),
            Span::raw("  "),
            Span::raw(entry.description.to_string()),
        ]));
    }

    if filtered.is_empty() {
        lines.push(Line::from(vec![Span::styled(
            "  (no matching commands)",
            Style::default().fg(Color::DarkGray),
        )]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![Span::styled(
        " Type to filter  ·  Esc / ? to close  ·  ↑↓ to scroll",
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::ITALIC),
    )]));

    let total_lines = lines.len() as u16;
    let inner_height = dialog_height.saturating_sub(2);
    let max_scroll = total_lines.saturating_sub(inner_height);
    let scroll = overlay.scroll_offset.min(max_scroll);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Help — Claude Code ")
        .border_style(Style::default().fg(Color::Cyan));

    let para = Paragraph::new(lines)
        .block(block)
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0));

    frame.render_widget(para, dialog_area);
}

// ============================================================================
// HistorySearchOverlay
// ============================================================================

/// State for the Ctrl+R history search floating panel.
#[derive(Debug, Default)]
pub struct HistorySearchOverlay {
    pub visible: bool,
    pub query: String,
    /// Indices into the app's `input_history` that match the query.
    pub matches: Vec<usize>,
    pub selected_idx: usize,
}

impl HistorySearchOverlay {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn open(history: &[String]) -> Self {
        let mut s = Self {
            visible: true,
            query: String::new(),
            matches: Vec::new(),
            selected_idx: 0,
        };
        s.update_matches(history);
        s
    }

    /// Recompute `matches` based on `query` against the given `history` slice.
    pub fn update_matches(&mut self, history: &[String]) {
        let q = self.query.to_lowercase();
        self.matches = history
            .iter()
            .enumerate()
            .filter_map(|(i, s)| {
                if q.is_empty() || s.to_lowercase().contains(&q) {
                    Some(i)
                } else {
                    None
                }
            })
            .collect();
        // clamp selection
        if !self.matches.is_empty() && self.selected_idx >= self.matches.len() {
            self.selected_idx = self.matches.len() - 1;
        }
    }

    pub fn push_char(&mut self, c: char, history: &[String]) {
        self.query.push(c);
        self.selected_idx = 0;
        self.update_matches(history);
    }

    pub fn pop_char(&mut self, history: &[String]) {
        self.query.pop();
        self.selected_idx = 0;
        self.update_matches(history);
    }

    pub fn select_prev(&mut self) {
        if self.selected_idx > 0 {
            self.selected_idx -= 1;
        }
    }

    pub fn select_next(&mut self) {
        let max = self.matches.len().saturating_sub(1);
        if self.selected_idx < max {
            self.selected_idx += 1;
        }
    }

    /// Return the currently selected history entry text, if any.
    pub fn current_entry<'a>(&self, history: &'a [String]) -> Option<&'a str> {
        self.matches
            .get(self.selected_idx)
            .and_then(|&i| history.get(i))
            .map(String::as_str)
    }

    pub fn close(&mut self) {
        self.visible = false;
    }
}

/// Render the history search floating panel.
pub fn render_history_search_overlay(
    frame: &mut Frame,
    overlay: &HistorySearchOverlay,
    history: &[String],
    area: Rect,
) {
    if !overlay.visible {
        return;
    }

    const VISIBLE_MATCHES: usize = 8;
    let dialog_width = 64u16.min(area.width.saturating_sub(4));
    let match_count = overlay.matches.len().max(1);
    let rows = VISIBLE_MATCHES.min(match_count) as u16;
    let dialog_height = (4 + rows).min(area.height.saturating_sub(4));
    let dialog_area = centered_rect(dialog_width, dialog_height, area);

    frame.render_widget(Clear, dialog_area);

    let mut lines: Vec<Line> = Vec::new();

    // Search query line
    lines.push(Line::from(vec![
        Span::raw("  Search: "),
        Span::styled(
            overlay.query.clone(),
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        ),
        Span::styled("\u{2588}", Style::default().fg(Color::White)),
    ]));
    lines.push(Line::from(""));

    if overlay.matches.is_empty() {
        lines.push(Line::from(vec![Span::styled(
            "  (no matches)",
            Style::default().fg(Color::DarkGray),
        )]));
    } else {
        let start = overlay
            .selected_idx
            .saturating_sub(VISIBLE_MATCHES / 2)
            .min(overlay.matches.len().saturating_sub(VISIBLE_MATCHES));
        let end = (start + VISIBLE_MATCHES).min(overlay.matches.len());

        for (display_i, &hist_idx) in overlay.matches[start..end].iter().enumerate() {
            let real_i = start + display_i;
            let is_selected = real_i == overlay.selected_idx;
            let entry = history.get(hist_idx).map(String::as_str).unwrap_or("");

            let max_chars = dialog_width as usize - 6;
            let truncated = if UnicodeWidthStr::width(entry) > max_chars {
                let mut s = entry.to_string();
                s.truncate(max_chars.saturating_sub(1));
                format!("{}…", s)
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

// ============================================================================
// MessageSelectorOverlay
// ============================================================================

/// A single entry shown in the message selector list.
#[derive(Debug, Clone)]
pub struct SelectorMessage {
    /// Original index in the conversation.
    pub idx: usize,
    pub role: String,
    /// First ~80 chars of content.
    pub preview: String,
    pub has_tool_use: bool,
}

/// State for the message selector overlay used by /rewind step 1.
#[derive(Debug, Default)]
pub struct MessageSelectorOverlay {
    pub visible: bool,
    pub messages: Vec<SelectorMessage>,
    pub selected_idx: usize,
    pub scroll_offset: usize,
}

impl MessageSelectorOverlay {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn open(messages: Vec<SelectorMessage>) -> Self {
        // Start with selection at the end (most recent)
        let selected = messages.len().saturating_sub(1);
        Self {
            visible: true,
            messages,
            selected_idx: selected,
            scroll_offset: selected.saturating_sub(5),
        }
    }

    pub fn close(&mut self) {
        self.visible = false;
    }

    pub fn select_prev(&mut self) {
        if self.selected_idx > 0 {
            self.selected_idx -= 1;
            // Scroll up if needed
            if self.selected_idx < self.scroll_offset {
                self.scroll_offset = self.selected_idx;
            }
        }
    }

    pub fn select_next(&mut self) {
        if self.selected_idx + 1 < self.messages.len() {
            self.selected_idx += 1;
        }
    }

    pub fn current_message(&self) -> Option<&SelectorMessage> {
        self.messages.get(self.selected_idx)
    }
}

/// Render the message selector overlay.
pub fn render_message_selector(frame: &mut Frame, overlay: &MessageSelectorOverlay, area: Rect) {
    if !overlay.visible {
        return;
    }

    const VISIBLE_ROWS: usize = 12;
    let dialog_width = 70u16.min(area.width.saturating_sub(4));
    let rows = VISIBLE_ROWS.min(overlay.messages.len().max(1)) as u16;
    let dialog_height = (rows + 4).min(area.height.saturating_sub(4));
    let dialog_area = centered_rect(dialog_width, dialog_height, area);

    frame.render_widget(Clear, dialog_area);

    let mut lines: Vec<Line> = Vec::new();

    lines.push(Line::from(vec![Span::styled(
        "  Select a message to rewind to:",
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
    )]));
    lines.push(Line::from(""));

    if overlay.messages.is_empty() {
        lines.push(Line::from(vec![Span::styled(
            "  (no messages)",
            Style::default().fg(Color::DarkGray),
        )]));
    } else {
        let start = overlay.scroll_offset;
        let end = (start + VISIBLE_ROWS).min(overlay.messages.len());

        for (display_i, msg) in overlay.messages[start..end].iter().enumerate() {
            let real_i = start + display_i;
            let is_selected = real_i == overlay.selected_idx;

            let role_color = if msg.role == "user" {
                Color::Cyan
            } else {
                Color::Green
            };

            let tool_tag = if msg.has_tool_use { " [tool]" } else { "" };

            let preview_max = dialog_width as usize - 20;
            let preview = if UnicodeWidthStr::width(msg.preview.as_str()) > preview_max {
                format!("{}…", &msg.preview[..preview_max.saturating_sub(1)])
            } else {
                msg.preview.clone()
            };

            let prefix = if is_selected { "  \u{25BA} " } else { "    " };
            let idx_style = if is_selected {
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            };

            lines.push(Line::from(vec![
                Span::raw(prefix),
                Span::styled(format!("{:>3}. ", msg.idx), idx_style),
                Span::styled(
                    format!("{:<10}", msg.role),
                    Style::default().fg(role_color).add_modifier(if is_selected {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
                ),
                Span::styled(
                    preview,
                    if is_selected {
                        Style::default().fg(Color::White)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    },
                ),
                Span::styled(
                    tool_tag.to_string(),
                    Style::default().fg(Color::Yellow),
                ),
            ]));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![Span::styled(
        "  ↑↓ navigate  ·  Enter to select  ·  Esc to cancel",
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::ITALIC),
    )]));

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Rewind — Select Message ")
        .border_style(Style::default().fg(Color::Yellow));

    let para = Paragraph::new(lines).block(block);
    frame.render_widget(para, dialog_area);
}

// ============================================================================
// RewindFlowOverlay  (multi-step: select → confirm → done)
// ============================================================================

/// The current step in the rewind flow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RewindStep {
    /// Step 1: user is browsing the message list.
    Selecting,
    /// Step 2: user has chosen a message and must confirm.
    Confirming { message_idx: usize },
}

/// Full multi-step overlay for the /rewind command.
#[derive(Debug)]
pub struct RewindFlowOverlay {
    pub visible: bool,
    pub step: RewindStep,
    pub selector: MessageSelectorOverlay,
}

impl Default for RewindFlowOverlay {
    fn default() -> Self {
        Self {
            visible: false,
            step: RewindStep::Selecting,
            selector: MessageSelectorOverlay::new(),
        }
    }
}

impl RewindFlowOverlay {
    pub fn new() -> Self {
        Self::default()
    }

    /// Open the overlay with the given conversation messages.
    pub fn open(&mut self, messages: Vec<SelectorMessage>) {
        self.selector = MessageSelectorOverlay::open(messages);
        self.step = RewindStep::Selecting;
        self.visible = true;
    }

    pub fn close(&mut self) {
        self.visible = false;
        self.selector.close();
        self.step = RewindStep::Selecting;
    }

    /// Confirm the current selection; advances to the `Confirming` step.
    /// Returns the selected message index if in the Selecting step.
    pub fn confirm_selection(&mut self) -> Option<usize> {
        if self.step == RewindStep::Selecting {
            if let Some(msg) = self.selector.current_message() {
                let idx = msg.idx;
                self.step = RewindStep::Confirming { message_idx: idx };
                return Some(idx);
            }
        }
        None
    }

    /// The user pressed 'y' in the Confirming step.
    /// Returns the final message index to rewind to.
    pub fn accept_confirm(&mut self) -> Option<usize> {
        if let RewindStep::Confirming { message_idx } = self.step {
            self.close();
            return Some(message_idx);
        }
        None
    }

    /// The user pressed 'n' or Esc in the Confirming step — go back to selector.
    pub fn reject_confirm(&mut self) {
        if matches!(self.step, RewindStep::Confirming { .. }) {
            self.step = RewindStep::Selecting;
        }
    }
}

/// Render the full rewind flow overlay.
pub fn render_rewind_flow(frame: &mut Frame, overlay: &RewindFlowOverlay, area: Rect) {
    if !overlay.visible {
        return;
    }

    match &overlay.step {
        RewindStep::Selecting => {
            render_message_selector(frame, &overlay.selector, area);
        }
        RewindStep::Confirming { message_idx } => {
            render_rewind_confirm(frame, *message_idx, area);
        }
    }
}

fn render_rewind_confirm(frame: &mut Frame, message_idx: usize, area: Rect) {
    let dialog_width = 50u16.min(area.width.saturating_sub(4));
    let dialog_height = 7u16.min(area.height.saturating_sub(4));
    let dialog_area = centered_rect(dialog_width, dialog_height, area);

    frame.render_widget(Clear, dialog_area);

    let lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::raw("  Rewind to message "),
            Span::styled(
                format!("#{}", message_idx),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("?"),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "  [y] ",
                Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
            ),
            Span::raw("Yes, rewind"),
            Span::raw("    "),
            Span::styled(
                "[n] ",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
            Span::raw("Cancel"),
        ]),
        Line::from(""),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Confirm Rewind ")
        .border_style(Style::default().fg(Color::Yellow));

    let para = Paragraph::new(lines).block(block);
    frame.render_widget(para, dialog_area);
}

// ---------------------------------------------------------------------------
// Shared helper
// ---------------------------------------------------------------------------

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

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // --- HelpOverlay ---------------------------------------------------

    #[test]
    fn help_overlay_toggle() {
        let mut h = HelpOverlay::new();
        assert!(!h.visible);
        h.toggle();
        assert!(h.visible);
        h.toggle();
        assert!(!h.visible);
    }

    #[test]
    fn help_overlay_close_resets_state() {
        let mut h = HelpOverlay::new();
        h.visible = true;
        h.scroll_offset = 5;
        h.filter = "foo".to_string();
        h.close();
        assert!(!h.visible);
        assert_eq!(h.scroll_offset, 0);
        assert!(h.filter.is_empty());
    }

    #[test]
    fn help_overlay_filter() {
        let mut h = HelpOverlay::new();
        h.push_filter_char('h', );
        h.push_filter_char('e', );
        assert_eq!(h.filter, "he");
        h.pop_filter_char();
        assert_eq!(h.filter, "h");
    }

    // --- HistorySearchOverlay -----------------------------------------

    #[test]
    fn history_search_update_matches() {
        // "cargo build" also contains 'g', so all three match a single-char 'g' query.
        let history = vec!["git commit".to_string(), "cargo build".to_string(), "git push".to_string()];
        let mut hs = HistorySearchOverlay::open(&history);
        hs.push_char('g', &history);
        // All three entries contain 'g' (cargo, git×2)
        assert_eq!(hs.matches.len(), 3);

        // Narrowing to "gi" matches only the git entries
        hs.push_char('i', &history);
        assert_eq!(hs.matches.len(), 2);
        // Should be "git commit" (idx 0) and "git push" (idx 2)
        assert!(hs.matches.contains(&0));
        assert!(hs.matches.contains(&2));
    }

    #[test]
    fn history_search_navigation() {
        let history = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let mut hs = HistorySearchOverlay::open(&history);
        assert_eq!(hs.selected_idx, 0);
        hs.select_next();
        assert_eq!(hs.selected_idx, 1);
        hs.select_prev();
        assert_eq!(hs.selected_idx, 0);
    }

    #[test]
    fn history_search_current_entry() {
        let history = vec!["first".to_string(), "second".to_string()];
        let mut hs = HistorySearchOverlay::open(&history);
        hs.selected_idx = 1;
        assert_eq!(hs.current_entry(&history), Some("second"));
    }

    // --- MessageSelectorOverlay ---------------------------------------

    #[test]
    fn message_selector_open_selects_last() {
        let msgs = vec![
            SelectorMessage { idx: 0, role: "user".to_string(), preview: "hi".to_string(), has_tool_use: false },
            SelectorMessage { idx: 1, role: "assistant".to_string(), preview: "hello".to_string(), has_tool_use: false },
        ];
        let sel = MessageSelectorOverlay::open(msgs);
        assert_eq!(sel.selected_idx, 1);
    }

    #[test]
    fn message_selector_navigate() {
        let msgs = vec![
            SelectorMessage { idx: 0, role: "user".to_string(), preview: "a".to_string(), has_tool_use: false },
            SelectorMessage { idx: 1, role: "assistant".to_string(), preview: "b".to_string(), has_tool_use: false },
            SelectorMessage { idx: 2, role: "user".to_string(), preview: "c".to_string(), has_tool_use: false },
        ];
        let mut sel = MessageSelectorOverlay::open(msgs);
        // starts at last
        assert_eq!(sel.selected_idx, 2);
        sel.select_prev();
        assert_eq!(sel.selected_idx, 1);
        sel.select_next();
        assert_eq!(sel.selected_idx, 2);
    }

    // --- RewindFlowOverlay -------------------------------------------

    #[test]
    fn rewind_flow_confirm_advances_step() {
        let msgs = vec![
            SelectorMessage { idx: 0, role: "user".to_string(), preview: "hi".to_string(), has_tool_use: false },
        ];
        let mut flow = RewindFlowOverlay::new();
        flow.open(msgs);
        let idx = flow.confirm_selection().unwrap();
        assert_eq!(idx, 0);
        assert!(matches!(flow.step, RewindStep::Confirming { message_idx: 0 }));
    }

    #[test]
    fn rewind_flow_accept_closes() {
        let msgs = vec![
            SelectorMessage { idx: 3, role: "user".to_string(), preview: "test".to_string(), has_tool_use: false },
        ];
        let mut flow = RewindFlowOverlay::new();
        flow.open(msgs);
        flow.confirm_selection();
        let result = flow.accept_confirm().unwrap();
        assert_eq!(result, 3);
        assert!(!flow.visible);
    }

    #[test]
    fn rewind_flow_reject_returns_to_selector() {
        let msgs = vec![
            SelectorMessage { idx: 0, role: "user".to_string(), preview: "x".to_string(), has_tool_use: false },
        ];
        let mut flow = RewindFlowOverlay::new();
        flow.open(msgs);
        flow.confirm_selection();
        assert!(matches!(flow.step, RewindStep::Confirming { .. }));
        flow.reject_confirm();
        assert_eq!(flow.step, RewindStep::Selecting);
        assert!(flow.visible);
    }
}
