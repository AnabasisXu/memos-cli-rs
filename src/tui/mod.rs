//! yazi 风格 memos TUI（对标 memos_tui.py）

use crate::api::{edit_in_editor, Client, Memo};
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};
use ratatui::Frame;
use std::collections::HashSet;
use std::time::Duration;

const PAGE: usize = 20;

struct Row {
    uid: String,
    content: String,
}

struct App {
    client: Client,
    all: Vec<Row>,
    rows: Vec<Row>,
    selected: HashSet<String>,
    cursor: usize,
    search: String,
    msg: String,
    mode: Mode,
    confirm_uids: Vec<String>,
    search_buf: String,
}

enum Mode {
    Normal,
    Search,
    Confirm,
}

impl App {
    fn new(client: Client) -> Self {
        Self {
            client,
            all: Vec::new(),
            rows: Vec::new(),
            selected: HashSet::new(),
            cursor: 0,
            search: String::new(),
            msg: String::new(),
            mode: Mode::Normal,
            confirm_uids: Vec::new(),
            search_buf: String::new(),
        }
    }

    fn refresh(&mut self) -> Result<()> {
        let raw = self.client.list_all()?;
        self.all = raw
            .into_iter()
            .map(|m: Memo| Row {
                uid: m.uid().to_string(),
                content: m.content,
            })
            .collect();
        let alive: HashSet<_> = self.all.iter().map(|r| r.uid.clone()).collect();
        self.selected.retain(|u| alive.contains(u));
        self.apply_filter();
        self.msg = format!("{} 条 memo", self.all.len());
        Ok(())
    }

    fn apply_filter(&mut self) {
        let s = self.search.trim().to_lowercase();
        self.rows = self
            .all
            .iter()
            .filter(|r| s.is_empty() || r.content.to_lowercase().contains(&s))
            .map(|r| Row {
                uid: r.uid.clone(),
                content: r.content.clone(),
            })
            .collect();
        self.clamp();
    }

    fn clamp(&mut self) {
        if self.rows.is_empty() {
            self.cursor = 0;
        } else if self.cursor >= self.rows.len() {
            self.cursor = self.rows.len() - 1;
        }
    }

    fn page_count(&self) -> usize {
        max1((self.rows.len() + PAGE - 1) / PAGE)
    }

    fn page_index(&self) -> usize {
        self.cursor / PAGE
    }

    fn do_delete(&mut self, uids: &[String]) {
        let mut ok = 0;
        for u in uids {
            match self.client.delete(u) {
                Ok(()) => ok += 1,
                Err(e) => {
                    self.msg = format!("删除失败: {e}");
                    break;
                }
            }
        }
        for u in uids {
            self.selected.remove(u);
        }
        let _ = self.refresh();
        self.msg = format!("已删除 {ok} 条");
    }

    fn do_edit(&mut self, uid: &str) {
        let Some(r) = self.all.iter().find(|x| x.uid == uid) else {
            self.msg = format!("条目不存在: {uid}");
            return;
        };
        let old = r.content.clone();
        match edit_in_editor(&old) {
            Ok(Some(new)) => match self.client.patch(uid, &new) {
                Ok(()) => {
                    let _ = self.refresh();
                    self.msg = format!("已更新 {uid}");
                }
                Err(e) => self.msg = format!("更新失败: {e}"),
            },
            Ok(None) => self.msg = format!("未修改 {uid}"),
            Err(e) => self.msg = format!("编辑失败: {e}"),
        }
    }
}

fn max1(n: usize) -> usize {
    if n == 0 {
        1
    } else {
        n
    }
}

pub fn run(client: &Client) -> Result<()> {
    let mut app = App::new(client.clone());
    app.refresh()?;

    let mut terminal = ratatui::init();
    let res = loop_ui(&mut terminal, &mut app);
    ratatui::restore();
    res
}

fn loop_ui(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|f| draw(f, app))?;
        if !event::poll(Duration::from_millis(250))? {
            continue;
        }
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != event::KeyEventKind::Press {
            continue;
        }

        match app.mode {
            Mode::Search => {
                match key.code {
                    KeyCode::Enter => {
                        app.search = app.search_buf.clone();
                        app.apply_filter();
                        app.msg = format!(
                            "搜索: /{}",
                            if app.search.is_empty() {
                                "(无)"
                            } else {
                                &app.search
                            }
                        );
                        app.mode = Mode::Normal;
                    }
                    KeyCode::Esc => {
                        app.search_buf.clear();
                        app.search.clear();
                        app.apply_filter();
                        app.mode = Mode::Normal;
                    }
                    KeyCode::Backspace => {
                        app.search_buf.pop();
                    }
                    KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                        app.search_buf.push(c);
                    }
                    _ => {}
                }
            }
            Mode::Confirm => match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    let uids = std::mem::take(&mut app.confirm_uids);
                    app.mode = Mode::Normal;
                    app.do_delete(&uids);
                }
                KeyCode::Char('n')
                | KeyCode::Char('N')
                | KeyCode::Esc
                | KeyCode::Enter => {
                    app.confirm_uids.clear();
                    app.mode = Mode::Normal;
                    app.msg = "已取消".into();
                }
                _ => {}
            },
            Mode::Normal => {
                if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c')
                {
                    break;
                }
                match key.code {
                    KeyCode::Char('q') | KeyCode::Char('Q') => break,
                    KeyCode::Char('j') | KeyCode::Down => {
                        if app.cursor + 1 < app.rows.len() {
                            app.cursor += 1;
                        }
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        app.cursor = app.cursor.saturating_sub(1);
                    }
                    KeyCode::Char('g') => app.cursor = 0,
                    KeyCode::Char('G') => {
                        app.cursor = app.rows.len().saturating_sub(1);
                    }
                    KeyCode::Char('l') | KeyCode::Right | KeyCode::PageDown => {
                        if !app.rows.is_empty() {
                            app.cursor = (app.cursor + PAGE).min(app.rows.len() - 1);
                        }
                    }
                    KeyCode::Char('h') | KeyCode::Left | KeyCode::PageUp => {
                        app.cursor = app.cursor.saturating_sub(PAGE);
                    }
                    KeyCode::Char(' ') => {
                        if let Some(r) = app.rows.get(app.cursor) {
                            if !app.selected.remove(&r.uid) {
                                app.selected.insert(r.uid.clone());
                            }
                        }
                    }
                    KeyCode::Char('d') => {
                        let uids: Vec<_> = app
                            .rows
                            .iter()
                            .filter(|r| app.selected.contains(&r.uid))
                            .map(|r| r.uid.clone())
                            .collect();
                        if uids.is_empty() {
                            app.msg = "未选中条目".into();
                        } else {
                            app.confirm_uids = uids;
                            app.mode = Mode::Confirm;
                        }
                    }
                    KeyCode::Char('D') => {
                        if let Some(r) = app.rows.get(app.cursor) {
                            app.confirm_uids = vec![r.uid.clone()];
                            app.mode = Mode::Confirm;
                        } else {
                            app.msg = "列表为空".into();
                        }
                    }
                    KeyCode::Enter => {
                        if let Some(r) = app.rows.get(app.cursor) {
                            let uid = r.uid.clone();
                            // 退出 raw 模式再开编辑器
                            ratatui::restore();
                            app.do_edit(&uid);
                            *terminal = ratatui::init();
                            terminal.clear()?;
                        } else {
                            app.msg = "列表为空".into();
                        }
                    }
                    KeyCode::Char('/') => {
                        app.search_buf = app.search.clone();
                        app.mode = Mode::Search;
                    }
                    KeyCode::Esc => {
                        if !app.search.is_empty() {
                            app.search.clear();
                            app.apply_filter();
                        }
                    }
                    KeyCode::Char('r') | KeyCode::Char('R') => {
                        if let Err(e) = app.refresh() {
                            app.msg = format!("刷新失败: {e}");
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

fn draw(f: &mut Frame, app: &App) {
    let area = f.area();
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(1),
    ])
    .areas(area);

    let tag = if app.search.is_empty() {
        String::new()
    } else {
        format!(" /{}/", app.search)
    };
    let title = format!(
        " memos-tui{tag}  第{}/{}页  共{}条  {}",
        app.page_index() + 1,
        app.page_count(),
        app.rows.len(),
        app.msg
    );
    f.render_widget(Paragraph::new(title).cyan(), header);

    let base = app.page_index() * PAGE;
    let visible = &app.rows[base..app.rows.len().min(base + PAGE)];
    let items: Vec<ListItem> = visible
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let idx = base + i;
            let mark = if app.selected.contains(&r.uid) {
                "*"
            } else {
                " "
            };
            let flat = r.content.replace('\n', " ");
            let line = format!(" {mark} {:<5} {flat}", idx + 1);
            let mut item = ListItem::new(line);
            if idx == app.cursor {
                item = item.style(Style::default().black().on_cyan().add_modifier(Modifier::BOLD));
            }
            item
        })
        .collect();
    f.render_widget(List::new(items), body);

    let status = if matches!(app.mode, Mode::Search) {
        format!(" 搜索: /{}", app.search_buf)
    } else {
        format!(
            " space选中[{}] d删选中 D删当前 Enter编辑 /搜索 g/G首末 h/l翻页 r刷新 q退出",
            app.selected.len()
        )
    };
    let st = if app.selected.is_empty() {
        Paragraph::new(status).cyan()
    } else {
        Paragraph::new(status).red()
    };
    f.render_widget(st, footer);

    if matches!(app.mode, Mode::Confirm) {
        let n = app.confirm_uids.len();
        let msg = format!(" 删除 {n} 条 memo ？(y/N) ");
        let popup = Paragraph::new(Line::from(Span::styled(
            msg,
            Style::default().add_modifier(Modifier::REVERSED),
        )))
        .block(Block::default().borders(Borders::ALL).title("确认"));
        let pop = centered(area, 50, 3);
        f.render_widget(Clear, pop);
        f.render_widget(popup, pop);
    }
}

fn centered(area: ratatui::layout::Rect, pct_x: u16, height: u16) -> ratatui::layout::Rect {
    let [_, mid, _] = Layout::vertical([
        Constraint::Percentage((100 - height.min(100)) / 2),
        Constraint::Length(height),
        Constraint::Percentage((100 - height.min(100)) / 2),
    ])
    .areas(area);
    let [_, boxx, _] = Layout::horizontal([
        Constraint::Percentage((100 - pct_x) / 2),
        Constraint::Percentage(pct_x),
        Constraint::Percentage((100 - pct_x) / 2),
    ])
    .areas(mid);
    boxx
}
