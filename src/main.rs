use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use directories::ProjectDirs;
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::Line,
    widgets::{Block, Borders, Clear, Paragraph},
    Terminal,
};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs::{self, OpenOptions};
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

#[derive(PartialEq)]
enum AppState {
    Setup,
    Running,
}

#[derive(Serialize, Deserialize)]
struct Config {
    api_key: String,
}

impl Config {
    fn load() -> Result<Self> {
        if let Some(proj_dirs) = ProjectDirs::from("com", "tachyonterm", "tachyonterm") {
            let config_path = proj_dirs.config_dir().join("config.toml");
            if config_path.exists() {
                let content = fs::read_to_string(config_path)?;
                let config: Config = toml::from_str(&content)?;
                return Ok(config);
            }
        }
        Err(anyhow::anyhow!("Config file not found"))
    }

    fn save(&self) -> Result<()> {
        if let Some(proj_dirs) = ProjectDirs::from("com", "tachyonterm", "tachyonterm") {
            let config_dir = proj_dirs.config_dir();
            if !config_dir.exists() {
                fs::create_dir_all(config_dir)?;
            }
            let config_path = config_dir.join("config.toml");
            let content = toml::to_string(self)?;
            fs::write(config_path, content)?;
            Ok(())
        } else {
            Err(anyhow::anyhow!("Could not determine config directory"))
        }
    }
}

struct App {
    buffer: Arc<Mutex<Vec<String>>>,
    chat_history: Arc<Mutex<Vec<ChatMessage>>>,
    active_focus: Focus,
    chat_input: TextArea<'static>,
    pty_master: Arc<Mutex<Box<dyn MasterPty>>>,
    state: AppState,
    config: Option<Config>,
    setup_input: TextArea<'static>,
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
        // 1. Mostrar mensaje del usuario inmediatamente en la UI
        {
            let mut history = self.chat_history.lock().unwrap();
            history.push(ChatMessage {
                role: "user".to_string(),
                content: message.clone(),
            });
        }

        let chat_history = self.chat_history.clone();
        let buffer_clone = self.buffer.clone(); // Necesitamos clonar el puntero al buffer
        
        // Usar la API Key de la config si existe, sino variable de entorno (fallback)
        let api_key = if let Some(config) = &self.config {
            config.api_key.clone()
        } else {
            env::var("GEMINI_API_KEY").unwrap_or_else(|_| "$GEMINI_API_KEY".to_string())
        };

        // 2. Lógica asíncrona
        tokio::spawn(async move {
            // --- FASE 1: Recolectar Contexto ---
            let context_text = {
                let locked_buffer = buffer_clone.lock().unwrap();
                let len = locked_buffer.len();
                // Cogemos las últimas 50 líneas para no saturar
                let start = len.saturating_sub(50); 
                locked_buffer[start..].join("\n")
            };

            // --- FASE 2: Construir el Prompt Maestro ---
            let full_prompt = format!(
                "CONTEXTO TERMINAL:\n{}\n\nUSUARIO DICE:\n{}", 
                context_text, 
                message
            );

            // LOGGING DE DEBUG
            let mut file = OpenOptions::new()
                .create(true)
                .append(true)
                .open("debug.log")
                .unwrap();
            
            writeln!(file, "--- ENVIANDO REQUEST ---\nKey: {}\nPrompt Length: {}", api_key, full_prompt.len()).ok();

            let client = reqwest::Client::new();
            
            let request_body = GeminiRequest {
                contents: vec![GeminiContent {
                    parts: vec![GeminiPart { text: full_prompt }],
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

            // --- FASE 3: Gestionar Respuesta ---
            match response {
                Ok(resp) => {
                    let status = resp.status();
                    writeln!(file, "Status Code: {}", status).ok();
                    
                    if !status.is_success() {
                        let error_text = resp.text().await.unwrap_or_default();
                        writeln!(file, "API ERROR BODY: {}", error_text).ok();
                        
                        let mut history = chat_history.lock().unwrap();
                        history.push(ChatMessage {
                            role: "model".to_string(),
                            content: format!("Error API ({}): Mira debug.log", status),
                        });
                        return;
                    }

                    match resp.json::<GeminiResponse>().await {
                        Ok(gemini_resp) => {
                            if let Some(candidates) = gemini_resp.candidates {
                                if let Some(candidate) = candidates.first() {
                                    if let Some(part) = candidate.content.parts.first() {
                                        let mut history = chat_history.lock().unwrap();
                                        history.push(ChatMessage {
                                            role: "model".to_string(),
                                            content: part.text.clone(),
                                        });
                                        writeln!(file, "Respuesta recibida y parseada OK").ok();
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            writeln!(file, "Error parseando JSON: {:?}", e).ok();
                        }
                    }
                }
                Err(e) => {
                    writeln!(file, "Error HTTP: {:?}", e).ok();
                    let mut history = chat_history.lock().unwrap();
                    history.push(ChatMessage {
                        role: "model".to_string(),
                        content: format!("Error de conexión: {}", e),
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

    let mut setup_input = TextArea::default();
    setup_input.set_block(Block::default().borders(Borders::ALL).title("API Key"));
    setup_input.set_placeholder_text("Pegue su Google Gemini API Key aquí...");

    // Intentar cargar config
    let (state, config) = match Config::load() {
        Ok(cfg) => (AppState::Running, Some(cfg)),
        Err(_) => (AppState::Setup, None),
    };

    let mut app = App {
        buffer: Arc::new(Mutex::new(vec![String::new()])),
        chat_history: Arc::new(Mutex::new(Vec::new())),
        active_focus: Focus::Terminal,
        chat_input,
        pty_master: Arc::new(Mutex::new(pair.master)),
        state,
        config,
        setup_input,
    };

    // Spawn background reader task
    let master_guard = app.pty_master.lock().unwrap();
    // Try to get a reader from the master
    // We need to clone the reader. portable-pty's MasterPty has try_clone_reader.
    // Since we are holding the lock, we can call it.
    let mut reader = master_guard.try_clone_reader()?;
    // Drop lock so we can move pty_master into App later if needed (already moved)
    // Actually app.pty_master is Arc<Mutex<Box<dyn MasterPty>>>.
    // We locked it to get reader. Now drop guard.
    drop(master_guard);
    
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
            if app.state == AppState::Setup {
                // Render Setup UI
                let area = centered_rect(60, 20, f.area());
                let popup_block = Block::default()
                    .title(" Configuración Inicial ")
                    .borders(Borders::ALL)
                    .style(Style::default().bg(Color::Blue).fg(Color::White));
                
                f.render_widget(Clear, area); // Clear background
                f.render_widget(popup_block, area);

                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .margin(2)
                    .constraints([
                        Constraint::Length(3), // Texto bienvenida
                        Constraint::Length(3), // Input
                        Constraint::Min(1),    // Espacio
                    ])
                    .split(area);

                let text = vec![
                    Line::from("Bienvenido a TachyonTerm."),
                    Line::from("No se ha detectado configuración."),
                    Line::from("Por favor, introduce tu Google Gemini API Key:"),
                ];
                f.render_widget(Paragraph::new(text), chunks[0]);
                
                f.render_widget(&app.setup_input, chunks[1]);
            } else {
                // Render Running UI
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
            }
        })?;

        if crossterm::event::poll(Duration::from_millis(100))? {
            let event = event::read()?;
            
            // Global shortcuts
            if let Event::Key(key) = event {
                if key.code == KeyCode::Char('q') && key.modifiers.contains(KeyModifiers::CONTROL) {
                    return Ok(());
                }
            }

            match app.state {
                AppState::Setup => {
                    if let Event::Key(key) = event {
                        if key.code == KeyCode::Enter {
                            let lines = app.setup_input.lines();
                            let api_key = lines.join("").trim().to_string();
                            if !api_key.is_empty() {
                                let config = Config { api_key: api_key.clone() };
                                if let Err(_e) = config.save() {
                                    // En una app real mostraríamos error, aquí lo logueamos o ignoramos por simplicidad
                                    // O podríamos cambiar el texto del popup
                                } else {
                                    app.config = Some(config);
                                    app.state = AppState::Running;
                                }
                            }
                        } else {
                            app.setup_input.input(event);
                        }
                    }
                }
                AppState::Running => {
                    if let Event::Key(key) = event {
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
                            if let Event::Key(key) = event {
                                if key.code == KeyCode::Enter {
                                    let lines = app.chat_input.lines();
                                    let message = lines.join("\n");
                                    if !message.trim().is_empty() {
                                        app.send_message(message);
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
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
