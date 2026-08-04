use crate::wayland::{
    ClipboardEntry, count_entries, delete_entry, list_entries, promote_entry, select_entry,
};
use crossterm::event::{self, KeyCode, KeyModifiers};
use nucleo_matcher::{
    Config, Matcher,
    pattern::{AtomKind, CaseMatching, Normalization, Pattern},
};
use ratatui::widgets::{
    Block, BorderType, Cell, HighlightSpacing, Paragraph, Row, Scrollbar, ScrollbarOrientation,
    ScrollbarState, Table, TableState, Wrap,
};
use ratatui::{
    DefaultTerminal, Frame,
    layout::{Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
};
use ratatui::{
    layout::{Constraint, Layout},
    style::palette::tailwind,
};
use rusqlite::Connection;
use tui_input::Input;
use tui_input::backend::crossterm::EventHandler;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

struct ClipboardColors {
    header_bg: Color,
    header_fg: Color,
    row_fg: Color,
    selected_column_style_fg: Color,
    selected_cell_style_fg: Color,
}

impl ClipboardColors {
    const fn new(color: &tailwind::Palette) -> Self {
        Self {
            header_bg: color.c900,
            header_fg: tailwind::SLATE.c200,
            row_fg: tailwind::SLATE.c200,
            selected_column_style_fg: color.c400,
            selected_cell_style_fg: color.c400,
        }
    }
}

const ITEM_HEIGHT: usize = 3;
const MAX_ITEM_PREVIEW_LEN: usize = 50;

#[derive(Debug, Clone, Copy)]
struct SearchCandidate<'a> {
    index: usize,
    text: &'a str,
}

impl AsRef<str> for SearchCandidate<'_> {
    fn as_ref(&self) -> &str {
        self.text
    }
}

#[derive(Default, Clone, Copy, PartialEq)]
enum InputMode {
    #[default]
    Normal,
    Editing,
}

pub struct App<'a> {
    connection: &'a Connection,
    items: Vec<ClipboardEntry>,
    matches: Vec<usize>,
    matcher: Matcher,

    input: Input,
    input_mode: InputMode,

    state: TableState,
    scroll_state: ScrollbarState,
    preview_state: bool,
    preview_scroll: u16,
    preview_max_scroll: u16,
    colors: ClipboardColors,
}

impl<'a> App<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        let clipboard_len = count_entries(conn).expect("number of clipboard entries");
        let clipboard_data = list_entries(conn, clipboard_len);
        let items = clipboard_data.expect("clipboard data");
        let matches = (0..items.len()).collect();

        let mut matcher_config = Config::DEFAULT;
        matcher_config.prefer_prefix = true;

        Self {
            connection: conn,
            items,
            matches,
            matcher: Matcher::new(matcher_config),

            input: Input::new(String::new()),
            input_mode: InputMode::default(),

            state: TableState::default().with_selected(0),
            scroll_state: ScrollbarState::new(clipboard_len as usize * ITEM_HEIGHT),
            preview_state: false,
            preview_scroll: 0,
            preview_max_scroll: 0,
            colors: ClipboardColors::new(&tailwind::BLUE),
        }
    }

    pub const fn next_row(&mut self) {
        if self.matches.is_empty() {
            return;
        }

        let i = match self.state.selected() {
            Some(i) => {
                if i >= self.matches.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
        self.scroll_state = self.scroll_state.position(i * ITEM_HEIGHT);
    }

    pub const fn previous_row(&mut self) {
        if self.matches.is_empty() {
            return;
        }

        let i = match self.state.selected() {
            Some(i) => {
                if i == 0 {
                    self.matches.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
        self.scroll_state = self.scroll_state.position(i * ITEM_HEIGHT);
    }

    fn selected_item_index(&self) -> Option<usize> {
        let visible_index = self.state.selected()?;
        self.matches.get(visible_index).copied()
    }

    pub fn delete_row(&mut self) -> anyhow::Result<()> {
        if self.matches.is_empty() {
            return Ok(());
        }

        let selected_row = self.state.selected().unwrap_or_default();
        let Some(idx) = self.selected_item_index() else {
            return Ok(());
        };

        let id = self.items[idx].id;

        delete_entry(self.connection, id)?;
        self.items.remove(idx);
        self.matches.retain(|&x| x != idx);
        for matched_idx in &mut self.matches {
            if *matched_idx > idx {
                *matched_idx -= 1;
            }
        }

        let selected_row =
            (!self.matches.is_empty()).then(|| selected_row.min(self.matches.len() - 1));
        self.state.select(selected_row);
        self.scroll_state = ScrollbarState::new(self.matches.len() * ITEM_HEIGHT)
            .position(selected_row.unwrap_or_default() * ITEM_HEIGHT);

        Ok(())
    }

    pub fn save_row_to_clipboard(&mut self) -> anyhow::Result<()> {
        if self.matches.is_empty() {
            return Ok(());
        }

        let Some(idx) = self.selected_item_index() else {
            return Ok(());
        };

        let id = self.items[idx].id;
        select_entry(self.connection, id)?;
        promote_entry(self.connection, id)?;

        let item = self.items[idx].clone();
        self.items.remove(idx);
        self.items.insert(0, item);

        Ok(())
    }

    pub fn preview_scroll_down(&mut self) {
        if self.preview_scroll < self.preview_max_scroll {
            self.preview_scroll = self.preview_scroll.saturating_add(1);
        }
    }

    pub fn preview_scroll_up(&mut self) {
        self.preview_scroll = self.preview_scroll.saturating_sub(1);
    }

    fn open_preview(&mut self) {
        if self.selected_item_index().is_some() {
            self.preview_state = true;
            self.preview_scroll = 0;
        }
    }

    pub fn search_item(&mut self, query: &str) {
        if query.trim().is_empty() {
            self.matches = (0..self.items.len()).collect();
            return;
        }

        let pattern = Pattern::new(
            query,
            CaseMatching::Ignore,
            Normalization::Smart,
            AtomKind::Fuzzy,
        );

        let candidates = self
            .items
            .iter()
            .enumerate()
            .map(|(index, item)| SearchCandidate {
                index,
                text: &item.data,
            });

        let ranked_matches = pattern.match_list(candidates, &mut self.matcher);

        self.matches = ranked_matches
            .into_iter()
            .map(|(candidate, _score)| candidate.index)
            .collect();
    }

    fn start_editing(&mut self) {
        self.input_mode = InputMode::Editing
    }

    fn stop_editing(&mut self) {
        self.input_mode = InputMode::Normal
    }

    pub fn run(mut self, terminal: &mut DefaultTerminal) -> anyhow::Result<()> {
        loop {
            if self.preview_state {
                terminal.draw(|frame| self.render_item_preview(frame, frame.area()))?;

                if let Some(key) = event::read()?.as_key_press_event() {
                    if key.code == KeyCode::Char('c')
                        && key.modifiers.contains(KeyModifiers::CONTROL)
                    {
                        return Ok(());
                    }

                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                        KeyCode::Char('j') | KeyCode::Down => self.preview_scroll_down(),
                        KeyCode::Char('k') | KeyCode::Up => self.preview_scroll_up(),
                        KeyCode::Enter => {
                            self.save_row_to_clipboard()?;
                            return Ok(());
                        }
                        KeyCode::Char(' ') => {
                            self.preview_state = false;
                        }
                        _ => {}
                    }
                }
            } else {
                terminal.draw(|frame| self.render(frame))?;
                let event = event::read()?;

                if let Some(key) = event.as_key_press_event() {
                    if key.code == KeyCode::Char('c')
                        && key.modifiers.contains(KeyModifiers::CONTROL)
                    {
                        return Ok(());
                    }

                    match self.input_mode {
                        InputMode::Normal => match key.code {
                            KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                            KeyCode::Char('j') | KeyCode::Down => self.next_row(),
                            KeyCode::Char('k') | KeyCode::Up => self.previous_row(),
                            KeyCode::Enter => {
                                self.save_row_to_clipboard()?;
                                return Ok(());
                            }
                            KeyCode::Backspace => self.delete_row()?,
                            KeyCode::Char(' ') => {
                                self.open_preview();
                            }
                            KeyCode::Char('/') => self.start_editing(),
                            _ => {}
                        },

                        InputMode::Editing => match key.code {
                            KeyCode::Enter => self.stop_editing(),
                            KeyCode::Esc => {
                                self.stop_editing();
                                self.input = "".into();
                                self.state.select((!self.items.is_empty()).then_some(0));
                                self.matches = (0..self.items.len()).collect();
                                self.scroll_state =
                                    ScrollbarState::new(self.matches.len() * ITEM_HEIGHT);
                            }
                            _ => {
                                self.input.handle_event(&event);

                                let query = self.input.value().to_owned();
                                self.search_item(&query);

                                self.state.select((!self.items.is_empty()).then_some(0));
                                self.scroll_state =
                                    ScrollbarState::new(self.matches.len() * ITEM_HEIGHT)
                            }
                        },
                    }
                }
            }
        }
    }

    fn render(&mut self, frame: &mut Frame) {
        if self.input_mode == InputMode::Normal {
            let area = frame.area();

            self.render_table(frame, area);
            self.render_scrollbar(frame, area);
        } else {
            let [input_area, table_area] =
                Layout::vertical([Constraint::Length(3), Constraint::Min(0)]).areas(frame.area());

            self.render_input(frame, input_area);
            self.render_table(frame, table_area);
            self.render_scrollbar(frame, table_area);
        }
    }

    pub fn generate_preview(entry: &str) -> String {
        let entry = entry.replace('\n', "\\n");

        let width = UnicodeWidthStr::width(entry.as_str());
        if width <= MAX_ITEM_PREVIEW_LEN {
            return entry;
        }

        let mut preview = String::new();
        let avaliable_width: usize = MAX_ITEM_PREVIEW_LEN.saturating_sub(3);
        let mut used_width: usize = 0;

        for grapheme in entry.graphemes(true) {
            let width = UnicodeWidthStr::width(grapheme);

            if used_width + width > avaliable_width {
                break;
            }
            preview.push_str(grapheme);
            used_width += width;
        }

        preview.push_str("...");
        preview
    }

    fn render_input(&mut self, frame: &mut Frame, area: Rect) {
        let width = area.width.max(3) - 3;
        let scroll = self.input.visual_scroll(width as usize);

        let input = Paragraph::new(self.input.value())
            .style(Color::Green)
            .scroll((0, 0))
            .block(Block::bordered().title("search"));

        frame.render_widget(input, area);

        if self.input_mode == InputMode::Editing {
            let x = self.input.visual_cursor().max(scroll) - scroll + 1;
            frame.set_cursor_position((area.x + x as u16, area.y + 1));
        }
    }

    fn render_table(&mut self, frame: &mut Frame, area: Rect) {
        let header_style = Style::default()
            .fg(self.colors.header_fg)
            .bg(self.colors.header_bg);

        let count_text = match self.input_mode {
            InputMode::Editing => format!("{} of {} items", self.matches.len(), self.items.len()),
            InputMode::Normal => format!("{} items", self.matches.len()),
        };

        let header = Row::new([Cell::from(Text::from(vec![
            Line::from("Clipboard History"),
            Line::from(count_text),
        ]))])
        .style(header_style)
        .height(2);

        let selected_row_style = Style::default().bg(self.colors.selected_cell_style_fg);

        let selected_col_style = Style::default().fg(self.colors.selected_column_style_fg);

        let selected_cell_style = Style::default()
            .add_modifier(Modifier::REVERSED)
            .fg(self.colors.selected_cell_style_fg);

        let rows = self
            .matches
            .iter()
            .filter_map(|&index| self.items.get(index))
            .map(|entry| {
                let entry_preview = App::generate_preview(&entry.data);

                let content = Text::from(vec![
                    Line::from(entry_preview),
                    Line::from(Span::styled(
                        entry.timestamp.to_string(),
                        Style::new().fg(Color::Gray),
                    )),
                ]);

                Row::new([Cell::from(content)])
                    .style(Style::new().fg(self.colors.row_fg))
                    .height(ITEM_HEIGHT as u16)
            });

        let footer_text = match self.input_mode {
            InputMode::Normal => "↑/↓ up/down • ⤶ copy • ⌫ delete • ␣ preview",
            InputMode::Editing => "⤶ apply • esc cancel",
        };

        let footer = Row::new([footer_text]).top_margin(2);

        let table = Table::new(rows, [MAX_ITEM_PREVIEW_LEN as u16])
            .header(header)
            .row_highlight_style(selected_row_style)
            .column_highlight_style(selected_col_style)
            .cell_highlight_style(selected_cell_style)
            .highlight_spacing(HighlightSpacing::Always)
            .footer(footer);

        frame.render_stateful_widget(table, area, &mut self.state);
    }

    fn render_scrollbar(&mut self, frame: &mut Frame, area: Rect) {
        frame.render_stateful_widget(
            Scrollbar::default()
                .orientation(ScrollbarOrientation::VerticalRight)
                .begin_symbol(None)
                .end_symbol(None),
            area.inner(Margin {
                vertical: 1,
                horizontal: 1,
            }),
            &mut self.scroll_state,
        );
    }

    fn render_item_preview(&mut self, frame: &mut Frame, area: Rect) {
        let Some(idx) = self.selected_item_index() else {
            self.preview_state = false;
            self.render(frame);
            return;
        };

        let item = self.items[idx].data.as_str();

        let footer_text = "↑/↓ up/down • ␣ exit preview";
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(self.colors.selected_cell_style_fg)
            .title("item preview")
            .title_bottom(footer_text);

        let inner_area = block.inner(area);
        let text = Paragraph::new(item).wrap(Wrap { trim: true });

        let rendered_lines = text.line_count(inner_area.width);
        self.preview_max_scroll = rendered_lines
            .saturating_sub(inner_area.height as usize)
            .min(u16::MAX as usize) as u16;
        self.preview_scroll = self.preview_scroll.min(self.preview_max_scroll);

        let text = text.block(block).scroll((self.preview_scroll, 0));

        frame.render_widget(text, area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> Connection {
        let conn = Connection::open_in_memory().expect("open in-memory database");
        conn.pragma_update(None, "secure_delete", true)
            .expect("turn on secure_delete");
        conn.execute(
            "CREATE TABLE clipd (
                id INTEGER PRIMARY KEY,
                sort_order INTEGER NOT NULL DEFAULT 0,
                data TEXT NOT NULL,
                timestamp TEXT DEFAULT CURRENT_TIMESTAMP NOT NULL
            )",
            (),
        )
        .expect("create clipd table");
        conn
    }

    #[test]
    fn preview_stays_closed_when_database_is_empty() {
        let conn = test_db();
        let mut app = App::new(&conn);

        app.open_preview();

        assert!(!app.preview_state);
    }

    #[test]
    fn preview_stays_closed_when_search_has_no_matches() {
        let conn = test_db();
        conn.execute(
            "INSERT INTO clipd (sort_order, data) VALUES (1, 'clipboard entry')",
            (),
        )
        .expect("insert clipboard entry");
        let mut app = App::new(&conn);
        app.search_item("does not match");

        app.open_preview();

        assert!(!app.preview_state);
    }

    #[test]
    fn short_preview_unchanged() {
        let preview = App::generate_preview("test");
        assert_eq!(preview, "test");
    }

    #[test]
    fn preview_escapes_newlines() {
        let preview = App::generate_preview("hello\nworld");
        assert_eq!(preview, "hello\\nworld");
    }

    #[test]
    fn generate_preview_truncates_multibyte_chars() {
        let test_str = format!("{}🇺🇸", "a".repeat(MAX_ITEM_PREVIEW_LEN - 1));

        let expected = format!("{}...", "a".repeat(MAX_ITEM_PREVIEW_LEN - 3));

        let preview = App::generate_preview(&test_str);

        assert_eq!(preview, expected);
        assert_eq!(
            UnicodeWidthStr::width(preview.as_str()),
            MAX_ITEM_PREVIEW_LEN
        );
    }
}
