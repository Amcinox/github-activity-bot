use chrono::Utc;
use clap::Parser;
use git2::Repository;
use octocrab::{Octocrab, models::pulls::PullRequest, params::pulls::MergeMethod};
use rand::{Rng, seq::SliceRandom};
use serde::{Serialize, Deserialize};
use std::{fs, path::Path, process::Command, time::Duration};
use tokio::time;
use tokio_cron_scheduler::{Job, JobScheduler};
use dotenv;

#[derive(Parser, Debug)]
#[clap(author, version, about = "Bot to automatically create GitHub activity")]
struct Args {
    /// Path to the config file
    #[clap(short, long, default_value = "config.toml")]
    config: String,

    /// Run the bot immediately once and exit
    #[clap(long)]
    run_now: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct Config {
    /// GitHub username
    username: String,
    /// Repository name (format: owner/repo)
    repo: String,
    /// Local path to the repository
    repo_path: String,
    /// Cron schedule (e.g., "0 */8 * * *" for every 8 hours)
    cron_schedule: String,
    /// Minimum number of files to change
    min_files: usize,
    /// Maximum number of files to change
    max_files: usize,
    /// Minimum number of lines to change per file
    min_lines: usize,
    /// Maximum number of lines to change per file
    max_lines: usize,
    /// Whether to print debug information
    debug: bool,
}

#[derive(Clone)]
struct GitHubBot {
    config: Config,
    octocrab: Octocrab,
    repo_owner: String,
    repo_name: String,
}

impl GitHubBot {
    async fn new(config: Config) -> Result<Self, Box<dyn std::error::Error>> {
        // Get token from environment variable
        let token = std::env::var("GITHUB_TOKEN")
            .map_err(|_| "GITHUB_TOKEN environment variable not set")?;

        let octocrab = Octocrab::builder()
            .personal_token(token)
            .build()?;

        let repo_parts: Vec<&str> = config.repo.split('/').collect();
        if repo_parts.len() != 2 {
            return Err("Repository should be in the format 'owner/repo'".into());
        }

        Ok(Self {
            config: config.clone(),
            octocrab,
            repo_owner: repo_parts[0].to_string(),
            repo_name: repo_parts[1].to_string(),
        })
    }

    async fn run_once(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("Starting bot run at {}", Utc::now());
        
        // Step 1: Make local changes
        let branch_name = self.make_changes()?;
        
        // Step 2: Push changes and create PR
        let pr = self.create_pull_request(&branch_name).await?;
        
        // Step 3: Wait a bit to make it look natural
        let wait_time = rand::thread_rng().gen_range(60..180);
        println!("Waiting {} seconds before approving PR...", wait_time);
        time::sleep(Duration::from_secs(wait_time)).await;
        
        // Step 4: Approve and merge the PR
        self.approve_and_merge_pr(pr.number).await?;
        
        println!("Bot run completed successfully at {}", Utc::now());
        Ok(())
    }

    fn make_changes(&self) -> Result<String, Box<dyn std::error::Error>> {
        // Ensure we're on the main branch and pull latest changes
        let repo = Repository::open(&self.config.repo_path)?;
        
        // Checkout main/master branch
        let main_branch = if repo.find_branch("main", git2::BranchType::Local).is_ok() {
            "main"
        } else if repo.find_branch("master", git2::BranchType::Local).is_ok() {
            "master"
        } else {
            return Err("Neither 'main' nor 'master' branch found".into());
        };
        
        if self.config.debug {
            println!("Using {} branch as base", main_branch);
        }
        
        // Run git commands with system process for simplicity
        self.run_git_command(&["checkout", main_branch])?;
        self.run_git_command(&["pull", "origin", main_branch])?;
        
        // Create a new branch with timestamp
        let timestamp = Utc::now().timestamp();
        let branch_name = format!("bot-update-{}", timestamp);
        self.run_git_command(&["checkout", "-b", &branch_name])?;
        
        // Get all files in the repository (excluding .git directory)
        let files = self.get_repository_files()?;
        if self.config.debug {
            println!("Found {} files in repository", files.len());
        }
        
        // Choose random files to modify
        let mut rng = rand::thread_rng();
        let num_files_to_change = rng.gen_range(self.config.min_files..=self.config.max_files);
        let files_to_change = files.choose_multiple(&mut rng, num_files_to_change).collect::<Vec<_>>();
        
        if self.config.debug {
            println!("Will modify {} files", files_to_change.len());
        }
        
        // Modify each selected file
        for file_path in files_to_change {
            self.modify_file(file_path)?;
        }
        
        // Commit changes
        let commit_message = format!("Update files {}", Utc::now().format("%Y-%m-%d %H:%M:%S"));
        self.run_git_command(&["add", "."])?;
        self.run_git_command(&["commit", "-m", &commit_message])?;
        
        // Push the branch
        self.run_git_command(&["push", "--set-upstream", "origin", &branch_name])?;
        
        Ok(branch_name)
    }

    fn get_repository_files(&self) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let mut result = Vec::new();
        self.collect_files(Path::new(&self.config.repo_path), &mut result)?;
        
        // If no files found, create some default files
        if result.is_empty() {
            if self.config.debug {
                println!("No files found, creating default files");
            }
            
            // Create a sample Rust file
            let rust_file = Path::new(&self.config.repo_path).join("src").join("lib.rs");
            fs::create_dir_all(rust_file.parent().unwrap())?;
            fs::write(&rust_file, "// Sample Rust library\npub fn hello() {\n    println!(\"Hello, world!\");\n}\n")?;
            
            // Create a README if it doesn't exist
            let readme_file = Path::new(&self.config.repo_path).join("README.md");
            if !readme_file.exists() {
                fs::write(&readme_file, "# GitHub Activity Bot\n\nThis repository is managed by a bot that creates activity.\n")?;
            }
            
            // Add the new files to git
            self.run_git_command(&["add", "."])?;
            self.run_git_command(&["commit", "-m", "Add initial files"])?;
            self.run_git_command(&["push", "origin", "main"])?;
            
            // Refresh the file list
            result.clear();
            self.collect_files(Path::new(&self.config.repo_path), &mut result)?;
        }
        
        Ok(result)
    }

    fn collect_files(&self, dir: &Path, result: &mut Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
        if dir.ends_with(".git") {
            return Ok(());
        }
        
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            
            if path.is_dir() {
                self.collect_files(&path, result)?;
            } else {
                // Skip binary files and only include certain text file extensions
                if let Some(ext) = path.extension() {
                    let ext = ext.to_string_lossy().to_lowercase();
                    if ["rs", "txt", "md", "toml", "json", "yaml", "yml"].contains(&ext.as_str()) {
                        if let Ok(relative_path) = path.strip_prefix(&self.config.repo_path) {
                            result.push(relative_path.to_string_lossy().to_string());
                        }
                    }
                }
            }
        }
        
        Ok(())
    }

    fn modify_file(&self, file_path: &str) -> Result<(), Box<dyn std::error::Error>> {
        let full_path = format!("{}/{}", self.config.repo_path, file_path);
        
        // Read file content
        let content = fs::read_to_string(&full_path)?;
        let lines: Vec<&str> = content.lines().collect();
        
        if lines.is_empty() {
            return Ok(());
        }
        
        let mut rng = rand::thread_rng();
        let num_lines_to_change = rng.gen_range(self.config.min_lines..=self.config.max_lines.min(lines.len()));
        
        if self.config.debug {
            println!("Modifying {} lines in file {}", num_lines_to_change, file_path);
        }
        
        // Modify random lines
        let mut modified_lines = lines.iter().map(|&s| s.to_string()).collect::<Vec<_>>();
        for _ in 0..num_lines_to_change {
            let line_idx = rng.gen_range(0..lines.len());
            let line = &modified_lines[line_idx];
            
            // Different modification strategies
            let strategy = rng.gen_range(0..4);
            match strategy {
                0 => {
                    // Add a comment
                    if line.trim().is_empty() {
                        modified_lines[line_idx] = format!("// Bot update: {}", Utc::now());
                    } else if !line.trim_start().starts_with("//") {
                        modified_lines[line_idx] = format!("{} // Bot update: {}", line, Utc::now());
                    }
                }
                1 => {
                    // Add an empty line after this line
                    if line_idx < modified_lines.len() - 1 {
                        modified_lines.insert(line_idx + 1, String::new());
                    }
                }
                2 => {
                    // Modify whitespace
                    if !line.trim().is_empty() {
                        let trimmed = line.trim();
                        let indent = " ".repeat(rng.gen_range(0..4) * 4);
                        modified_lines[line_idx] = format!("{}{}", indent, trimmed);
                    }
                }
                _ => {
                    // Add or replace a TODO comment
                    modified_lines[line_idx] = format!("// TODO: Update this line - bot modification {}", Utc::now());
                }
            }
        }
        
        // Write back the modified content
        let new_content = modified_lines.join("\n");
        fs::write(full_path, new_content)?;
        
        Ok(())
    }

    async fn create_pull_request(&self, branch_name: &str) -> Result<PullRequest, Box<dyn std::error::Error>> {
        let title = format!("Bot update {}", Utc::now().format("%Y-%m-%d %H:%M:%S"));
        let body = format!(
            "This is an automated PR created by the activity bot.\n\nTimestamp: {}",
            Utc::now()
        );
        
        println!("Creating PR: {} from {} to main", title, branch_name);
        
        let pr = self.octocrab
            .pulls(&self.repo_owner, &self.repo_name)
            .create(&title, branch_name, "main")
            .body(&body)
            .send()
            .await?;
            
        println!("Created PR #{}: {:?}", pr.number, pr.html_url);
        
        Ok(pr)
    }

    async fn approve_and_merge_pr(&self, pr_number: u64) -> Result<(), Box<dyn std::error::Error>> {
        // Skip review approval for now since the API is not working as expected
        println!("Skipping PR review approval for PR #{}", pr_number);
        
        // Wait a moment before merging
        time::sleep(Duration::from_secs(30)).await;
        
        // Merge the PR
        let _ = self.octocrab
            .pulls(&self.repo_owner, &self.repo_name)
            .merge(pr_number)
            .method(MergeMethod::Squash)
            .title(format!("Merged bot update PR #{}", pr_number))
            .send()
            .await?;
            
        println!("Merged PR #{}", pr_number);
        
        Ok(())
    }

    fn run_git_command(&self, args: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
        let output = Command::new("git")
            .current_dir(&self.config.repo_path)
            .args(args)
            .output()?;
            
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if self.config.debug {
                println!("Git command failed: git {}", args.join(" "));
                println!("Error: {}", stderr);
            }
            return Err(format!("Git command failed: {}", stderr).into());
        }
        
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load environment variables from .env file
    dotenv::dotenv().ok();
    
    let args = Args::parse();
    
    // Load config
    let config_str = fs::read_to_string(&args.config)?;
    let config: Config = toml::from_str(&config_str)?;
    
    println!("Starting GitHub Activity Bot with config: {:?}", config);
    
    let bot = GitHubBot::new(config).await?;

    if args.run_now {
        println!("Running bot once immediately...");
        if let Err(e) = bot.run_once().await {
            eprintln!("Error in bot run: {}", e);
            return Err(e);
        }
        println!("Bot run completed successfully");
        return Ok(());
    }
    
    let bot_clone = bot.clone();
    let cron_schedule = bot_clone.config.cron_schedule.clone();
    
    // Set up scheduler
    let scheduler = JobScheduler::new().await?;
    
    // Add job based on cron schedule
    scheduler.add(
        Job::new_async(&*cron_schedule, move |_, _| {
            let bot_clone = bot_clone.clone();
            Box::pin(async move {
                if let Err(e) = bot_clone.run_once().await {
                    eprintln!("Error in bot run: {}", e);
                }
            })
        })?
    ).await?;
    
    // Start the scheduler
    scheduler.start().await?;
    
    println!("Bot started and will run on schedule: {}", cron_schedule);
    println!("Press Ctrl+C to stop");
    
    // Keep the program running
    loop {
        time::sleep(Duration::from_secs(60)).await;
    }
}

// Add this to your Cargo.toml:
//
// [dependencies]
// tokio = { version = "1", features = ["full"] }
// octocrab = "0.18"
// git2 = "0.15"
// chrono = "0.4"
// rand = "0.8"
// clap = { version = "3.2", features = ["derive"] }
// serde = { version = "1.0", features = ["derive"] }
// toml = "0.5"
// tokio-cron-scheduler = "0.9"