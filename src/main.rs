use clap::{Parser};
use tracing_subscriber::EnvFilter;
use client_side::*;

fn main() {
    let args = shell::Args::parse();

    let log_config = format!("eframe=off,client_side={}", args.log_level);

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new(log_config))
        .init();

    if let Err(e) = eframe::run_native(
        "ClientSide",
        eframe::NativeOptions::default(),
        Box::new(|_cc| Ok(Box::new(shell::App::new()))),
    ) {
        tracing::error!("{}", e);
    }
}