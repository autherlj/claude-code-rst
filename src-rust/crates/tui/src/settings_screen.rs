// settings_screen.rs — Full-screen tabbed settings interface.
//
// Opened by /config or /settings commands. Provides a tabbed UI for
// viewing and editing General, Display, Privacy, Advanced, and KeyBindings
// settings. Changes are persisted via Settings::save_sync().

use cc_core::config::{Config, Settings};
use cc_core::keybindings::default_bindings;
use cc_core::output_styles::builtin_styles;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Tabs, Wrap};
use ratatui::Frame;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsTab {
    General,
    Display,
    Privacy,
    Advanced,
    KeyBindings,
}

impl SettingsTab {
    pub fn all() -> &'static [SettingsTab] {
        &[
            SettingsTab::General,
            SettingsTab::Display,
            SettingsTab::Privacy,
            SettingsTab::Advanced,
            SettingsTab::KeyBindings,
        ]
    }

    pub fn label(&self) -> &'static str {
        match self {
            SettingsTab::General => "General",
            SettingsTab::Display => "Display",
            SettingsTab::Privacy => "Privacy",
            SettingsTab::Advanced => "Advanced",
            SettingsTab::KeyBindings => "KeyBindings",
        }
    }

    pub fn index(&self) -> usize {
        Self::all().iter().position(|t| t == self).unwrap_or(0)
    }
}

pub struct SettingsScreen {
    pub visible: bool,
    pub active_tab: SettingsTab,
    pub scroll_offset: u16,
    /// Which field is being edited (field name as key).
    pub edit_field: Option<String>,
    /// Current buffer content while editing a field.
    pub edit_value: String,
    /// Snapshot of settings at open time for display.
    pub settings_snapshot: Settings,
    /// Pending changes (field_name → new_value string).
    pub pending_changes: std::collections::HashMap<String, String>,
}

impl SettingsScreen {
    pub fn new() -> Self {
        let settings_snapshot = Settings::load_sync().unwrap_or_default();
        Self {
            visible: false,
            active_tab: SettingsTab::General,
            scroll_offset: 0,
            edit_field: None,
            edit_value: String::new(),
            settings_snapshot,
            pending_changes: std::collections::HashMap::new(),
        }
    }

    pub fn open(&mut self) {
        self.settings_snapshot = Settings::load_sync().unwrap_or_default();
        self.pending_changes.clear();
        self.edit_field = None;
        self.edit_value.clear();
        self.scroll_offset = 0;
        self.active_tab = SettingsTab::General;
        self.visible = true;
    }

    pub fn close(&mut self) {
        self.visible = false;
        self.edit_field = None;
        self.edit_value.clear();
    }

    pub fn next_tab(&mut self) {
        let idx = self.active_tab.index();
        let next = (idx + 1) % SettingsTab::all().len();
        self.active_tab = SettingsTab::all()[next].clone();
        self.scroll_offset = 0;
        self.edit_field = None;
        self.edit_value.clear();
    }

    pub fn prev_tab(&mut self) {
        let idx = self.active_tab.index();
        let prev = if idx == 0 {
            SettingsTab::all().len() - 1
        } else {
            idx - 1
        };
        self.active_tab = SettingsTab::all()[prev].clone();
        self.scroll_offset = 0;
        self.edit_field = None;
        self.edit_value.clear();
    }

    pub fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
    }

    pub fn scroll_down(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_add(1);
    }

    /// Start editing a field by name, seeding the buffer with current value.
    pub fn start_edit(&mut self, field: &str, current_value: &str) {
        self.edit_field = Some(field.to_string());
        self.edit_value = current_value.to_string();
    }

    /// Commit the current edit to pending_changes.
    pub fn commit_edit(&mut self) {
        if let Some(field) = self.edit_field.take() {
            let value = std::mem::take(&mut self.edit_value);
            self.pending_changes.insert(field, value);
        }
    }

    /// Discard the current edit.
    pub fn cancel_edit(&mut self) {
        self.edit_field = None;
        self.edit_value.clear();
    }

    /// Apply all pending changes to settings and persist them.
    pub fn apply_and_save(&mut self, config: &mut Config) {
        for (field, value) in &self.pending_changes {
            match field.as_str() {
                "model" => {
                    config.model = if value.is_empty() {
                        None
                    } else {
                        Some(value.clone())
                    };
                }
                "max_tokens" => {
                    if let Ok(n) = value.parse::<u32>() {
                        config.max_tokens = Some(n);
                    }
                }
                "output_style" => {
                    config.output_style = if value.is_empty() {
                        None
                    } else {
                        Some(value.clone())
                    };
                }
                _ => {}
            }
        }
        // Update snapshot and persist
        self.settings_snapshot.config = config.clone();
        let _ = self.settings_snapshot.save_sync();
        self.pending_changes.clear();
    }
}

impl Default for SettingsScreen {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

/// Render the settings screen (full-screen popup) into `frame`.
pub fn render_settings_screen(frame: &mut Frame, screen: &SettingsScreen, area: Rect) {
    if !screen.visible {
        return;
    }

    // 80% width, 90% height, centred
    let w = (area.width * 4 / 5).max(60).min(area.width.saturating_sub(2));
    let h = (area.height * 9 / 10).max(20).min(area.height.saturating_sub(2));
    let x = area.x + area.width.saturating_sub(w) / 2;
    let y = area.y + area.height.saturating_sub(h) / 2;
    let popup = Rect {
        x,
        y,
        width: w,
        height: h,
    };

    frame.render_widget(Clear, popup);

    // Outer border
    let outer_block = Block::default()
        .borders(Borders::ALL)
        .title(" Settings — Claude Code ")
        .border_style(Style::default().fg(Color::Cyan));
    frame.render_widget(outer_block, popup);

    // Inset inner area
    let inner = Rect {
        x: popup.x + 1,
        y: popup.y + 1,
        width: popup.width.saturating_sub(2),
        height: popup.height.saturating_sub(2),
    };

    if inner.height < 4 {
        return;
    }

    // Split into tabs row + content
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let tabs_area = layout[0];
    let content_area = layout[1];
    let footer_area = layout[2];

    // Tabs bar
    let tab_labels: Vec<Line> = SettingsTab::all()
        .iter()
        .map(|t| {
            if *t == screen.active_tab {
                Line::from(vec![Span::styled(
                    format!(" {} ", t.label()),
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )])
            } else {
                Line::from(vec![Span::styled(
                    format!(" {} ", t.label()),
                    Style::default().fg(Color::DarkGray),
                )])
            }
        })
        .collect();

    let tabs = Tabs::new(tab_labels)
        .divider(Span::raw(" │ "))
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(tabs, tabs_area);

    // Tab content
    render_tab_content(frame, screen, content_area);

    // Footer
    let footer = if screen.edit_field.is_some() {
        Line::from(vec![
            Span::styled(" Enter ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::raw("save  "),
            Span::styled(" Esc ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("cancel"),
        ])
    } else {
        Line::from(vec![
            Span::styled(" Tab ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::raw("next tab  "),
            Span::styled(" Enter ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::raw("edit  "),
            Span::styled(" Esc ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("close"),
        ])
    };
    let footer_para = Paragraph::new(vec![footer])
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);
    frame.render_widget(footer_para, footer_area);
}

fn render_tab_content(frame: &mut Frame, screen: &SettingsScreen, area: Rect) {
    let lines = match &screen.active_tab {
        SettingsTab::General => build_general_lines(screen),
        SettingsTab::Display => build_display_lines(screen),
        SettingsTab::Privacy => build_privacy_lines(screen),
        SettingsTab::Advanced => build_advanced_lines(screen),
        SettingsTab::KeyBindings => build_keybindings_lines(screen),
    };

    let total = lines.len() as u16;
    let visible = area.height;
    let max_scroll = total.saturating_sub(visible);
    let scroll = screen.scroll_offset.min(max_scroll);

    let para = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0));
    frame.render_widget(para, area);
}

// ---------------------------------------------------------------------------
// General tab
// ---------------------------------------------------------------------------

fn build_general_lines(screen: &SettingsScreen) -> Vec<Line<'static>> {
    let cfg = &screen.settings_snapshot.config;
    let mut lines: Vec<Line<'static>> = Vec::new();

    lines.push(section_header("General Settings"));
    lines.push(Line::from(""));

    // Model
    let model_val = cfg.model.clone().unwrap_or_else(|| {
        cc_core::constants::DEFAULT_MODEL.to_string()
    });
    lines.extend(field_lines(
        "model",
        "Model",
        &model_val,
        "AI model used for responses.",
        screen,
    ));
    // Show available models hint
    lines.push(indent_line(
        "  Available: claude-opus-4-6, claude-sonnet-4-6, claude-haiku-4-5-20251001",
        Color::DarkGray,
    ));
    lines.push(Line::from(""));

    // Max tokens
    let max_tokens_val = cfg
        .max_tokens
        .map(|n| n.to_string())
        .unwrap_or_else(|| cc_core::constants::DEFAULT_MAX_TOKENS.to_string());
    lines.extend(field_lines(
        "max_tokens",
        "Max Tokens",
        &max_tokens_val,
        "Maximum tokens per response.",
        screen,
    ));
    lines.push(Line::from(""));

    // Output style
    let style_names: Vec<String> = builtin_styles().into_iter().map(|s| s.name).collect();
    let output_style_val = cfg
        .output_style
        .clone()
        .unwrap_or_else(|| "default".to_string());
    lines.extend(field_lines(
        "output_style",
        "Output Style",
        &output_style_val,
        "Controls the verbosity and format of responses.",
        screen,
    ));
    lines.push(indent_line(
        &format!("  Available: {}", style_names.join(", ")),
        Color::DarkGray,
    ));
    lines.push(Line::from(""));

    // Working directory
    let wd = cfg
        .project_dir
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| std::env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "(unknown)".to_string()));
    lines.push(label_value_line("Working Directory", &wd));
    lines.push(indent_line("  (Set via --project-dir flag)", Color::DarkGray));
    lines.push(Line::from(""));

    // Auto compact
    let auto_compact = if cfg.auto_compact { "enabled" } else { "disabled" };
    lines.push(label_value_line("Auto Compact", auto_compact));
    lines.push(indent_line("  Automatically compact conversation when context limit is near.", Color::DarkGray));
    lines.push(Line::from(""));

    lines
}

// ---------------------------------------------------------------------------
// Display tab
// ---------------------------------------------------------------------------

fn build_display_lines(screen: &SettingsScreen) -> Vec<Line<'static>> {
    let cfg = &screen.settings_snapshot.config;
    let mut lines: Vec<Line<'static>> = Vec::new();

    lines.push(section_header("Display Settings"));
    lines.push(Line::from(""));

    // Theme
    let theme_name = match &cfg.theme {
        cc_core::config::Theme::Default => "default",
        cc_core::config::Theme::Dark => "dark",
        cc_core::config::Theme::Light => "light",
        cc_core::config::Theme::Custom(s) => s.as_str(),
    };
    lines.push(label_value_line("Theme", theme_name));
    lines.push(indent_line("  Options: default, dark, light  (use /theme to change)", Color::DarkGray));
    lines.push(Line::from(""));

    // Output format
    let fmt = match &cfg.output_format {
        cc_core::config::OutputFormat::Text => "text",
        cc_core::config::OutputFormat::Json => "json",
        cc_core::config::OutputFormat::StreamJson => "stream-json",
    };
    lines.push(label_value_line("Output Format", fmt));
    lines.push(indent_line("  Options: text, json, stream-json", Color::DarkGray));
    lines.push(Line::from(""));

    // Verbose
    let verbose = if cfg.verbose { "yes" } else { "no" };
    lines.push(label_value_line("Verbose Mode", verbose));
    lines.push(indent_line("  Shows additional debug information during queries.", Color::DarkGray));
    lines.push(Line::from(""));

    // Output styles section
    lines.push(section_header("Available Output Styles"));
    lines.push(Line::from(""));
    for style in builtin_styles() {
        let active = cfg.output_style.as_deref() == Some(&style.name)
            || (cfg.output_style.is_none() && style.name == "default");
        let marker = if active { " *" } else { "  " };
        lines.push(Line::from(vec![
            Span::styled(
                format!("{}  {:<15}", marker, style.name),
                Style::default()
                    .fg(if active { Color::Cyan } else { Color::White })
                    .add_modifier(if active { Modifier::BOLD } else { Modifier::empty() }),
            ),
            Span::styled(style.description.clone(), Style::default().fg(Color::DarkGray)),
        ]));
    }
    lines.push(Line::from(""));

    let _ = cfg; // suppress unused warning
    lines
}

// ---------------------------------------------------------------------------
// Privacy tab
// ---------------------------------------------------------------------------

fn build_privacy_lines(screen: &SettingsScreen) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();

    lines.push(section_header("Privacy Settings"));
    lines.push(Line::from(""));

    lines.push(Line::from(vec![Span::styled(
        "  These settings control data sharing with Anthropic.",
        Style::default().fg(Color::DarkGray),
    )]));
    lines.push(Line::from(""));

    // Telemetry
    let telemetry = screen.settings_snapshot.config.verbose; // placeholder — verbose is closest bool
    privacy_toggle_lines(&mut lines, "Telemetry", false,
        "Sends anonymised usage statistics to help improve Claude Code.");

    // Usage sharing
    privacy_toggle_lines(&mut lines, "Usage Sharing", false,
        "Shares aggregate usage data for product improvement.");

    // API request logging
    privacy_toggle_lines(&mut lines, "API Request Logging", false,
        "Logs API requests locally for debugging purposes.");

    lines.push(Line::from(""));
    lines.push(Line::from(vec![Span::styled(
        "  Note: Edit ~/.claude/settings.json to toggle these values.",
        Style::default().fg(Color::Yellow).add_modifier(Modifier::ITALIC),
    )]));
    lines.push(Line::from(""));
    lines.push(Line::from(vec![Span::styled(
        "  For full privacy policy see: https://www.anthropic.com/privacy",
        Style::default().fg(Color::DarkGray),
    )]));

    let _ = telemetry;
    lines
}

fn privacy_toggle_lines(lines: &mut Vec<Line<'static>>, name: &str, enabled: bool, desc: &str) {
    let (toggle_text, toggle_color) = if enabled {
        (" ON  ", Color::Green)
    } else {
        (" OFF ", Color::Red)
    };
    lines.push(Line::from(vec![
        Span::styled(
            format!("  {:<25}", name),
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("[{}]", toggle_text),
            Style::default()
                .fg(toggle_color)
                .add_modifier(Modifier::BOLD),
        ),
    ]));
    lines.push(indent_line(&format!("  {}", desc), Color::DarkGray));
    lines.push(Line::from(""));
}

// ---------------------------------------------------------------------------
// Advanced tab
// ---------------------------------------------------------------------------

fn build_advanced_lines(screen: &SettingsScreen) -> Vec<Line<'static>> {
    let cfg = &screen.settings_snapshot.config;
    let mut lines: Vec<Line<'static>> = Vec::new();

    lines.push(section_header("Advanced Settings"));
    lines.push(Line::from(""));

    // API key source
    let key_source = if cfg.api_key.is_some() {
        "config file (masked)"
    } else if std::env::var("ANTHROPIC_API_KEY").is_ok() {
        "environment variable (ANTHROPIC_API_KEY)"
    } else {
        "not set"
    };
    lines.push(label_value_line("API Key Source", key_source));
    if cfg.api_key.is_some() {
        lines.push(indent_line("  sk-ant-api03-***...***", Color::DarkGray));
    }
    lines.push(Line::from(""));

    // MCP Servers
    lines.push(section_header("MCP Servers"));
    lines.push(Line::from(""));
    if cfg.mcp_servers.is_empty() {
        lines.push(indent_line("  (none configured)", Color::DarkGray));
    } else {
        for srv in &cfg.mcp_servers {
            let kind = if srv.url.is_some() { "http" } else { &srv.server_type };
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {:<20}", srv.name),
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!("[{}]", kind), Style::default().fg(Color::DarkGray)),
            ]));
            if let Some(cmd) = &srv.command {
                lines.push(indent_line(&format!("  cmd: {}", cmd), Color::DarkGray));
            }
            if let Some(url) = &srv.url {
                lines.push(indent_line(&format!("  url: {}", url), Color::DarkGray));
            }
        }
    }
    lines.push(Line::from(""));

    // Hooks
    lines.push(section_header("Configured Hooks"));
    lines.push(Line::from(""));
    if cfg.hooks.is_empty() {
        lines.push(indent_line("  (none configured)", Color::DarkGray));
    } else {
        for (event, entries) in &cfg.hooks {
            for entry in entries {
                let event_name = format!("{:?}", event);
                let filter = entry
                    .tool_filter
                    .as_deref()
                    .map(|f| format!("[{}]", f))
                    .unwrap_or_default();
                let blocking = if entry.blocking { " (blocking)" } else { "" };
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("  {:<20}", event_name),
                        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(filter, Style::default().fg(Color::Cyan)),
                    Span::styled(blocking.to_string(), Style::default().fg(Color::Red)),
                ]));
                lines.push(indent_line(&format!("    cmd: {}", entry.command), Color::DarkGray));
            }
        }
    }
    lines.push(Line::from(""));

    // Environment variables
    lines.push(section_header("Environment Variables"));
    lines.push(Line::from(""));
    if cfg.env.is_empty() {
        lines.push(indent_line("  (none configured)", Color::DarkGray));
    } else {
        for (key, _val) in &cfg.env {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {:<25}", key),
                    Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
                ),
                Span::styled("= ***".to_string(), Style::default().fg(Color::DarkGray)),
            ]));
        }
    }
    lines.push(Line::from(""));

    lines
}

// ---------------------------------------------------------------------------
// KeyBindings tab
// ---------------------------------------------------------------------------

fn build_keybindings_lines(_screen: &SettingsScreen) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();

    lines.push(section_header("Key Bindings"));
    lines.push(Line::from(""));
    lines.push(Line::from(vec![Span::styled(
        "  Edit ~/.claude/keybindings.json to customise bindings.",
        Style::default().fg(Color::Yellow).add_modifier(Modifier::ITALIC),
    )]));
    lines.push(Line::from(""));

    // Group bindings by context
    let mut by_context: std::collections::HashMap<String, Vec<(String, String)>> =
        std::collections::HashMap::new();

    for binding in default_bindings() {
        if let Some(action) = &binding.action {
            let ctx_name = format!("{:?}", binding.context);
            let chord_str = binding
                .chord
                .iter()
                .map(|ks| {
                    let mut parts = Vec::new();
                    if ks.ctrl { parts.push("Ctrl"); }
                    if ks.alt { parts.push("Alt"); }
                    if ks.shift { parts.push("Shift"); }
                    parts.push(ks.key.as_str());
                    parts.join("+")
                })
                .collect::<Vec<_>>()
                .join(" ");
            by_context
                .entry(ctx_name)
                .or_default()
                .push((chord_str, action.clone()));
        }
    }

    // Render in sorted context order
    let mut contexts: Vec<String> = by_context.keys().cloned().collect();
    contexts.sort();

    // Ensure Global and Chat come first
    contexts.retain(|c| c != "Global" && c != "Chat");
    let mut ordered = vec!["Global".to_string(), "Chat".to_string()];
    ordered.extend(contexts);

    for ctx in &ordered {
        if let Some(entries) = by_context.get(ctx) {
            lines.push(Line::from(vec![Span::styled(
                format!("  {} Context", ctx),
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            )]));
            lines.push(Line::from(""));
            for (chord, action) in entries {
                lines.push(Line::from(vec![
                    Span::raw("    "),
                    Span::styled(
                        format!("{:<25}", chord),
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(action.clone(), Style::default().fg(Color::White)),
                ]));
            }
            lines.push(Line::from(""));
        }
    }

    lines
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn section_header(title: &str) -> Line<'static> {
    Line::from(vec![Span::styled(
        format!("  {}", title),
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
    )])
}

fn label_value_line(label: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("  {:<25}", label),
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        ),
        Span::styled(value.to_string(), Style::default().fg(Color::Cyan)),
    ])
}

fn indent_line(text: &str, color: Color) -> Line<'static> {
    Line::from(vec![Span::styled(
        text.to_string(),
        Style::default().fg(color),
    )])
}

/// Build display lines for an editable field.
fn field_lines(
    field_key: &str,
    label: &str,
    current_value: &str,
    description: &str,
    screen: &SettingsScreen,
) -> Vec<Line<'static>> {
    let is_editing = screen.edit_field.as_deref() == Some(field_key);
    let has_pending = screen.pending_changes.contains_key(field_key);

    let display_value = if is_editing {
        format!("{}_", screen.edit_value)
    } else if let Some(pending) = screen.pending_changes.get(field_key) {
        format!("{} (unsaved)", pending)
    } else {
        current_value.to_string()
    };

    let value_color = if is_editing {
        Color::Yellow
    } else if has_pending {
        Color::Magenta
    } else {
        Color::Cyan
    };

    let edit_hint = if is_editing {
        " [editing]"
    } else {
        " [Enter to edit]"
    };

    vec![
        Line::from(vec![
            Span::styled(
                format!("  {:<25}", label),
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
            ),
            Span::styled(display_value, Style::default().fg(value_color)),
            Span::styled(
                edit_hint.to_string(),
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::ITALIC),
            ),
        ]),
        Line::from(vec![Span::styled(
            format!("    {}", description),
            Style::default().fg(Color::DarkGray),
        )]),
    ]
}

// ---------------------------------------------------------------------------
// Key handling helpers (called from app.rs)
// ---------------------------------------------------------------------------

/// Returns `true` if the key event was consumed by the settings screen.
pub fn handle_settings_key(
    screen: &mut SettingsScreen,
    config: &mut Config,
    key: crossterm::event::KeyEvent,
) -> bool {
    use crossterm::event::{KeyCode, KeyModifiers};

    if !screen.visible {
        return false;
    }

    // Editing mode
    if screen.edit_field.is_some() {
        match key.code {
            KeyCode::Enter => {
                screen.commit_edit();
                screen.apply_and_save(config);
            }
            KeyCode::Esc => {
                screen.cancel_edit();
            }
            KeyCode::Backspace => {
                screen.edit_value.pop();
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                screen.edit_value.push(c);
            }
            _ => {}
        }
        return true;
    }

    // Navigation mode
    match key.code {
        KeyCode::Esc => {
            screen.close();
        }
        KeyCode::Tab => {
            screen.next_tab();
        }
        KeyCode::BackTab => {
            screen.prev_tab();
        }
        KeyCode::Up => {
            screen.scroll_up();
        }
        KeyCode::Down => {
            screen.scroll_down();
        }
        KeyCode::PageUp => {
            for _ in 0..10 {
                screen.scroll_up();
            }
        }
        KeyCode::PageDown => {
            for _ in 0..10 {
                screen.scroll_down();
            }
        }
        KeyCode::Enter => {
            // Start editing the first editable field for the current tab
            match &screen.active_tab {
                SettingsTab::General => {
                    let cfg = &screen.settings_snapshot.config;
                    let model_val = cfg.model.clone().unwrap_or_else(|| {
                        cc_core::constants::DEFAULT_MODEL.to_string()
                    });
                    screen.start_edit("model", &model_val);
                }
                _ => {}
            }
        }
        _ => {}
    }
    true
}
