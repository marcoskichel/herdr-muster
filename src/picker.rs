use crate::model::{AgentState, Kind, Row};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::{execute, terminal};
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config as NucleoConfig, Matcher, Utf32Str};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use std::io::stdout;

pub enum Outcome {
    Cancel,
    Jump(usize),
    ForceNew(usize),
    Close(usize),
}

fn state_color(s: AgentState) -> Color {
    match s {
        AgentState::Blocked => Color::Red,
        AgentState::Working => Color::Cyan,
        AgentState::Done => Color::Green,
        AgentState::Idle => Color::DarkGray,
        AgentState::Unknown => Color::DarkGray,
    }
}

/// Returns original row indices, ranked. Empty query keeps assembled order.
fn filter(rows: &[Row], query: &str, matcher: &mut Matcher) -> Vec<usize> {
    if query.is_empty() {
        return (0..rows.len()).collect();
    }
    let pat = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);
    let mut buf = Vec::new();
    let mut scored: Vec<(u32, usize)> = rows
        .iter()
        .enumerate()
        .filter_map(|(i, r)| {
            let hay_str = format!("{} {}", r.name, r.display);
            let hay = Utf32Str::new(&hay_str, &mut buf);
            pat.score(hay, matcher).map(|s| (s, i))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored.into_iter().map(|(_, i)| i).collect()
}

fn row_line(r: &Row) -> Line<'static> {
    match &r.kind {
        Kind::Open { state, agent, .. } => {
            let color = state_color(*state);
            let meta = match agent {
                Some(a) => format!("{a} · {}", state.word()),
                None => state.word().to_string(),
            };
            Line::from(vec![
                Span::styled(format!("{} ", state.glyph()), Style::default().fg(color)),
                Span::styled(format!("{:<18} ", r.name), Style::default().add_modifier(Modifier::BOLD)),
                Span::styled(format!("{:<28} ", r.display), Style::default().fg(Color::DarkGray)),
                Span::styled(meta, Style::default().fg(color)),
            ])
        }
        Kind::Dormant => Line::from(vec![
            Span::raw("  "),
            Span::styled(format!("{:<18} ", r.name), Style::default().fg(Color::Gray)),
            Span::styled(r.display.clone(), Style::default().fg(Color::DarkGray)),
        ]),
    }
}

fn header(text: &str) -> ListItem<'static> {
    ListItem::new(Line::from(Span::styled(
        text.to_string(),
        Style::default().fg(Color::Yellow).add_modifier(Modifier::DIM),
    )))
}

/// Build list items (with headers when browsing) and the list position of the
/// currently-selected filtered row.
fn build(rows: &[Row], filtered: &[usize], sel: usize, show_headers: bool) -> (Vec<ListItem<'static>>, usize) {
    let mut items = Vec::new();
    let mut sel_pos = 0;
    let mut last_group: Option<u8> = None;
    for (fi, &ri) in filtered.iter().enumerate() {
        let r = &rows[ri];
        let group = match r.kind { Kind::Open { .. } => 0u8, Kind::Dormant => 1u8 };
        if show_headers && last_group != Some(group) {
            items.push(header(if group == 0 { "  OPEN" } else { "  PROJECTS" }));
            last_group = Some(group);
        }
        if fi == sel {
            sel_pos = items.len();
        }
        items.push(ListItem::new(row_line(r)));
    }
    (items, sel_pos)
}

pub fn run(rows: &[Row]) -> std::io::Result<Outcome> {
    terminal::enable_raw_mode()?;
    execute!(stdout(), terminal::EnterAlternateScreen)?;
    let mut term = Terminal::new(CrosstermBackend::new(stdout()))?;
    let mut matcher = Matcher::new(NucleoConfig::DEFAULT);
    let mut query = String::new();
    let mut sel: usize = 0;
    let mut outcome = Outcome::Cancel;

    loop {
        let filtered = filter(rows, &query, &mut matcher);
        if sel >= filtered.len() {
            sel = filtered.len().saturating_sub(1);
        }
        let (items, sel_pos) = build(rows, &filtered, sel, query.is_empty());

        term.draw(|f| {
            let v = Layout::vertical([Constraint::Length(3), Constraint::Min(1), Constraint::Length(2)])
                .split(f.area());
            let prompt = Paragraph::new(format!("muster> {query}"))
                .block(Block::default().borders(Borders::ALL).title("pick project"));
            f.render_widget(prompt, v[0]);

            let mut st = ListState::default();
            if !filtered.is_empty() {
                st.select(Some(sel_pos));
            }
            let list = List::new(items)
                .highlight_style(Style::default().bg(Color::Rgb(60, 50, 30)).add_modifier(Modifier::BOLD))
                .highlight_symbol("▌");
            f.render_stateful_widget(list, v[1], &mut st);

            let help = Paragraph::new("↵ jump   ^n new   ^x close   esc cancel")
                .style(Style::default().fg(Color::DarkGray));
            f.render_widget(help, v[2]);
        })?;

        if let Event::Key(k) = event::read()? {
            if k.kind != KeyEventKind::Press {
                continue;
            }
            let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
            match k.code {
                KeyCode::Esc => break,
                KeyCode::Char('c') if ctrl => break,
                KeyCode::Enter => {
                    if let Some(&ri) = filtered.get(sel) {
                        outcome = Outcome::Jump(ri);
                        break;
                    }
                }
                KeyCode::Char('n') if ctrl => {
                    if let Some(&ri) = filtered.get(sel) {
                        outcome = Outcome::ForceNew(ri);
                        break;
                    }
                }
                KeyCode::Char('x') if ctrl => {
                    if let Some(&ri) = filtered.get(sel) {
                        if matches!(rows[ri].kind, Kind::Open { .. }) {
                            outcome = Outcome::Close(ri);
                            break;
                        }
                    }
                }
                KeyCode::Up => sel = sel.saturating_sub(1),
                KeyCode::Down => {
                    if sel + 1 < filtered.len() {
                        sel += 1;
                    }
                }
                KeyCode::Backspace => {
                    query.pop();
                    sel = 0;
                }
                KeyCode::Char(c) if !ctrl => {
                    query.push(c);
                    sel = 0;
                }
                _ => {}
            }
        }
    }

    execute!(stdout(), terminal::LeaveAlternateScreen)?;
    terminal::disable_raw_mode()?;
    Ok(outcome)
}
