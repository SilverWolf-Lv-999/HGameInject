use std::path::PathBuf;

use hgameinject::config::client::{ClientConfig, config_directory};
use hgameinject::web::api::Session;
use hgameinject::web::server;

fn main() {
    let arguments: Vec<String> = std::env::args().collect();
    let directory = argument_value(&arguments, "--config-dir")
        .map(PathBuf::from)
        .unwrap_or_else(config_directory);
    if let Err(error) = std::fs::create_dir_all(&directory) {
        eprintln!("cannot create config directory {}: {error}", directory.display());
        std::process::exit(1);
    }
    let mut config = ClientConfig::load(&directory);
    if arguments.iter().any(|argument| argument == "--no-browser") {
        config.autoOpenBrowser = false;
    }
    if arguments.iter().any(|argument| argument == "--scan-all") {
        config.targetDirectories.clear();
    }
    if let Some(port) = argument_value(&arguments, "--port").and_then(|value| value.parse().ok()) {
        config.webPort = port;
    }
    println!("HGameInject {}", env!("CARGO_PKG_VERSION"));
    println!("config directory: {}", directory.display());
    if config.targetDirectories.is_empty() {
        println!("target directories: (all processes with a visible window)");
    } else {
        println!("target directories:");
        for entry in &config.targetDirectories {
            println!("  {entry}");
        }
    }
    let session = Session::new(config);
    if let Err(error) = server::serve(session) {
        eprintln!("web server failed: {error}");
        std::process::exit(1);
    }
}

fn argument_value(arguments: &[String], name: &str) -> Option<String> {
    let position = arguments.iter().position(|argument| argument == name)?;
    arguments.get(position + 1).cloned()
}
