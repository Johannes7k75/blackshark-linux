mod battery;
mod device;
mod protocol;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "blackshark-ctl", about = "Control the Razer BlackShark V3 Pro headset")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Set sidetone level (0–15)
    ///
    /// Controls how much of your own voice you hear in the headset.
    /// 0 = off, 15 = maximum.
    Sidetone {
        #[arg(value_name = "LEVEL", value_parser = clap::value_parser!(u8).range(0..=15))]
        level: u8,
    },
    /// Query battery level (command bytes not yet confirmed — pending capture)
    Battery,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Sidetone { level } => {
            let dev = device::open()?;
            cmd_sidetone(&dev, level)
        }
        Command::Battery => {
            let dev = device::open()?;
            cmd_battery(&dev)
        }
    }
}

fn cmd_sidetone(dev: &hidapi::HidDevice, level: u8) -> Result<()> {
    use protocol::cmd;
    use protocol::Report;

    // GET current value first (mirrors what Synapse does).
    let get = Report::new(0x60, cmd::SIDETONE_GET_CLASS, cmd::SIDETONE_ID, &[cmd::SIDETONE_GET_ARG]);
    device::send(dev, &get)?;

    // SET new value.
    let set = Report::new(0x60, cmd::SIDETONE_SET_CLASS, cmd::SIDETONE_ID, &[level]);
    device::send(dev, &set)?;

    println!("sidetone set to {level}");
    Ok(())
}

fn cmd_battery(dev: &hidapi::HidDevice) -> Result<()> {
    let state = battery::query(dev)?;
    let charging = if state.charging { " (charging)" } else { "" };
    println!("battery: {:.0}%{charging}", state.percentage);
    Ok(())
}
