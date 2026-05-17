> [!WARNING]
   > **This repository is deprecated and no longer maintained.**
   > Development has moved to [filip-cudny/promptheus2](https://github.com/filip-cudny/promptheus2).
   > Please use the new repository for issues, PRs, and the latest version.

# Promptheus

A powerful context menu application that provides instant access to AI prompts from anywhere in your system. Execute common tasks like text summarization, translation, and custom prompts using clipboard content or speech-to-text input.

## Features

- **Global Context Menu**: Access prompts from any application using keyboard shortcuts
- **Clipboard Integration**: Automatically uses clipboard content as prompt input
- **Speech-to-Text**: Dual functionality - alternative prompt input mode (hold Shift) or standalone dictation tool for clipboard content
- **Active Prompt**: Set a frequently used prompt for direct hotkey execution without opening the context menu
- **Cross-Platform**: Supports macOS, Linux, and Windows
- **Customizable**: Configure prompts, models, and key bindings
- **Background Service**: Runs silently in the background

## Installation

### Prerequisites

- Python 3.11 or higher
- Operating System: macOS, Linux, or Windows

#### Ubuntu/Debian

```bash
sudo apt install libxcb-cursor0
```

### Setup

1. Clone the repository:

```bash
git clone <repository-url>
cd promptheus
```

2. Install and setup using Make:

```bash
make install
```

This will:

- Create a virtual environment
- Install dependencies (with platform-specific extras)
- Create `.env` file (add your API keys)
- Copy example settings to `settings/` directory

3. Configure your API keys in `.env`:

```bash
OPENAI_API_KEY=your_api_key_here
```

4. Start the service:

```bash
make start
```

## Usage

### Basic Operations

- **Open Context Menu**: Use the configured shortcut (default: Cmd+F1 on macOS, Ctrl+F1 on Linux/Windows)
- **Execute Active Prompt**: Use the configured shortcut (default: Cmd+F2 on macOS, Ctrl+F2 on Linux/Windows)
- **Speech-to-Text Toggle**: Use Shift+F1 to toggle speech input mode

### Input Modes

1. **Clipboard Mode** (default): Content from system clipboard is used as prompt input
2. **Speech Mode**: Hold Shift while selecting a prompt to use voice transcription as input

### Speech-to-Text Usage

Speech-to-text functionality works in two ways:

1. **Alternative Prompt Input**: Hold Shift when selecting any prompt to replace clipboard content with voice transcription
2. **Standalone Dictation**: Use speech-to-text as a separate action to dictate text, review it, and copy the transcription to clipboard for use in any application

### Context Menu Options

- Select and execute any configured prompt
- Set active prompt (for quick hotkey execution without opening menu)
- Copy last input/output of prompt execution
- Use speech-to-text as standalone dictation tool
- Copy last transcription output (from standalone dictation)
- Switch between models/providers

## Default Key Bindings

### macOS

- `Cmd+F1` / `Cmd+F3`: Open context menu
- `Cmd+F2`: Execute active prompt
- `Shift+F1`: Speech-to-text toggle

### Linux/Windows

- `Ctrl+F1`: Open context menu
- `Ctrl+F2`: Execute active prompt
- `Shift+F1`: Speech-to-text toggle

## Configuration

### Settings Structure

The main configuration is stored in `settings/settings.json`. Key sections include:

#### Models Configuration

```json
{
  "default_model": "gpt-4.1-model",
  "models": {
    "gpt-4.1-model": {
      "model": "gpt-4.1",
      "display_name": "gpt-4.1",
      "temperature": 0.3,
      "api_key_env": "OPENAI_API_KEY",
      "base_url": "https://api.openai.com/v1"
    }
  }
}
```

#### Speech-to-Text Configuration

```json
{
  "speech_to_text_model": {
    "model": "gpt-4o-transcribe",
    "display_name": "gpt-4o-transcribe",
    "api_key_env": "OPENAI_API_KEY",
    "base_url": "https://api.openai.com/v1"
  }
}
```

#### Prompts Configuration

```json
{
  "prompts": [
    {
      "id": "unique-id",
      "name": "Helpful assistant",
      // optional model - to assign model value is key of "models"
      // when model is provided - prompt always uses selected model
      // otherwise prompt uses model currently selected in settings (default model by default)
      "model": "gpt-4.1-model",
      "messages": [
        {
          "role": "system",
          "content": "You are a helpful assistant."
        },
        {
          "role": "user",
          "content": "{{clipboard}}"
        }
      ]
    }
  ]
}
```

#### Key Bindings

Supported operating systems and available actions:

**Supported OS:**

- `macos`
- `linux`
- `windows`

**Available Actions:**

- `open_context_menu` - Opens the context menu with available prompts
- `execute_active_prompt` - Executes the currently set active prompt
- `speech_to_text_toggle` - Toggles speech-to-text input mode

**Configuration Example:**

```json
{
  "keymaps": [
    {
      "context": "os == macos",
      "bindings": {
        "cmd+f1": "open_context_menu",
        "cmd+f3": "open_context_menu",
        "cmd+f2": "execute_active_prompt",
        "shift+f1": "speech_to_text_toggle"
      }
    },
    {
      "context": "os == linux",
      "bindings": {
        "ctrl+f1": "open_context_menu",
        "ctrl+f2": "execute_active_prompt",
        "shift+f1": "speech_to_text_toggle"
      }
    },
    {
      "context": "os == windows",
      "bindings": {
        "ctrl+f1": "open_context_menu",
        "ctrl+f2": "execute_active_prompt",
        "shift+f1": "speech_to_text_toggle"
      }
    }
  ]
}
```

### Model Switching

You can switch between different AI models and providers from the context menu. Configure multiple models in your settings and easily switch between them during use.

### Active Prompt

Set any prompt as "active" from the context menu to enable quick execution via hotkey (Cmd+F2/Ctrl+F2) without opening the menu. This is perfect for frequently used prompts like translation or summarization.

### Prompt Templates

Prompts support template variables:

- `{{clipboard}}`: Current clipboard content

You can also reference external files:

```json
{
  "role": "system",
  "file": "prompts/translate_english.md"
}
```

## Service Management

### Start/Stop Service

```bash
make start    # Start in background
make stop     # Stop service
make restart  # Restart service
make status   # Check status
```

### View Logs

```bash
make logs         # Show recent logs
make logs-follow  # Follow logs in real-time
```

## Example Prompts

### Text Summarization

```json
{
  "name": "Summarize",
  "messages": [
    {
      "role": "system",
      "content": "Summarize the following text in 2-3 sentences."
    },
    {
      "role": "user",
      "content": "{{clipboard}}"
    }
  ]
}
```

### Translation

```json
{
  "name": "Translate to English",
  "messages": [
    {
      "role": "system",
      "content": "Translate the following text to English."
    },
    {
      "role": "user",
      "content": "{{clipboard}}"
    }
  ]
}
```

## Troubleshooting

### Service Not Starting

1. Check if port is already in use
2. Verify API keys in `.env`
3. Check logs: `make logs`

### Key Bindings Not Working

1. Ensure service is running: `make status`
2. Check for conflicting system shortcuts
3. Verify settings.json syntax

### Speech-to-Text Issues

1. Verify microphone permissions
2. Check speech-to-text model configuration
3. Ensure audio drivers are working
4. Check microphone permissions for your application
5. Ensure your system has working audio drivers

## Development

### Project Structure

```
promptheus/
├── main.py                 # Main application entry
├── pyproject.toml          # Project dependencies
├── settings/              # Configuration directory
│   ├── prompts/           # External prompt files
│   └── settings.json      # Main settings file
├── Makefile              # Build and service management
└── .env                  # Environment variables
```

### Clean Up

```bash
make clean      # Remove logs and temporary files
make clean-all  # Remove virtual environment
```
