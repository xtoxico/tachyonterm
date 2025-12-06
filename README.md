# TachyonTerm

**TachyonTerm** is a next-generation Rust-based Terminal User Interface (TUI) that seamlessly integrates a local terminal emulator with an AI assistant powered by Google's Gemini API. It's designed to be your intelligent pair programmer directly in the command line.

## 🚀 Features

### 🧠 AI-Powered Assistance
- **Context-Aware**: The AI sees your terminal output and history, allowing it to provide relevant answers without copy-pasting.
- **Gemini 2.5 Flash**: Powered by Google's latest fast and efficient model.
- **Markdown Rendering**: Rich text support in chat, including syntax-highlighted code blocks and headers.

### ⚡ Actionable Suggestions
- **Smart Extraction**: Automatically detects code blocks in AI responses and extracts them into a dedicated "Suggestions" panel.
- **One-Click Execution**: Navigate to a suggestion and press `Enter` to instantly execute it in your terminal.
- **Safety First**: Intelligent sanitization removes shell prompts (`$`, `#`) and output lines, ensuring only clean commands are executed.

### 🖥️ Professional UI
- **Split-Screen Layout**:
    - **Left Panel**: Chat History (Top), Input Box (Middle), Actionable Suggestions (Bottom).
    - **Right Panel**: Fully functional local terminal (running `/bin/bash`).
- **Setup Wizard**: User-friendly first-run experience to configure your API Key.
- **Auto-Scroll**: Chat automatically scrolls to the latest message.

## 🛠️ Installation

1.  **Clone the repository**:
    ```bash
    git clone <repository-url>
    cd tachyonterm
    ```

2.  **Run the application**:
    ```bash
    cargo run
    ```

3.  **First Run Configuration**:
    - On the first launch, you will be greeted by a Setup Wizard.
    - Follow the on-screen instructions to get your free Google Gemini API Key from [Google AI Studio](https://aistudio.google.com/app/apikey).
    - Paste the key into the input box and press `Enter`.

## 🎮 Controls

| Key Binding | Context | Action |
| :--- | :--- | :--- |
| **Ctrl + Space** | Global | Switch focus between **Chat** and **Terminal**. |
| **Ctrl + Q** | Global | Quit the application. |
| **Ctrl + R** | Global | Reset configuration (delete API Key and restart setup). |
| **Enter** | Chat Input | Send message to AI. |
| **Ctrl + Down** | Chat Input | Focus the **Suggestions Panel**. |
| **Up / Down** | Suggestions | Select a command. |
| **Enter** | Suggestions | **Execute** the selected command in the terminal. |
| **PgUp / PgDn** | Chat | Scroll the chat history manually. |
| **Home** | Chat | Scroll to the top of the chat. |

## 🏗️ Architecture

- **Frontend**: Built with `ratatui` and `crossterm` for a robust TUI experience.
- **Async Runtime**: `tokio` handles non-blocking I/O for smooth UI performance.
- **Terminal Emulation**: `portable-pty` and `vt100` provide accurate terminal rendering and state management.
- **AI Client**: `reqwest` manages secure communication with the Gemini API.

## 📄 License

MIT
