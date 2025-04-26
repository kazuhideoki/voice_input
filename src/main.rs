use clap::{Parser, Subcommand};
use voice_input::ipc::{IpcCmd, send_cmd};

#[derive(Parser)]
#[command(author, version, about = "Voice Input client (daemon control)")]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    Start {
        #[arg(long, default_value_t = false)]
        paste: bool,
        #[arg(long)]
        prompt: Option<String>,
    },
    Stop,
    Toggle {
        #[arg(long, default_value_t = false)]
        paste: bool,
        #[arg(long)]
        prompt: Option<String>,
    },
    Status,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let resp = match cli.cmd.unwrap_or(Cmd::Toggle {
        paste: false,
        prompt: None,
    }) {
        Cmd::Start { paste, prompt } => send_cmd(&IpcCmd::Start { paste, prompt })?,
        Cmd::Stop => send_cmd(&IpcCmd::Stop)?,
        Cmd::Toggle { paste, prompt } => send_cmd(&IpcCmd::Toggle { paste, prompt })?,
        Cmd::Status => send_cmd(&IpcCmd::Status)?,
    };

    if resp.ok {
        println!("{}", resp.msg);
    } else {
        eprintln!("Error: {}", resp.msg);
    }
    Ok(())
}
