use anyhow::{bail, Result};
use clap::{Parser, Subcommand};
use zbus::Connection;

mod proxy;
use proxy::HeadsetProxy;

#[derive(Parser)]
#[command(name = "blackshark-ctl", about = "Control the Razer BlackShark V3 Pro headset")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Set sidetone level (0–15)
    Sidetone {
        #[arg(value_name = "LEVEL", value_parser = clap::value_parser!(u8).range(0..=15))]
        level: u8,
    },
    /// Query battery level
    Battery,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let conn = Connection::session().await?;
    let proxy = HeadsetProxy::new(&conn).await?;

    if !proxy.connected().await? {
        bail!("headset is not connected (is blacksharkd running?)");
    }

    match cli.command {
        Command::Sidetone { level } => {
            proxy.set_sidetone(level).await?;
            println!("sidetone set to {level}");
        }
        Command::Battery => {
            let (pct, charging) = proxy.get_battery().await?;
            let charging = if charging { " (charging)" } else { "" };
            println!("battery: {pct}%{charging}");
        }
    }

    Ok(())
}
