/// CodeLens: botón "▶ Ejecutar" sobre el bloque `main` del script.
///
/// El comando `alcaparra.runScript` es manejado por la extensión VS Code,
/// que abre un terminal y ejecuta `caper run <archivo>`.
use alcaparra::lexer::{self, Token};
use tower_lsp::lsp_types::{CodeLens, Command, Position, Range, Url};

pub fn code_lenses(source: &str, uri: &Url) -> Vec<CodeLens> {
    // Solo emite el CodeLens si hay un bloque `main {}`.
    match find_main_line(source) {
        Some(line) => vec![run_lens(line, uri)],
        None       => vec![],
    }
}

fn find_main_line(source: &str) -> Option<u32> {
    let tokens = lexer::tokenize(source).ok()?;
    tokens
        .iter()
        .find(|s| matches!(s.token, Token::Main))
        .map(|s| s.line.saturating_sub(1) as u32)
}

fn run_lens(line: u32, uri: &Url) -> CodeLens {
    let range = Range {
        start: Position { line, character: 0 },
        end:   Position { line, character: 4 }, // longitud de "main"
    };
    CodeLens {
        range,
        command: Some(Command {
            title:     "▶ Ejecutar".to_string(),
            command:   "alcaparra.runScript".to_string(),
            arguments: Some(vec![serde_json::Value::String(uri.to_string())]),
        }),
        data: None,
    }
}
