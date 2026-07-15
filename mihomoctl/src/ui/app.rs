use std::{
    cell::RefCell,
    fs::OpenOptions,
    io::{self, Stdout},
    sync::{
        mpsc::{channel, Sender},
        Arc, Mutex, RwLock,
    },
    thread::{spawn, JoinHandle},
    time::{Duration, Instant},
};

use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use log::{info, warn};
use tui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame, Terminal,
};
use url::Url;

// use clap::Parser;
use crate::{
    interactive::{ControllerKind, Flags},
    servo,
    ui::{
        api, components::Tabs, get_config, get_config_mut, init_config, input_job, pages::route,
        ApiResultPopup, Event, Interval, LoggerBuilder, NoticePopup, ServerPopupMode, TicksCounter,
        TuiOpt, TuiResult, TuiStates,
    },
};

thread_local!(pub(crate) static TICK_COUNTER: RefCell<TicksCounter> = RefCell::new(TicksCounter::new_with_time(Instant::now())));

pub type Backend = CrosstermBackend<Stdout>;

fn setup() -> TuiResult<Terminal<Backend>> {
    let mut stdout = io::stdout();

    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    enable_raw_mode()?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    Ok(terminal)
}

fn wrap_up(mut terminal: Terminal<Backend>) -> TuiResult<()> {
    execute!(terminal.backend_mut(), DisableMouseCapture, LeaveAlternateScreen)?;

    disable_raw_mode()?;

    Ok(())
}

pub fn main_loop(opt: TuiOpt, flag: Flags) -> TuiResult<()> {
    let config = flag.get_config()?;
    // Without a configured server the TUI still starts: the server manager
    // popup opens right away so the user can add one from within the TUI.
    let server = config.using_server().map(ToOwned::to_owned);
    let controller_kind = server.as_ref().map(|server| server.kind).unwrap_or_default();

    init_config(config);

    // Keyboard input and log records must outlive a single server session, so
    // they go through a persistent bus that forwards to the active session.
    let (bus_tx, bus_rx) = channel();
    let session_tx: Arc<Mutex<Option<Sender<Event>>>> = Arc::new(Mutex::new(None));
    {
        let session_tx = session_tx.clone();
        spawn(move || {
            while let Ok(event) = bus_rx.recv() {
                if let Some(tx) = session_tx.lock().unwrap().as_ref() {
                    let _ = tx.send(event);
                }
            }
        });
    }
    {
        let bus_tx = bus_tx.clone();
        spawn(move || input_job(bus_tx));
    }

    LoggerBuilder::new(bus_tx)
        .file(get_config().tui.log_file.as_ref().map(|x| {
            OpenOptions::new()
                .append(true)
                .create(true)
                .open(x)
                .unwrap()
        }))
        .apply()?;
    info!("Logger set");

    let mut terminal = setup()?;

    let mut controller_kind = controller_kind;
    let mut notice = None;
    let result = loop {
        match run_session(
            &mut terminal,
            &opt,
            &flag,
            controller_kind,
            &session_tx,
            notice.take(),
        ) {
            Ok(SessionOutcome::Quit) => break Ok(()),
            Ok(SessionOutcome::Switch(url)) => match switch_server(&flag, &url) {
                Ok(kind) => {
                    controller_kind = kind;
                    notice = Some(SessionNotice::Success(format!("Switched to {url}")));
                }
                Err(message) => notice = Some(SessionNotice::Failure(message)),
            },
            Err(e) => break Err(e),
        }
    };

    wrap_up(terminal)?;

    result
}

enum SessionOutcome {
    Quit,
    Switch(Url),
}

/// Feedback of a server switch shown when the next session starts. Success is
/// a transient notice (any key or timeout closes it); failure stays up until
/// dismissed.
enum SessionNotice {
    Success(String),
    Failure(String),
}

/// Verify the target server is reachable, then persist it as the active one.
/// On failure the config is left untouched and a message for the user is
/// returned instead.
fn switch_server(flag: &Flags, url: &Url) -> Result<ControllerKind, String> {
    let server = get_config()
        .servers
        .iter()
        .find(|server| &server.url == url)
        .cloned()
        .ok_or_else(|| format!("Server {url} is not in the config anymore"))?;

    server
        .clone()
        .into_clash_with_timeout(Some(Duration::from_millis(flag.timeout)))
        .map_err(|e| e.to_string())
        .and_then(|clash| clash.get_version().map_err(|e| e.to_string()))
        .map_err(|e| format!("Failed to connect to {url}: {e}\n\nStaying on the current server."))?;

    let mut config = get_config_mut();
    config.use_server(url.to_owned()).map_err(|e| e.to_string())?;
    config.write().map_err(|e| e.to_string())?;

    Ok(server.kind)
}

fn run_session(
    terminal: &mut Terminal<Backend>,
    opt: &TuiOpt,
    flag: &Flags,
    controller_kind: ControllerKind,
    session_tx: &Arc<Mutex<Option<Sender<Event>>>>,
    notice: Option<SessionNotice>,
) -> TuiResult<SessionOutcome> {
    let mut initial_state = TuiStates::for_controller_kind(controller_kind);
    match notice {
        Some(SessionNotice::Success(body)) => {
            initial_state.notice_popup = Some(NoticePopup::new("Server Switched", body));
        }
        Some(SessionNotice::Failure(body)) => {
            initial_state.api_result_popup = Some(ApiResultPopup {
                title: "Switch Server".to_owned(),
                body,
                offset: Default::default(),
            });
        }
        None => {}
    }

    // Without an active server there is nothing to poll: skip the servo and
    // greet the user with the server manager so they can add or pick one.
    let has_server = {
        let config = get_config();
        let has_server = config.using_server().is_some();
        if !has_server {
            let servers = config.servers.clone();
            let active = config.using.clone();
            drop(config);
            initial_state.open_server_popup_with(servers, active);
            if let Some(popup) = initial_state.server_popup.as_mut() {
                popup.message = Some(
                    "No active server: press a to add one, Enter to use the selected".to_owned(),
                );
            }
        }
        has_server
    };

    let state = Arc::new(RwLock::new(initial_state));
    let error = Arc::new(Mutex::new(None));

    let (event_tx, event_rx) = channel();
    let (action_tx, action_rx) = channel();

    *session_tx.lock().unwrap() = Some(event_tx.clone());

    let mut servo = has_server.then(|| {
        let servo_opt = opt.clone();
        let servo_flag = flag.clone();
        spawn(move || servo(event_tx, action_rx, servo_opt, servo_flag))
    });

    let event_handler_state = state.clone();
    let event_handler_error = error.clone();

    let handle = spawn(move || {
        let mut should_quit;
        while let Ok(event) = event_rx.recv() {
            should_quit = event.is_quit();
            let mut state = event_handler_state.write().unwrap();
            match state.handle(event) {
                Ok(Some(action)) => {
                    // The servo may have quit after losing the server; the UI
                    // stays alive so the user can switch servers or quit.
                    if action_tx.send(action).is_err() {
                        warn!("Server connection lost, action dropped");
                    }
                }
                // No action needed
                Ok(None) => {}
                Err(e) => {
                    event_handler_error.lock().unwrap().replace(e);
                    should_quit = true;
                }
            }
            // Exiting here drops the event/action channels, which unwinds the
            // servo jobs of this session on their next send.
            if should_quit || state.should_quit {
                break;
            }
        }
        event_handler_state
            .write()
            .map(|mut x| x.should_quit = true)
            .unwrap();
    });

    let mut interval = Interval::every(Duration::from_millis(33));
    loop {
        if handle.is_finished() {
            info!("State handler quit");
            break;
        }

        // A dead servo (e.g. the server became unreachable) is not fatal: the
        // UI keeps running so the user can switch servers or quit.
        if servo.as_ref().map(JoinHandle::is_finished).unwrap_or(false) {
            info!("Servo quit");
            let message = servo_failure_message(servo.take().expect("checked above").join());
            let Ok(mut state) = state.write() else { break };
            state.api_result_popup = Some(ApiResultPopup {
                title: "Server Connection Error".to_owned(),
                body: server_failure_body(&message),
                offset: Default::default(),
            });
        }

        let Ok(state) = state.read() else { break };

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

    // Stop forwarding input into this session's (soon to be dead) channel.
    session_tx.lock().unwrap().take();

    if let Some(error) = error.lock().unwrap().take() {
        return Err(error);
    }

    let switch = state.read().unwrap().switch_to_server.clone();
    match switch {
        Some(url) => Ok(SessionOutcome::Switch(url)),
        None => Ok(SessionOutcome::Quit),
    }
}

fn servo_failure_message(result: std::thread::Result<TuiResult<()>>) -> String {
    match result {
        Err(_) => {
            warn!("Servo panicked");
            "Background worker panicked".to_owned()
        }
        Ok(Err(e)) => {
            warn!("TUI error ({e})");
            e.to_string()
        }
        Ok(Ok(())) => "Background worker stopped".to_owned(),
    }
}

fn server_failure_body(message: &str) -> String {
    format!(
        "The configured service is unavailable.\n\n{message}\n\nData updates are stopped.\nPress Shift+S to select another server, Esc to close this message, or q to quit."
    )
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
    render_server_popup(state, f);
    render_mode_popup(state, f);
    render_confirm_popup(state, f);
    render_notice_popup(state, f);
    render_help_popup(state, f);
}

const HELP_TEXT: &str = "\
Global
  1-8         Go to tab
  [ / ]       Previous / next tab
  q, x        Quit
  Ctrl+C      Force quit
  Ctrl+D      Toggle debug page
  Shift-S     Manage / switch servers
  Shift-H     Toggle this help
  Space       Hold list, then move with arrow keys
  s / Alt+s   Next / previous sort method
  Esc         Close popup, leave hold or search mode

Status
  m           Open mode switcher (rule / global / direct)
  r           Refresh configs from the server
  Shift-R     Reload configs from the server's config file
  g           Update geo databases (POST /configs/geo)

Proxies
  Space/Enter Expand current group
  Up / Down   Move cursor
  Right/Enter Switch to selected node in expanded selector group
  t           Test latency of current group
  Shift-T     Test latency of all proxies
  /           Search proxies

Rules / Conns / Logs
  /           Search
  p           Pause updates (Conns, Logs)

Conns
  k           Close the selected connection
  Shift-K     Close all connections (Enter to confirm)

API pages (Core / DNS / APIs)
  Up / Down   Select operation
  Enter       Run operation (dangerous ones ask to confirm)
  i           Edit parameters
  Tab         Next parameter
  p           Pick parameter from current context
  Ctrl+U      Clear parameter while editing

Server manager (Shift-S)
  Up / Down   Select server
  Enter       Switch to selected server
  a           Add server
  d           Delete selected server (Enter to confirm)

Popups
  Arrow keys  Scroll
  Esc         Close";

fn render_help_popup(state: &TuiStates, f: &mut Frame<Backend>) {
    let Some(offset) = &state.help_popup else {
        return;
    };

    let area = centered_rect(70, 76, f.size());
    let block = Block::default()
        .title(Span::styled(
            " Help ",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let mut inner = block.inner(area);
    inner.width = inner.width.saturating_sub(1);
    inner.height = inner.height.saturating_sub(1);
    let text = scrolled_text(HELP_TEXT, offset.x, offset.y, inner.width as usize);

    f.render_widget(Clear, area);
    f.render_widget(block, area);
    f.render_widget(
        Paragraph::new(text).style(Style::default().fg(Color::White)),
        inner,
    );
    render_scrollbars(HELP_TEXT, offset.x, offset.y, inner, f);
}

fn render_mode_popup(state: &TuiStates, f: &mut Frame<Backend>) {
    use crate::ui::ModeSwitchPopup;

    let Some(popup) = &state.mode_popup else {
        return;
    };

    let area = centered_rect(36, 24, f.size());
    let block = Block::default()
        .title(Span::styled(
            " Switch Mode ",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(area);

    let mut lines = ModeSwitchPopup::MODES
        .iter()
        .enumerate()
        .map(|(index, mode)| {
            let is_current = *mode == popup.current;
            let marker = if is_current { "→ " } else { "  " };
            let content = format!("{}{:?}", marker, mode);
            let style = if index == popup.index {
                Style::default().fg(Color::Black).bg(Color::Cyan)
            } else if is_current {
                Style::default().fg(Color::Green)
            } else {
                Style::default().fg(Color::White)
            };
            Spans::from(Span::styled(content, style))
        })
        .collect::<Vec<_>>();

    lines.push(Spans::default());
    lines.push(Spans::from(Span::styled(
        "↑/↓ select | Enter apply | Esc close",
        Style::default().fg(Color::DarkGray),
    )));

    f.render_widget(Clear, area);
    f.render_widget(block, area);
    f.render_widget(Paragraph::new(lines), inner);
}

fn render_confirm_popup(state: &TuiStates, f: &mut Frame<Backend>) {
    let Some(popup) = &state.confirm_popup else {
        return;
    };

    let area = centered_rect(46, 20, f.size());
    let block = Block::default()
        .title(Span::styled(
            format!(" {} ", popup.title),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Red));
    let inner = block.inner(area);

    let lines = vec![
        Spans::from(Span::styled(
            popup.body.clone(),
            Style::default().fg(Color::White),
        )),
        Spans::default(),
        Spans::from(Span::styled(
            "Enter confirm | Esc cancel",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    f.render_widget(Clear, area);
    f.render_widget(block, area);
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn render_notice_popup(state: &TuiStates, f: &mut Frame<Backend>) {
    let Some(notice) = &state.notice_popup else {
        return;
    };

    let area = centered_rect(50, 20, f.size());
    let block = Block::default()
        .title(Span::styled(
            format!(" {} ", notice.title),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Green));
    let inner = block.inner(area);

    let lines = vec![
        Spans::from(Span::styled(
            notice.body.clone(),
            Style::default().fg(Color::White),
        )),
        Spans::default(),
        Spans::from(Span::styled(
            format!(
                "Press any key to dismiss · auto-closes in {}s",
                notice.remaining_secs()
            ),
            Style::default().fg(Color::DarkGray),
        )),
    ];

    f.render_widget(Clear, area);
    f.render_widget(block, area);
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn render_server_popup(state: &TuiStates, f: &mut Frame<Backend>) {
    let Some(popup) = &state.server_popup else {
        return;
    };

    let (title, border) = match &popup.mode {
        ServerPopupMode::List => (" Servers ", Color::Cyan),
        ServerPopupMode::ConfirmDelete => (" Delete Server ", Color::Red),
        ServerPopupMode::Add(_) => (" Add Server ", Color::Yellow),
    };
    let area = centered_rect(62, 46, f.size());
    let block = Block::default()
        .title(Span::styled(
            title,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border));
    let inner = block.inner(area);

    let lines = match &popup.mode {
        ServerPopupMode::Add(form) => server_form_lines(form),
        _ => server_list_lines(popup),
    };

    f.render_widget(Clear, area);
    f.render_widget(block, area);
    f.render_widget(Paragraph::new(lines), inner);
}

fn server_list_lines<'a>(popup: &'a crate::ui::ServerSwitchPopup) -> Vec<Spans<'a>> {
    let mut lines = popup
        .servers
        .iter()
        .enumerate()
        .map(|(index, server)| {
            let is_active = popup.active.as_ref() == Some(&server.url);
            let marker = if is_active { "→ " } else { "  " };
            let content = format!("{}{:<8}{}", marker, server.kind.as_str(), server.url);
            let style = if index == popup.index {
                Style::default().fg(Color::Black).bg(Color::Cyan)
            } else if is_active {
                Style::default().fg(Color::Green)
            } else {
                Style::default().fg(Color::White)
            };
            Spans::from(Span::styled(content, style))
        })
        .collect::<Vec<_>>();
    if popup.servers.is_empty() {
        lines.push(Spans::from(Span::styled(
            "No servers configured",
            Style::default().fg(Color::DarkGray),
        )));
    }
    lines.push(Spans::default());
    match popup.mode {
        ServerPopupMode::ConfirmDelete => {
            lines.push(Spans::from(Span::styled(
                format!("Delete {} ?", popup.servers[popup.index].url),
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            )));
            lines.push(Spans::from(Span::styled(
                "Enter confirm | Esc cancel",
                Style::default().fg(Color::DarkGray),
            )));
        }
        _ => {
            if let Some(message) = &popup.message {
                lines.push(Spans::from(Span::styled(
                    message.clone(),
                    Style::default().fg(Color::Yellow),
                )));
            }
            lines.push(Spans::from(Span::styled(
                "↑/↓ select | Enter switch | a add | d delete | Esc close",
                Style::default().fg(Color::DarkGray),
            )));
        }
    }
    lines
}

fn server_form_lines<'a>(form: &'a crate::ui::ServerForm) -> Vec<Spans<'a>> {
    use crate::ui::ServerFormField as Field;

    let field_style = |field: Field| {
        if form.field == field {
            Style::default().fg(Color::Black).bg(Color::Yellow)
        } else {
            Style::default().fg(Color::White)
        }
    };
    let cursor = |field: Field| if form.field == field { "█" } else { "" };

    let mut lines = vec![
        Spans::from(Span::styled(
            format!("URL:    {}{}", form.url, cursor(Field::Url)),
            field_style(Field::Url),
        )),
        Spans::from(Span::styled(
            format!(
                "Secret: {}{}",
                "*".repeat(form.secret.chars().count()),
                cursor(Field::Secret)
            ),
            field_style(Field::Secret),
        )),
        Spans::from(Span::styled(
            format!("Kind:   ◀ {} ▶", form.kind.as_str()),
            field_style(Field::Kind),
        )),
        Spans::default(),
    ];
    if let Some(error) = &form.error {
        lines.push(Spans::from(Span::styled(
            error.clone(),
            Style::default().fg(Color::Red),
        )));
    }
    lines.push(Spans::from(Span::styled(
        "Tab/↑/↓ switch field | ◀/▶ toggle kind | Enter save | Esc back",
        Style::default().fg(Color::DarkGray),
    )));
    lines
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn servo_failure_is_turned_into_a_user_message() {
        let message = servo_failure_message(Ok(Err(crate::ui::TuiError::TuiInternalErr)));
        assert_eq!(message, crate::ui::TuiError::TuiInternalErr.to_string());

        assert_eq!(
            servo_failure_message(Ok(Ok(()))),
            "Background worker stopped"
        );
    }

    #[test]
    fn server_failure_prompt_includes_error_and_switch_shortcut() {
        let body = server_failure_body("authentication failed: 401");

        assert!(body.contains("authentication failed: 401"));
        assert!(body.contains("Shift+S"));
    }
}
