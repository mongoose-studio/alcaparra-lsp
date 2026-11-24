mod analysis;
mod util;
mod backend;
mod callhier;
mod catalog;
mod codelens;
mod completion;
mod docblock;
mod formatting;
mod hover;
mod inlay;
mod links;
mod numeric;
mod project_config;
mod scope;
mod semantic;
mod signature;
mod symbols;
mod workspace;
mod wsymbols;

use backend::Backend;
use tower_lsp::{LspService, Server};

/// Imprime los diagnósticos de un archivo y sale.
///
/// El servidor habla LSP por stdin/stdout, así que sin esto la única forma de
/// ver lo que reporta es abrir un editor. Con esto se puede revisar un `.caper`
/// desde la terminal o desde un pipeline, sin cliente de por medio.
fn check_file(path: &str) -> i32 {
    let source = match std::fs::read_to_string(path) {
        Ok(s)  => s,
        Err(e) => { eprintln!("no se pudo leer {path}: {e}"); return 2; }
    };

    let config = tower_lsp::lsp_types::Url::from_file_path(
        std::path::Path::new(path).canonicalize().unwrap_or_else(|_| path.into()),
    )
    .ok()
    .and_then(|uri| project_config::ProjectConfig::find_for_uri(&uri));

    let diags = analysis::analyze(&source, config.as_ref());
    if diags.is_empty() {
        println!("{path}: sin diagnósticos");
        return 0;
    }

    let mut hay_error = false;
    for d in &diags {
        let nivel = match d.severity {
            Some(tower_lsp::lsp_types::DiagnosticSeverity::ERROR) => { hay_error = true; "error" }
            Some(tower_lsp::lsp_types::DiagnosticSeverity::WARNING) => "aviso",
            _ => "nota",
        };
        let codigo = match &d.code {
            Some(tower_lsp::lsp_types::NumberOrString::String(c)) => c.clone(),
            Some(tower_lsp::lsp_types::NumberOrString::Number(n)) => n.to_string(),
            None => "-".to_string(),
        };
        println!(
            "{path}:{}:{}: {nivel} [{codigo}] {}",
            d.range.start.line + 1,
            d.range.start.character + 1,
            d.message,
        );
    }
    println!("\n{} diagnóstico(s)", diags.len());
    if hay_error { 1 } else { 0 }
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("alcaparra-lsp {}", env!("CARGO_PKG_VERSION"));
        return;
    }

    if let Some(i) = args.iter().position(|a| a == "--check") {
        match args.get(i + 1) {
            Some(path) => std::process::exit(check_file(path)),
            None => { eprintln!("--check necesita la ruta de un archivo .caper"); std::process::exit(2); }
        }
    }

    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("alcaparra-lsp {}", env!("CARGO_PKG_VERSION"));
        println!("Servidor LSP para AlcaparraLang.");
        println!();
        println!("USO:");
        println!("  alcaparra-lsp                    Inicia el servidor LSP (stdin/stdout)");
        println!("  alcaparra-lsp --check <archivo>  Imprime los diagnósticos de un .caper y sale");
        println!("  alcaparra-lsp --version          Muestra la versión");
        println!();
        println!("SALIDA DE --check:");
        println!("  0  sin errores (puede haber avisos)");
        println!("  1  hay al menos un error");
        println!("  2  no se pudo leer el archivo");
        return;
    }

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(Backend::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}
