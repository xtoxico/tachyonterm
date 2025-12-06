use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    widgets::{Block, Borders, Paragraph},
    Terminal,
};
use serde::{Deserialize, Serialize};
use std::env;
use std::io::{stdout, Read, Stdout, Write};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tui_textarea::TextArea;

#[derive(Clone, Serialize, Deserialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(PartialEq)]
enum Focus {
    Chat,
    Terminal,
}

struct App {
    buffer: Arc<Mutex<Vec<String>>>,
    chat_history: Arc<Mutex<Vec<ChatMessage>>>,
    active_focus: Focus,
    chat_input: TextArea<'static>,
    pty_master: Arc<Mutex<Box<dyn MasterPty>>>,
}

#[derive(Serialize)]
struct GeminiRequest {
    contents: Vec<GeminiContent>,
}

#[derive(Serialize)]
struct GeminiContent {
    parts: Vec<GeminiPart>,
}

#[derive(Serialize)]
struct GeminiPart {
    text: String,
}

#[derive(Deserialize)]
struct GeminiResponse {
    candidates: Option<Vec<GeminiCandidate>>,
}

#[derive(Deserialize)]
struct GeminiCandidate {
    content: GeminiContentResponse,
}

#[derive(Deserialize)]
struct GeminiContentResponse {
    parts: Vec<GeminiPartResponse>,
}

#[derive(Deserialize)]
struct GeminiPartResponse {
    text: String,
}

impl App {
    fn send_message(&mut self, message: String) {
        // Add user message to history
        {
            let mut history = self.chat_history.lock().unwrap();
            history.push(ChatMessage {
                role: "user".to_string(),
                content: message.clone(),
            });
        }

        let chat_history = self.chat_history.clone();
        let api_key = env::var("GEMINI_API_KEY").unwrap_or_else(|_| "$GEMINI_API_KEY".to_string());

        tokio::spawn(async move {
            let client = reqwest::Client::new();
            let request_body = GeminiRequest {
                contents: vec![GeminiContent {
                    parts: vec![GeminiPart { text: message }],
                }],
            };

            let response = client
                .post(format!(
                    "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:generateContent?key={}",
                    api_key
                ))
                .json(&request_body)
                .send()
                .await;

            match response {
                Ok(resp) => {
                    if let Ok(gemini_resp) = resp.json::<GeminiResponse>().await {
                        if let Some(candidates) = gemini_resp.candidates {
                            if let Some(candidate) = candidates.first() {
                                if let Some(part) = candidate.content.parts.first() {
                                    let mut history = chat_history.lock().unwrap();
                                    history.push(ChatMessage {
                                        role: "model".to_string(),
                                        content: part.text.clone(),
                                    });
                                }
                            }
                        }
                    } else {
                         let mut history = chat_history.lock().unwrap();
                         history.push(ChatMessage {
                             role: "model".to_string(),
                             content: "Error parsing response".to_string(),
                         });
                    }
                }
                Err(e) => {
                    let mut history = chat_history.lock().unwrap();
                    history.push(ChatMessage {
                        role: "model".to_string(),
                        content: format!("Error sending request: {}", e),
                    });
                }
            }
        });
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Setup PTY
    let pty_system = native_pty_system();
    let pair = pty_system.openpty(PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
    })?;

    // Spawn shell
    let cmd = CommandBuilder::new("/bin/bash");
    let _child = pair.slave.spawn_command(cmd)?;

    // Initialize App state
    let mut chat_input = TextArea::default();
    chat_input.set_block(Block::default().borders(Borders::ALL).title("Input"));

    let mut app = App {
        buffer: Arc::new(Mutex::new(vec![String::new()])),
        chat_history: Arc::new(Mutex::new(Vec::new())),
        active_focus: Focus::Terminal,
        chat_input,
        pty_master: Arc::new(Mutex::new(pair.master)),
    };

    // Spawn background reader task
    // We need to clone the reader from the master before we wrap it in the App, 
    // BUT pair.master is moved into app.pty_master.
    // So we should clone the reader FIRST.
    // However, pair.master is a Box<dyn MasterPty>.
    // Let's see if we can clone the reader from the locked master.
    // Actually, `try_clone_reader` is on `MasterPty`.
    
    // Better approach: Clone reader BEFORE moving master to App.
    let mut reader = app.pty_master.lock().unwrap().try_clone_reader()?;
    
    let buffer = app.buffer.clone();
    tokio::task::spawn_blocking(move || {
        let mut buf = [0u8; 1024];
        loop {
            match reader.read(&mut buf) {
                Ok(n) if n > 0 => {
                    let s = String::from_utf8_lossy(&buf[..n]);
                    let mut locked = buffer.lock().unwrap();
                    
                    let parts: Vec<&str> = s.split('\n').collect();
                    
                    if let Some(last) = locked.last_mut() {
                        last.push_str(parts[0]);
                    } else {
                        locked.push(parts[0].to_string());
                    }

                    for part in parts.iter().skip(1) {
                        locked.push(part.to_string());
                    }

                    if locked.len() > 1000 {
                        let len = locked.len();
                        locked.drain(0..len - 500);
                    }
                }
                Ok(_) => break, // EOF
                Err(_) => break, // Error
            }
        }
    });

    // Run app loop
    let res = run_app(&mut terminal, &mut app).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{:?}", err);
    }

    Ok(())
}

async fn run_app(terminal: &mut Terminal<CrosstermBackend<Stdout>>, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|f| {
            let main_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(50),
                    Constraint::Percentage(50),
                ])
                .split(f.area());

            let left_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Min(1),
                    Constraint::Length(3),
                ])
                .split(main_chunks[0]);

            let assistant_block = Block::default()
                .borders(Borders::ALL)
                .title("AI Assistant / Context")
                .style(if app.active_focus == Focus::Chat {
                    Style::default().fg(Color::Yellow)
                } else {
                    Style::default()
                });
            
            // Render Chat History
            let history = app.chat_history.lock().unwrap();
            let chat_text: String = history.iter()
                .map(|msg| format!("{}: {}", msg.role, msg.content))
                .collect::<Vec<String>>()
                .join("\n\n");
            
            f.render_widget(Paragraph::new(chat_text).block(assistant_block), left_chunks[0]);

            // Render Chat Input
            // Update block style based on focus
            let input_style = if app.active_focus == Focus::Chat {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default()
            };
            app.chat_input.set_block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Input")
                    .style(input_style)
            );
            f.render_widget(&app.chat_input, left_chunks[1]);

            let terminal_block = Block::default()
                .borders(Borders::ALL)
                .title("Local Terminal")
                .style(if app.active_focus == Focus::Terminal {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default()
                });
            
            let buffer = app.buffer.lock().unwrap();
            let height = main_chunks[1].height.saturating_sub(2) as usize;
            let start = buffer.len().saturating_sub(height);
            let display_text: String = buffer[start..].join("\n");
            
            f.render_widget(Paragraph::new(display_text).block(terminal_block), main_chunks[1]);
        })?;

        if crossterm::event::poll(Duration::from_millis(100))? {
            let event = event::read()?;
            
            // Global shortcuts
            if let Event::Key(key) = event {
                if key.code == KeyCode::Char('q') && key.modifiers.contains(KeyModifiers::CONTROL) {
                    return Ok(());
                }
                if key.code == KeyCode::Tab {
                    app.active_focus = match app.active_focus {
                        Focus::Chat => Focus::Terminal,
                        Focus::Terminal => Focus::Chat,
                    };
                    continue;
                }
            }

            match app.active_focus {
                Focus::Chat => {
                    // Pass event to textarea
                    // Convert crossterm event to tui-textarea input
                    // tui-textarea supports crossterm events directly via `input(impl Into<Input>)`
                    // But `event` is `crossterm::event::Event`. `Input::from(event)` works.
                    
                    // Check for Enter to send
                    if let Event::Key(key) = event {
                        if key.code == KeyCode::Enter {
                            let lines = app.chat_input.lines();
                            let message = lines.join("\n");
                            if !message.trim().is_empty() {
                                app.send_message(message);
                                // Clear input
                                app.chat_input = TextArea::default();
                                app.chat_input.set_block(Block::default().borders(Borders::ALL).title("Input"));
                            }
                            continue;
                        }
                    }
                    
                    app.chat_input.input(event);
                }
                Focus::Terminal => {
                    if let Event::Key(key) = event {
                        let master_guard = app.pty_master.lock().unwrap();
                        // Hack: MasterPty might not implement Write directly in this version?
                        // Try to use AsRawFd if available (Unix only)
                        #[cfg(unix)]
                        {
                            use std::os::unix::io::FromRawFd;
                            use std::fs::File;
                            use std::mem;
                            
                            // master_guard.as_raw_fd() returns Option<i32> in portable-pty
                            let fd = master_guard.as_raw_fd().expect("Failed to get PTY FD");
                            let mut file = unsafe { File::from_raw_fd(fd) };
                            
                            match key.code {
                                KeyCode::Char(c) => {
                                    let _ = write!(file, "{}", c);
                                }
                                KeyCode::Enter => {
                                    let _ = file.write_all(b"\r");
                                }
                                KeyCode::Backspace => {
                                    let _ = file.write_all(b"\x08");
                                }
                                KeyCode::Left => {
                                    let _ = file.write_all(b"\x1b[D");
                                }
                                KeyCode::Right => {
                                    let _ = file.write_all(b"\x1b[C");
                                }
                                KeyCode::Up => {
                                    let _ = file.write_all(b"\x1b[A");
                                }
                                KeyCode::Down => {
                                    let _ = file.write_all(b"\x1b[B");
                                }
                                _ => {}
                            }
                            
                            // Prevent closing the FD
                            mem::forget(file);
                        }
                    }
                }
            }
        }
    }
}
