use std::{
    cell::RefCell,
    fs::OpenOptions,
    io::{self, Stdout},
    sync::{mpsc::channel, Arc, Mutex, RwLock},
    thread::spawn,
    time::{Duration, Instant},
};

use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use log::{info, warn};
use owo_colors::OwoColorize;
use tui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Span,
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame, Terminal,
};

// use clap::Parser;
use crate::{
    interactive::Flags,
    servo,
    ui::{
        api, components::Tabs, get_config, init_config, pages::route, Interval, LoggerBuilder,
        TicksCounter, TuiOpt, TuiResult, TuiStates,
    },
};

thread_local!(pub(crate) static TICK_COUNTER: RefCell<TicksCounter> = RefCell::new(TicksCounter::new_with_time(Instant::now())));

pub type Backend = CrosstermBackend<Stdout>;

fn setup() -> TuiResult<Terminal<Backend>> {
    let mut stdout = io::stdout();

    execute!(stdout, EnterAlternateScreen)?;
    enable_raw_mode()?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    Ok(terminal)
}

fn wrap_up(mut terminal: Terminal<Backend>) -> TuiResult<()> {
    execute!(terminal.backend_mut(), LeaveAlternateScreen,)?;

    disable_raw_mode()?;

    Ok(())
}

pub fn main_loop(opt: TuiOpt, flag: Flags) -> TuiResult<()> {
    let config = flag.get_config()?;
    if config.using_server().is_none() {
        println!(
            "{} No API server configured yet. Use this command to add a server:\n\n  $ {}",
            "WARN:".red(),
            "mihomoctl server add".green()
        );
        return Ok(());
    };
    let controller_kind = config.using_server().map(|server| server.kind).unwrap_or_default();

    init_config(config);

    let state = Arc::new(RwLock::new(TuiStates::for_controller_kind(controller_kind)));
    let error = Arc::new(Mutex::new(None));

    let (event_tx, event_rx) = channel();
    let (action_tx, action_rx) = channel();

    let servo_event_tx = event_tx.clone();
    let servo = spawn(|| servo(servo_event_tx, action_rx, opt, flag));

    LoggerBuilder::new(event_tx)
        .file(get_config().tui.log_file.as_ref().map(|x| {
            OpenOptions::new()
                .append(true)
                .create(true)
                .open(x)
                .unwrap()
        }))
        .apply()?;
    info!("Logger set");

    let event_handler_state = state.clone();
    let event_handler_error = error.clone();

    let handle = spawn(move || {
        let mut should_quit;
        while let Ok(event) = event_rx.recv() {
            should_quit = event.is_quit();
            let mut state = event_handler_state.write().unwrap();
            match state.handle(event) {
                Ok(Some(action)) => {
                    if let Err(e) = action_tx.send(action) {
                        event_handler_error.lock().unwrap().replace(e.into());
                        should_quit = true;
                    }
                }
                // No action needed
                Ok(None) => {}
                Err(e) => {
                    event_handler_error.lock().unwrap().replace(e);
                    should_quit = true;
                }
            }
            if should_quit {
                break;
            }
        }
        event_handler_state
            .write()
            .map(|mut x| x.should_quit = true)
            .unwrap();
    });

    let mut terminal = setup()?;

    let mut interval = Interval::every(Duration::from_millis(33));
    while let Ok(state) = state.read() {
        if handle.is_finished() {
            info!("State handler quit");
            break;
        }

        if servo.is_finished() {
            info!("Servo quit");
            match servo.join() {
                Err(_) => {
                    warn!("Servo panicked");
                }
                Ok(Err(e)) => {
                    warn!("TUI error ({e})");
                }
                _ => {}
            }
            break;
        }

        if state.should_quit {
            info!("Should quit issued");
            break;
        }

        TICK_COUNTER.with(|t| t.borrow_mut().new_tick());
        if let Err(e) = terminal.draw(|f| render(&state, f)) {
            error.lock().unwrap().replace(e.into());
            break;
        }
        drop(state);
        interval.tick();
    }

    drop(handle);

    wrap_up(terminal)?;

    if let Some(error) = error.lock().unwrap().take() {
        return Err(error);
    }

    Ok(())
}

fn render(state: &TuiStates, f: &mut Frame<Backend>) {
    let layout = Layout::default()
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(f.size());

    let tabs = Tabs::new(state);
    f.render_widget(tabs, layout[0]);

    let main = layout[1];

    route(state, main, f);
    render_api_result_popup(state, f);
    render_api_input_popup(state, f);
}

fn render_api_result_popup(state: &TuiStates, f: &mut Frame<Backend>) {
    let Some(popup) = &state.api_result_popup else {
        return;
    };

    let area = centered_rect(78, 62, f.size());
    let block = Block::default()
        .title(Span::styled(
            format!(" {} ", popup.title),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let mut inner = block.inner(area);
    inner.width = inner.width.saturating_sub(1);
    inner.height = inner.height.saturating_sub(1);
    let text = scrolled_text(&popup.body, popup.offset.x, popup.offset.y, inner.width as usize);

    f.render_widget(Clear, area);
    f.render_widget(block, area);
    f.render_widget(
        Paragraph::new(text)
            .style(Style::default().fg(Color::White))
            .wrap(Wrap { trim: false }),
        inner,
    );
    render_scrollbars(&popup.body, popup.offset.x, popup.offset.y, inner, f);
}

fn render_api_input_popup(state: &TuiStates, f: &mut Frame<Backend>) {
    let Some(api_state) = state.active_api_state() else {
        return;
    };
    if !api::is_editing(api_state) {
        return;
    }
    let Some(label) = api::current_param_label(api_state) else {
        return;
    };
    let value = api::current_param_value(api_state).unwrap_or_default();
    let area = centered_rect(54, 18, f.size());
    let block = Block::default()
        .title(Span::styled(
            format!(" Edit {label} "),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));
    let inner = block.inner(area);

    f.render_widget(Clear, area);
    f.render_widget(block, area);
    f.render_widget(
        Paragraph::new(format!(
            "{label}: {value}\n\nTab next param | Ctrl+U clear | Enter run | Esc cancel"
        ))
            .style(Style::default().fg(Color::White))
            .wrap(Wrap { trim: false }),
        inner,
    );
}

fn scrolled_text(body: &str, x_offset: usize, y_offset: usize, width: usize) -> String {
    body.lines()
        .skip(y_offset)
        .map(|line| {
            let mut visible = line.chars().skip(x_offset).take(width).collect::<String>();
            visible.push('\n');
            visible
        })
        .collect()
}

fn render_scrollbars(
    body: &str,
    x_offset: usize,
    y_offset: usize,
    inner: Rect,
    f: &mut Frame<Backend>,
) {
    let line_count = body.lines().count().max(1);
    let max_width = body.lines().map(|line| line.chars().count()).max().unwrap_or(1);
    let visible_height = inner.height as usize;
    let visible_width = inner.width as usize;

    let vertical = scrollbar(y_offset, visible_height, line_count, visible_height, '│', '█')
        .chars()
        .map(|ch| format!("{ch}\n"))
        .collect::<String>();
    f.render_widget(
        Paragraph::new(vertical),
        Rect {
            x: inner.x.saturating_add(inner.width),
            y: inner.y,
            width: 1,
            height: inner.height,
        },
    );

    let horizontal = scrollbar(x_offset, visible_width, max_width, visible_width, '─', '█');
    f.render_widget(
        Paragraph::new(horizontal),
        Rect {
            x: inner.x,
            y: inner.y.saturating_add(inner.height),
            width: inner.width,
            height: 1,
        },
    );
}

fn scrollbar(
    offset: usize,
    visible: usize,
    total: usize,
    len: usize,
    track: char,
    thumb: char,
) -> String {
    if len == 0 {
        return String::new();
    }
    if total <= visible || visible == 0 {
        return std::iter::repeat(thumb).take(len).collect();
    }
    let thumb_len = (len * visible / total).max(1).min(len);
    let max_offset = total.saturating_sub(visible);
    let start = (offset.min(max_offset) * (len.saturating_sub(thumb_len)) / max_offset.max(1))
        .min(len.saturating_sub(thumb_len));
    (0..len)
        .map(|index| {
            if index >= start && index < start + thumb_len {
                thumb
            } else {
                track
            }
        })
        .collect()
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}
