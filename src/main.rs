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
mod project_config;
mod scope;
mod semantic;
mod signature;
mod symbols;
mod workspace;
mod wsymbols;

use backend::Backend;
use tower_lsp::{LspService, Server};

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("alcaparra-lsp {}", env!("CARGO_PKG_VERSION"));
        return;
    }

    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("alcaparra-lsp {}", env!("CARGO_PKG_VERSION"));
        println!("Servidor LSP para AlcaparraLang.");
        println!();
        println!("USO:");
        println!("  alcaparra-lsp            Inicia el servidor LSP (stdin/stdout)");
        println!("  alcaparra-lsp --version  Muestra la versión");
        return;
    }

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(Backend::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}
