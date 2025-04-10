# GitHub Activity Bot

A Rust-based bot that automatically creates GitHub activity by making changes to a repository, creating pull requests, and merging them. This bot is useful for maintaining an active GitHub profile by automatically generating commits and pull requests.

## Features

-   Automatically creates and modifies files in a repository
-   Creates pull requests with the changes
-   Automatically approves and merges pull requests
-   Configurable schedule using cron expressions
-   Customizable number of files and lines to change
-   Debug mode for detailed logging

## Prerequisites

-   Rust and Cargo installed
-   Git installed
-   A GitHub repository
-   GitHub Personal Access Token with appropriate permissions

## Installation

1. Clone the repository:

```bash
git clone https://github.com/amcinox/github-activity-bot.git
cd github-activity-bot
```

2. Build the project:

```bash
cargo build --release
```

## Configuration

1. Create a `.env` file in the project root with your GitHub token:

```
GITHUB_TOKEN=your_github_token_here
```

2. Configure the bot by editing `config.toml`:

```toml
# GitHub username
username = "your_username"

# Repository details (format: owner/repo)
repo = "owner/repo"

# Local path to the repository
repo_path = "."

# Cron schedule (every 8 hours)
cron_schedule = "0 0 */8 * * *"

# Number of files to change
min_files = 10
max_files = 20

# Number of lines to change per file
min_lines = 100
max_lines = 500

# Debug mode
debug = true
```

## Usage

### Running Once

To run the bot once and exit:

```bash
cargo run -- --run-now
```

### Running as a Service

To run the bot continuously with the configured cron schedule:

```bash
cargo run
```

## Configuration Options

-   `username`: Your GitHub username
-   `repo`: Target repository in format "owner/repo"
-   `repo_path`: Local path to the repository
-   `cron_schedule`: Cron expression for scheduling (e.g., "0 0 _/8 _ \* \*" for every 8 hours)
-   `min_files`/`max_files`: Range of files to modify per run
-   `min_lines`/`max_lines`: Range of lines to modify per file
-   `debug`: Enable/disable debug logging

## Security Note

Never commit your GitHub token to the repository. Always use the `.env` file or environment variables to store sensitive information.

## License

This project is licensed under the MIT License - see the LICENSE file for details.
