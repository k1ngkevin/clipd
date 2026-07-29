use crate::wayland::{ClipboardEntry, count_entries, list_entries, select_entry};
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
    Cell, HighlightSpacing, Row, Scrollbar, ScrollbarOrientation, ScrollbarState, Table, TableState,
};
use rusqlite::Connection;

struct ClipboardColors {
    buffer_bg: Color,
    header_bg: Color,
    header_fg: Color,
    row_fg: Color,
    selected_row_style_fg: Color,
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
            selected_row_style_fg: color.c400,
            selected_column_style_fg: color.c400,
            selected_cell_style_fg: color.c600,
            normal_row_color: tailwind::SLATE.c950,
        }
    }
}

impl ClipboardEntry {
    const fn ref_array(&self) -> [&String; 2] {
        [&self.data, &self.timestamp]
    }
}

const ITEM_HEIGHT: usize = 5;

pub struct App<'a> {
    connection: &'a Connection,
    items: Vec<ClipboardEntry>,
    state: TableState,
    scroll_state: ScrollbarState,
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
            colors: ClipboardColors::new(&tailwind::BLUE),
        }
    }

    pub const fn next_row(&mut self) {
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

    pub fn save_row_to_clipboard(&mut self) -> anyhow::Result<()> {
        let idx = self.state.selected().expect("item index");
        let id = self.items[idx].id;
        select_entry(self.connection, id)
    }

    pub fn run(mut self, terminal: &mut DefaultTerminal) -> anyhow::Result<()> {
        loop {
            terminal.draw(|frame| self.render(frame))?;

            if let Some(key) = event::read()?.as_key_press_event() {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                    KeyCode::Char('j') | KeyCode::Down => self.next_row(),
                    KeyCode::Char('k') | KeyCode::Up => self.previous_row(),
                    KeyCode::Enter => self.save_row_to_clipboard()?,
                    _ => {}
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

        let selected_row_style = Style::default()
            .add_modifier(Modifier::REVERSED)
            .fg(self.colors.selected_row_style_fg);

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

        let rows = self.items.iter().map(|data| {
            let color = self.colors.normal_row_color;

            let item = data.ref_array();

            item.into_iter()
                .map(|content| Cell::from(Text::from(format!("\n{content}\n"))))
                .collect::<Row>()
                .style(Style::new().fg(self.colors.row_fg).bg(color))
                .height(5)
        });

        let bar = " █ ";

        let table = Table::new(rows, [50])
            .header(header)
            .row_highlight_style(selected_row_style)
            .column_highlight_style(selected_col_style)
            .cell_highlight_style(selected_cell_style)
            .highlight_symbol(Text::from(vec![
                "".into(),
                bar.into(),
                bar.into(),
                "".into(),
            ]))
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
}
