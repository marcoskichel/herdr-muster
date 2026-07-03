use crate::model::{AgentState, Kind, Row};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::{execute, terminal};
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config as NucleoConfig, Matcher, Utf32Str};
use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph};
use std::io::stdout;

pub enum Outcome {
    Cancel,
    Jump(usize),
    ForceNew(usize),
    Close(usize),
}

// --- tokyo-night palette ---
const AMBER: Color = Color::Rgb(0xe0, 0xaf, 0x68); // accent (prompt, selection)
const FG: Color = Color::Rgb(0xc0, 0xca, 0xf5); // primary text
const MUTED: Color = Color::Rgb(0x56, 0x5f, 0x89); // comments / dim
const FAINT: Color = Color::Rgb(0x3b, 0x42, 0x61); // dividers / placeholders
const SEL_BG: Color = Color::Rgb(0x2a, 0x27, 0x1c); // amber-tinted selection
const RED: Color = Color::Rgb(0xf7, 0x76, 0x8e);
const CYAN: Color = Color::Rgb(0x7d, 0xcf, 0xff);
const GREEN: Color = Color::Rgb(0x9e, 0xce, 0x6a);

const NAME_W: usize = 20;
const GLYPH_W: usize = 2; // glyph + space
const HL_W: usize = 2; // highlight symbol width ("▌ ")

fn state_color(s: AgentState) -> Color {
    match s {
        AgentState::Blocked => RED,
        AgentState::Working => CYAN,
        AgentState::Done => GREEN,
        AgentState::Idle => MUTED,
        AgentState::Unknown => MUTED,
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

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else if max <= 1 {
        "…".to_string()
    } else {
        let head: String = s.chars().take(max - 1).collect();
        format!("{head}…")
    }
}

/// One row, meta right-aligned to `w` columns (the list's inner content width).
fn row_line(r: &Row, w: usize) -> Line<'static> {
    match &r.kind {
        Kind::Open { state, agent, .. } => {
            let color = state_color(*state);
            let name = format!("{:<NAME_W$}", truncate(&r.name, NAME_W));
            let meta = match agent {
                Some(a) => format!("{a} · {}", state.word()),
                None => state.word().to_string(),
            };
            let meta_len = meta.chars().count();
            // columns left for path + gap + meta
            let prefix = GLYPH_W + NAME_W + 1;
            let rest = w.saturating_sub(prefix);
            let path_field = rest.saturating_sub(meta_len + 1).max(4);
            let path = truncate(&r.display, path_field);
            let pad = rest
                .saturating_sub(path.chars().count() + meta_len)
                .max(1);
            Line::from(vec![
                Span::styled(format!("{} ", state.glyph()), Style::default().fg(color)),
                Span::styled(name, Style::default().fg(FG).add_modifier(Modifier::BOLD)),
                Span::raw(" "),
                Span::styled(path, Style::default().fg(MUTED)),
                Span::raw(" ".repeat(pad)),
                Span::styled(meta, Style::default().fg(color)),
            ])
        }
        Kind::Dormant => {
            let name = format!("{:<NAME_W$}", truncate(&r.name, NAME_W));
            Line::from(vec![
                Span::raw(" ".repeat(GLYPH_W)),
                Span::styled(name, Style::default().fg(FG)),
                Span::raw(" "),
                Span::styled(r.display.clone(), Style::default().fg(FAINT)),
            ])
        }
    }
}

fn header_item(label: &str, suffix: &str) -> ListItem<'static> {
    ListItem::new(Line::from(vec![
        Span::styled(
            format!("▸ {label} "),
            Style::default().fg(MUTED).add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("— {suffix}"), Style::default().fg(FAINT)),
    ]))
}

/// Build list items (headers shown only while browsing) + the list position of
/// the selected filtered row.
fn build(
    rows: &[Row],
    filtered: &[usize],
    sel: usize,
    show_headers: bool,
    w: usize,
) -> (Vec<ListItem<'static>>, usize) {
    let mut items = Vec::new();
    let mut sel_pos = 0;
    let mut last_group: Option<u8> = None;
    for (fi, &ri) in filtered.iter().enumerate() {
        let r = &rows[ri];
        let group = match r.kind {
            Kind::Open { .. } => 0u8,
            Kind::Dormant => 1u8,
        };
        if show_headers && last_group != Some(group) {
            items.push(header_item(
                if group == 0 { "OPEN" } else { "PROJECTS" },
                if group == 0 { "LIVE WORKSPACES" } else { "NOT OPEN YET" },
            ));
            last_group = Some(group);
        }
        if fi == sel {
            sel_pos = items.len();
        }
        items.push(ListItem::new(row_line(r, w)));
    }
    (items, sel_pos)
}

/// A right-aligned pair of lines: fill `text_left` then push `text_right` to `w`.
fn spread(left: Vec<Span<'static>>, right: Vec<Span<'static>>, w: usize) -> Line<'static> {
    let lw: usize = left.iter().map(|s| s.content.chars().count()).sum();
    let rw: usize = right.iter().map(|s| s.content.chars().count()).sum();
    let pad = w.saturating_sub(lw + rw).max(1);
    let mut spans = left;
    spans.push(Span::raw(" ".repeat(pad)));
    spans.extend(right);
    Line::from(spans)
}

fn keycap(key: &str, label: &str) -> Vec<Span<'static>> {
    vec![
        Span::styled(
            format!(" {key} "),
            Style::default().fg(Color::Black).bg(MUTED),
        ),
        Span::styled(format!(" {label}   "), Style::default().fg(MUTED)),
    ]
}

pub fn run(rows: &[Row]) -> std::io::Result<Outcome> {
    terminal::enable_raw_mode()?;
    execute!(stdout(), terminal::EnterAlternateScreen)?;
    let mut term = Terminal::new(CrosstermBackend::new(stdout()))?;
    let mut matcher = Matcher::new(NucleoConfig::DEFAULT);
    let mut query = String::new();
    let mut sel: usize = 0;
    let mut outcome = Outcome::Cancel;

    let open_n = rows.iter().filter(|r| matches!(r.kind, Kind::Open { .. })).count();
    let dormant_n = rows.len() - open_n;

    loop {
        let filtered = filter(rows, &query, &mut matcher);
        if sel >= filtered.len() {
            sel = filtered.len().saturating_sub(1);
        }

        term.draw(|f| {
            let area = f.area();
            let title_left = Line::from(vec![
                Span::styled(" muster ", Style::default().fg(AMBER).add_modifier(Modifier::BOLD)),
                Span::styled("— one terminal for the whole herd ", Style::default().fg(MUTED)),
            ])
            .left_aligned();
            let title_right = Line::from(vec![
                Span::styled(format!(" {open_n} open"), Style::default().fg(GREEN)),
                Span::styled(" · ", Style::default().fg(FAINT)),
                Span::styled(format!("{dormant_n} idle "), Style::default().fg(MUTED)),
            ])
            .right_aligned();
            let block = Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(FAINT))
                .title_top(title_left)
                .title_top(title_right);
            let inner = block.inner(area);
            f.render_widget(block, area);

            let v = Layout::vertical([
                Constraint::Length(1), // prompt
                Constraint::Length(1), // divider
                Constraint::Min(1),    // list
                Constraint::Length(1), // divider
                Constraint::Length(1), // footer
            ])
            .horizontal_margin(1)
            .split(inner);

            let w = v[2].width as usize;

            // prompt row
            let query_span = if query.is_empty() {
                Span::styled("type to fuzzy-filter…", Style::default().fg(FAINT))
            } else {
                Span::styled(query.clone(), Style::default().fg(FG))
            };
            let prompt = spread(
                vec![
                    Span::styled("muster", Style::default().fg(AMBER).add_modifier(Modifier::BOLD)),
                    Span::styled("> ", Style::default().fg(AMBER)),
                    query_span,
                ],
                vec![Span::styled(
                    format!("{} matches", filtered.len()),
                    Style::default().fg(MUTED),
                )],
                w,
            );
            f.render_widget(Paragraph::new(prompt), v[0]);

            let rule = "─".repeat(w);
            let rule_style = Style::default().fg(FAINT);
            f.render_widget(Paragraph::new(rule.clone()).style(rule_style), v[1]);
            f.render_widget(Paragraph::new(rule).style(rule_style), v[3]);

            let list_w = w.saturating_sub(HL_W);
            let (items, sel_pos) = build(rows, &filtered, sel, query.is_empty(), list_w);
            let mut st = ListState::default();
            if !filtered.is_empty() {
                st.select(Some(sel_pos));
            }
            let list = List::new(items)
                .highlight_style(Style::default().bg(SEL_BG))
                .highlight_symbol("▌ ");
            f.render_stateful_widget(list, v[2], &mut st);

            // footer keycaps
            let mut footer = Vec::new();
            footer.extend(keycap("↵", "jump / create"));
            footer.extend(keycap("^n", "force new"));
            footer.extend(keycap("^x", "close"));
            footer.extend(keycap("esc", "cancel"));
            f.render_widget(Paragraph::new(Line::from(footer)), v[4]);
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
