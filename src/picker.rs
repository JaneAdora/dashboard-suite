//! Interactive checklist: toggle launchers + glance panels, then install.
use crate::apply::which;
use crate::manifest::Manifest;
use crate::theme;
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode};
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use std::time::Duration;

enum Row {
    Section(&'static str),
    Launcher(usize),
    Panel(usize),
}

pub struct Selection {
    pub launchers: Vec<usize>,
    pub panels: Vec<usize>,
}

pub fn run(m: &Manifest) -> Result<Option<Selection>> {
    let mut rows = vec![Row::Section("Launchers")];
    rows.extend((0..m.launchers.len()).map(Row::Launcher));
    rows.push(Row::Section("Glance panels"));
    rows.extend((0..m.panels.len()).map(Row::Panel));

    let mut lsel: Vec<bool> = m.launchers.iter().map(|l| l.default).collect();
    let mut psel: Vec<bool> = m.panels.iter().map(|p| p.default).collect();
    let lmiss: Vec<Vec<String>> = m
        .launchers
        .iter()
        .map(|l| l.requires.iter().filter(|r| which(r).is_none()).cloned().collect())
        .collect();

    let selectable: Vec<usize> = rows
        .iter()
        .enumerate()
        .filter(|(_, r)| !matches!(r, Row::Section(_)))
        .map(|(i, _)| i)
        .collect();
    let mut cur = 0usize;
    let mut top = 0usize;

    let _g = theme::TerminalGuard::enter()?;
    let mut term = ratatui::Terminal::new(ratatui::backend::CrosstermBackend::new(std::io::stdout()))?;

    let result = loop {
        let cursor_row = selectable[cur];
        term.draw(|f| draw(f, m, &rows, &lsel, &psel, &lmiss, cursor_row, &mut top))?;
        if !event::poll(Duration::from_millis(200))? {
            continue;
        }
        let Event::Key(k) = event::read()? else { continue };
        match k.code {
            KeyCode::Char('q') | KeyCode::Esc => break None,
            KeyCode::Char('j') | KeyCode::Down => {
                if cur + 1 < selectable.len() {
                    cur += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => cur = cur.saturating_sub(1),
            KeyCode::Char(' ') => match rows[selectable[cur]] {
                Row::Launcher(i) => lsel[i] = !lsel[i],
                Row::Panel(i) => psel[i] = !psel[i],
                Row::Section(_) => {}
            },
            KeyCode::Char('a') => {
                lsel.iter_mut().for_each(|x| *x = true);
                psel.iter_mut().for_each(|x| *x = true);
            }
            KeyCode::Char('n') => {
                lsel.iter_mut().for_each(|x| *x = false);
                psel.iter_mut().for_each(|x| *x = false);
            }
            KeyCode::Char('d') => {
                for (i, l) in m.launchers.iter().enumerate() {
                    lsel[i] = l.default;
                }
                for (i, p) in m.panels.iter().enumerate() {
                    psel[i] = p.default;
                }
            }
            KeyCode::Enter => {
                break Some(Selection {
                    launchers: (0..lsel.len()).filter(|&i| lsel[i]).collect(),
                    panels: (0..psel.len()).filter(|&i| psel[i]).collect(),
                });
            }
            _ => {}
        }
    };
    Ok(result)
}

#[allow(clippy::too_many_arguments)]
fn draw(
    f: &mut Frame,
    m: &Manifest,
    rows: &[Row],
    lsel: &[bool],
    psel: &[bool],
    lmiss: &[Vec<String>],
    cursor_row: usize,
    top: &mut usize,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(0), Constraint::Length(1)])
        .split(f.area());

    let lcount = lsel.iter().filter(|&&b| b).count();
    let pcount = psel.iter().filter(|&&b| b).count();
    f.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(" rsuite — install the dashboard suite", theme::header())),
            Line::from(Span::styled(
                format!(" {lcount} launchers + {pcount} panels selected", ),
                theme::dim(),
            )),
        ]),
        chunks[0],
    );

    let height = chunks[1].height as usize;
    if height > 0 {
        if cursor_row < *top {
            *top = cursor_row;
        } else if cursor_row >= *top + height {
            *top = cursor_row + 1 - height;
        }
    }

    let lines: Vec<Line> = rows
        .iter()
        .enumerate()
        .skip(*top)
        .take(height)
        .map(|(ri, row)| {
            let cursor = ri == cursor_row;
            match row {
                Row::Section(t) => Line::from(Span::styled(format!(" {t}"), theme::section())),
                Row::Launcher(i) => item(cursor, lsel[*i], &m.launchers[*i].name, &m.launchers[*i].summary, &lmiss[*i]),
                Row::Panel(i) => item(cursor, psel[*i], &m.panels[*i].name, &m.panels[*i].summary, &[]),
            }
        })
        .collect();
    f.render_widget(Paragraph::new(lines), chunks[1]);

    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            " space toggle · a all · n none · d defaults · enter install · q cancel",
            theme::dim(),
        ))),
        chunks[2],
    );
}

fn item(cursor: bool, checked: bool, name: &str, summary: &str, missing: &[String]) -> Line<'static> {
    let mut spans = vec![
        Span::styled(if cursor { "▸ " } else { "  " }.to_string(), theme::active()),
        Span::styled(
            if checked { "[x] " } else { "[ ] " }.to_string(),
            if checked { theme::active() } else { theme::dim() },
        ),
        Span::styled(format!("{name:<12}"), if cursor { theme::active() } else { Style::default() }),
        Span::styled(format!(" {summary}"), theme::dim()),
    ];
    if !missing.is_empty() {
        spans.push(Span::styled(format!("  needs: {}", missing.join(", ")), theme::warn()));
    }
    Line::from(spans)
}
