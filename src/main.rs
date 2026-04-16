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
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(Backend::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}
