use cmr::Format;
use ratatui::crossterm::event;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

const MARKDOWN: &str = r"## this is a header

this is a paragraph with **bold** and *italic* text in it

- this is a list
- with another item";

fn line(row: &cmr::Row) -> Line<'static> {
    let spans = row.pieces.iter().map(|(text, style)| {
        let mut out = Style::new();
        if style.bold {
            out = out.add_modifier(Modifier::BOLD);
        }
        if style.italic {
            out = out.add_modifier(Modifier::ITALIC);
        }
        Span::styled(text.clone(), out)
    });
    Line::from_iter(spans)
}

fn main() -> std::io::Result<()> {
    let page = cmr::parse(MARKDOWN).render_with(Format::builder().width(30).build());
    let lines: Vec<Line> = page.rows().iter().map(line).collect();

    let mut terminal = ratatui::init();
    terminal.draw(|frame| {
        let area = Rect::new(0, 0, 34, lines.len() as u16 + 2);
        let block = Block::bordered().title(" cmr in ratatui ");
        frame.render_widget(Paragraph::new(lines.clone()).block(block), area);
    })?;
    event::read()?;
    ratatui::restore();
    Ok(())
}
