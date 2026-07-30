use crate::wayland::{
    ClipboardEntry, count_entries, delete_entry, list_entries, promote_entry, select_entry,
};
use crossterm::event::{self, KeyCode};
use ratatui::layout::{Constraint, Layout};
use ratatui::style::palette::tailwind;
use ratatui::{
    DefaultTerminal, Frame,
    layout::{Margin, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::Text,
};

use ratatui::widgets::{
    Block, BorderType, Cell, HighlightSpacing, Paragraph, Row, Scrollbar, ScrollbarOrientation,
    ScrollbarState, Table, TableState, Wrap,
};
use rusqlite::Connection;

struct ClipboardColors {
    buffer_bg: Color,
    header_bg: Color,
    header_fg: Color,
    row_fg: Color,
    selected_column_style_fg: Color,
    selected_cell_style_fg: Color,
    normal_row_color: Color,
}

impl ClipboardColors {
    const fn new(color: &tailwind::Palette) -> Self {
        Self {
            buffer_bg: tailwind::SLATE.c950,
            header_bg: color.c900,
            header_fg: tailwind::SLATE.c200,
            row_fg: tailwind::SLATE.c200,
            selected_column_style_fg: color.c400,
            selected_cell_style_fg: color.c400,
            normal_row_color: tailwind::SLATE.c950,
        }
    }
}

const ITEM_HEIGHT: usize = 3;
const MAX_ITEM_PREVIEW_LEN: usize = 50;

pub struct App<'a> {
    connection: &'a Connection,
    items: Vec<ClipboardEntry>,
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
        let clipboard_data = list_entries(&conn, clipboard_len);

        Self {
            connection: conn,
            items: clipboard_data.expect("clipboard data"),
            state: TableState::default().with_selected(0),
            scroll_state: ScrollbarState::new(clipboard_len as usize * ITEM_HEIGHT),
            preview_state: false,
            preview_scroll: 0,
            preview_max_scroll: 0,
            colors: ClipboardColors::new(&tailwind::BLUE),
        }
    }

    pub const fn next_row(&mut self) {
        if self.items.len() == 0 {
            return;
        }

        let i = match self.state.selected() {
            Some(i) => {
                if i >= self.items.len() - 1 {
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
        if self.items.len() == 0 {
            return;
        }

        let i = match self.state.selected() {
            Some(i) => {
                if i == 0 {
                    self.items.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
        self.scroll_state = self.scroll_state.position(i * ITEM_HEIGHT);
    }

    pub fn delete_row(&mut self) -> anyhow::Result<()> {
        let idx = self.state.selected().expect("item index");
        let id = self.items[idx].id;

        self.items.remove(idx);
        delete_entry(self.connection, id)?;

        Ok(())
    }

    pub fn save_row_to_clipboard(&mut self) -> anyhow::Result<()> {
        let idx = self.state.selected().expect("item index");

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

    pub fn run(mut self, terminal: &mut DefaultTerminal) -> anyhow::Result<()> {
        loop {
            if self.preview_state {
                terminal.draw(|frame| self.render_item_preview(frame, frame.area()))?;

                if let Some(key) = event::read()?.as_key_press_event() {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                        KeyCode::Char('j') | KeyCode::Down => self.preview_scroll_down(),
                        KeyCode::Char('k') | KeyCode::Up => self.preview_scroll_up(),
                        KeyCode::Enter => {
                            self.save_row_to_clipboard()?;
                            return Ok(());
                        }
                        KeyCode::Char(' ') => {
                            self.preview_state = !self.preview_state;
                        }
                        _ => {}
                    }
                }
            } else {
                terminal.draw(|frame| self.render(frame))?;

                if let Some(key) = event::read()?.as_key_press_event() {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                        KeyCode::Char('j') | KeyCode::Down => self.next_row(),
                        KeyCode::Char('k') | KeyCode::Up => self.previous_row(),
                        KeyCode::Enter => {
                            self.save_row_to_clipboard()?;
                            return Ok(());
                        }
                        KeyCode::Backspace => self.delete_row()?,
                        KeyCode::Char(' ') => {
                            self.preview_state = !self.preview_state;
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    fn render(&mut self, frame: &mut Frame) {
        let layout = Layout::vertical([Constraint::Min(5), Constraint::Length(4)]);
        let rects = frame.area().layout_vec(&layout);

        self.render_table(frame, rects[0]);
        self.render_scrollbar(frame, rects[0]);
    }

    fn render_table(&mut self, frame: &mut Frame, area: Rect) {
        let header_style = Style::default()
            .fg(self.colors.header_fg)
            .bg(self.colors.header_bg);

        let selected_row_style = Style::default().bg(self.colors.selected_cell_style_fg);

        let selected_col_style = Style::default().fg(self.colors.selected_column_style_fg);

        let selected_cell_style = Style::default()
            .add_modifier(Modifier::REVERSED)
            .fg(self.colors.selected_cell_style_fg);

        let header = ["Clipboard History"]
            .into_iter()
            .map(Cell::from)
            .collect::<Row>()
            .style(header_style)
            .height(1);

        let rows = self.items.iter().map(|entry| {
            let mut entry_preview: String = entry
                .data
                .chars()
                .take(MAX_ITEM_PREVIEW_LEN)
                .collect::<String>()
                .replace('\n', "\\n");

            if entry.data.len() >= MAX_ITEM_PREVIEW_LEN {
                entry_preview.replace_range(47..50, "...");
            }

            let content = format!("{}\n{}", entry_preview, entry.timestamp);

            Row::new([Cell::from(Text::from(content))])
                .style(
                    Style::new()
                        .fg(self.colors.row_fg)
                        .bg(self.colors.normal_row_color),
                )
                .height(ITEM_HEIGHT as u16)
        });

        let table = Table::new(rows, [50])
            .header(header)
            .row_highlight_style(selected_row_style)
            .column_highlight_style(selected_col_style)
            .cell_highlight_style(selected_cell_style)
            .bg(self.colors.buffer_bg)
            .highlight_spacing(HighlightSpacing::Always);

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
        let idx = self.state.selected().expect("get item index");
        let item = self.items[idx].data.as_str();
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(self.colors.selected_cell_style_fg)
            .title("item preview");
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
