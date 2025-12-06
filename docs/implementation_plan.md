# Rust TUI Project Setup

## Goal Description
Create a Rust project for a TUI application with specific dependencies (`ratatui`, `crossterm`, `tokio`, `portable-pty`, `tui-textarea`, `anyhow`) and a basic "Hello World" application that handles terminal raw mode correctly.

## Proposed Changes

#### [MODIFY] [Cargo.toml](file:///home/xtoxico/workspace/tachyonterm/Cargo.toml)
- Add dependencies:
    - `reqwest` (features = ["json", "tokio-native-tls"])
    - `serde` (features = ["derive"])
    - `serde_json`

### Source Code
#### [MODIFY] [src/main.rs](file:///home/xtoxico/workspace/tachyonterm/src/main.rs)
- Add imports: `reqwest`, `serde::{Deserialize, Serialize}`, `serde_json`.
- Define `ChatMessage` struct (role, content).
- Define API request/response structs (Gemini API format).
- Update `App` struct:
    - `chat_history`: `Arc<Mutex<Vec<ChatMessage>>>`
    - `active_focus`: `Focus` (enum `Chat`, `Terminal`)
    - `chat_input`: `TextArea<'static>`
    - `pty_master`: `Arc<Mutex<Box<dyn MasterPty + Send>>>`
- Implement `impl App`:
    - `fn send_message(&mut self, message: String)`:
        - Add user message to history.
        - Spawn `tokio` task.
        - Build JSON body for Gemini API.
        - POST to `https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:generateContent?key=$GEMINI_API_KEY`.
        - Await response, parse JSON.
        - Add model response to history.
- Update `main` initialization of `App`.
    - Initialize `TextArea`.
    - Wrap `pair.master` in `Arc<Mutex<Box<dyn MasterPty + Send>>>`.
- Update `run_app`:
    - Handle `Tab` to toggle focus.
    - Render `chat_input` in Input block.
    - Handle Input:
        - `Focus::Chat`: Pass to `chat_input`. If `Enter`, call `send_message`.
        - `Focus::Terminal`: Write to `pty_master`.
            - `Enter` -> `\r`
            - `Backspace` -> `\x08`
            - `Char(c)` -> `c`

## Verification Plan
### Automated Tests
- `cargo check`.
- Manual verification:
    - Tab switches focus (visual indication?).
    - Typing in Chat works.
    - Sending message works.
    - Typing in Terminal sends characters to PTY (e.g. `ls`, `pwd`).


## Context Injection (Warp Feature)
### Goal
Enhance AI responses by injecting the last 60 lines of terminal output into the prompt, allowing the AI to "see" what's happening in the terminal.

### Proposed Changes
#### [MODIFY] [src/main.rs](file:///home/xtoxico/workspace/tachyonterm/src/main.rs)
- Update `send_message` function:
    - Lock `self.buffer` and retrieve the last 60 lines.
    - Construct `full_prompt` combining:
        - Terminal Context
        - User Question
        - System Instructions
    - Send `full_prompt` to Gemini API.
    - **Crucial**: Ensure `chat_history` only displays the original user message, keeping the context injection invisible to the user.

### Verification
- **Manual Testing**:
    - Run a command that produces output (e.g., `ls -la`, or a command that fails).
    - Ask the AI "What does the last command show?" or "Fix the error".
    - Verify the AI response references the terminal output.
    - Verify the Chat UI only shows the short question.
