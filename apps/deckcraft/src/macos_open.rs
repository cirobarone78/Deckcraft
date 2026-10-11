//! Receives open-document requests from the macOS bundle launcher over standard input.

use std::io::BufRead;
use std::sync::mpsc::{self, Receiver};

pub fn start(ctx: egui::Context) -> Option<Receiver<String>> {
    if std::env::var_os("DECKCRAFT_OPEN_STDIN").is_none() {
        return None;
    }
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            let line = match line {
                Ok(line) => line,
                Err(error) => {
                    log::warn!("macOS document input: {error}");
                    break;
                }
            };
            let paths: Vec<String> = match serde_json::from_str(&line) {
                Ok(paths) => paths,
                Err(error) => {
                    log::warn!("macOS document input: {error}");
                    continue;
                }
            };
            for path in paths {
                let supported = std::path::Path::new(&path)
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| matches!(extension.to_ascii_lowercase().as_str(), "deckcraft" | "pptx" | "potx" | "ppsx"));
                if supported && tx.send(path).is_err() {
                    return;
                }
            }
            ctx.request_repaint();
        }
    });
    Some(rx)
}
