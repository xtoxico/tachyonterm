# TachyonTerm

TachyonTerm is a Rust-based Terminal User Interface (TUI) application that combines a local terminal emulator with an AI assistant powered by the Gemini API.

## Features

*   **Split-Screen Interface**:
    *   **Left Panel**: AI Assistant context and chat input.
    *   **Right Panel**: Fully functional local terminal (running `/bin/bash`).
*   **AI Integration**:
    *   Chat with Google's Gemini 2.5 Flash model directly from the terminal.
    *   Context-aware assistance (future planned feature).
*   **Interactivity**:
    *   Seamless focus switching between Chat and Terminal using `Tab`.
    *   Real-time terminal output rendering.
    *   Multi-line chat input support.

## Prerequisites

*   Rust (latest stable)
*   `GEMINI_API_KEY` environment variable set with your Google Gemini API key.

## Installation & Running

1.  Clone the repository:
    ```bash
    git clone <repository-url>
    cd tachyonterm
    ```

2.  Set your API key:
    ```bash
    export GEMINI_API_KEY="your_api_key_here"
    ```

3.  Run the application:
    ```bash
    cargo run
    ```

## Controls

*   **Tab**: Switch focus between Chat Input and Local Terminal.
*   **Enter** (in Chat): Send message to AI.
*   **Ctrl+q**: Quit the application.

## Architecture

*   **Frontend**: Built with `ratatui` and `crossterm`.
*   **Async Runtime**: `tokio` for handling non-blocking I/O (PTY reading, API requests).
*   **Terminal Emulation**: `portable-pty` for spawning and controlling the pseudo-terminal.
*   **AI Client**: `reqwest` for communicating with the Gemini API.

## License

MIT
