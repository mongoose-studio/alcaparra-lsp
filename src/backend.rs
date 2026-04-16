use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::task::JoinHandle;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use crate::{analysis, callhier, codelens, completion, formatting, hover, inlay, links, semantic, signature, symbols, util, wsymbols};
use crate::project_config::ProjectConfig;
use crate::workspace::Workspace;

const DEBOUNCE_MS: u64 = 300;

pub struct Backend {
    client:    Client,
    workspace: Workspace,
    pending:   Arc<Mutex<HashMap<Url, JoinHandle<()>>>>,
}

impl Backend {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            workspace: Workspace::new(),
            pending: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn schedule_analysis(&self, uri: Url, text: String, version: Option<i32>) {
        if let Some(handle) = self.pending.lock().unwrap().remove(&uri) {
            handle.abort();
        }

        let client    = self.client.clone();
        let uri_clone = uri.clone();
        let config    = ProjectConfig::find_for_uri(&uri);

        let handle = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(DEBOUNCE_MS)).await;

            let diagnostics = tokio::task::spawn_blocking(move || {
                analysis::analyze(&text, config.as_ref())
            })
            .await
            .unwrap_or_default();

            client.publish_diagnostics(uri_clone, diagnostics, version).await;
        });

        self.pending.lock().unwrap().insert(uri, handle);
    }

    async fn analyze_now(&self, uri: Url, text: String, version: i32) {
        let config = ProjectConfig::find_for_uri(&uri);

        let diagnostics = tokio::task::spawn_blocking(move || {
            analysis::analyze(&text, config.as_ref())
        })
        .await
        .unwrap_or_default();

        self.client
            .publish_diagnostics(uri, diagnostics, Some(version))
            .await;
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _params: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![
                        ".".to_string(), // `std.` en imports
                        "@".to_string(), // `@alias` en imports
                        "/".to_string(), // `///` → DocBlock template
                    ]),
                    resolve_provider: Some(false),
                    ..Default::default()
                }),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                signature_help_provider: Some(SignatureHelpOptions {
                    trigger_characters: Some(vec!["(".to_string(), ",".to_string()]),
                    retrigger_characters: Some(vec![",".to_string()]),
                    ..Default::default()
                }),
                definition_provider: Some(OneOf::Left(true)),
                document_highlight_provider: Some(OneOf::Left(true)),
                references_provider: Some(OneOf::Left(true)),
                rename_provider: Some(OneOf::Right(RenameOptions {
                    prepare_provider: Some(true),
                    work_done_progress_options: Default::default(),
                })),
                code_lens_provider: Some(CodeLensOptions {
                    resolve_provider: Some(false),
                }),
                document_symbol_provider: Some(OneOf::Left(true)),
                document_formatting_provider: Some(OneOf::Left(true)),
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(
                        SemanticTokensOptions {
                            legend: semantic::legend(),
                            full: Some(SemanticTokensFullOptions::Bool(true)),
                            ..Default::default()
                        },
                    ),
                ),
                inlay_hint_provider: Some(OneOf::Left(true)),
                document_link_provider: Some(DocumentLinkOptions {
                    resolve_provider: Some(false),
                    work_done_progress_options: Default::default(),
                }),
                workspace_symbol_provider: Some(OneOf::Left(true)),
                call_hierarchy_provider: Some(CallHierarchyServerCapability::Simple(true)),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name:    "alcaparra-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "alcaparra-lsp iniciado")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    // ── Ciclo de vida del documento ──────────────────────────────────────────

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let doc = params.text_document;
        self.workspace.open(doc.uri.clone(), doc.text.clone());
        self.analyze_now(doc.uri, doc.text, doc.version).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri     = params.text_document.uri;
        let version = params.text_document.version;

        if let Some(change) = params.content_changes.into_iter().next() {
            self.workspace.update(&uri, change.text.clone());
            self.schedule_analysis(uri, change.text, Some(version));
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        self.workspace.close(&uri);

        if let Some(handle) = self.pending.lock().unwrap().remove(&uri) {
            handle.abort();
        }

        self.client.publish_diagnostics(uri, vec![], None).await;
    }

    // ── Completions ──────────────────────────────────────────────────────────

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri      = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let source   = self.workspace.get(&uri).unwrap_or_default();
        let config   = ProjectConfig::find_for_uri(&uri);
        let items    = completion::completions(&source, position, config.as_ref());
        Ok(Some(CompletionResponse::Array(items)))
    }

    // ── Code actions (auto-import) ────────────────────────────────────────────

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let uri      = params.text_document.uri;
        let position = params.range.start;
        let source   = self.workspace.get(&uri).unwrap_or_default();

        let word = match hover::word_at_position(&source, position) {
            Some(w) => w.to_string(),
            None    => return Ok(None),
        };

        let insert_line = completion::import_insert_line(&source);
        let config      = ProjectConfig::find_for_uri(&uri);
        let mut actions: Vec<CodeActionOrCommand> = Vec::new();

        // ── DocBlock template ─────────────────────────────────────────────────
        if let Some(action) = docblock_template_action(&source, &uri, position.line) {
            actions.push(action);
        }

        // ── Auto-import (solo si la función no está ya importada) ─────────────
        if !completion::is_already_imported(&source, &word) {
            // Stdlib
            if let Some(module) = completion::find_fn_module(&word) {
                let (range, import_text) =
                    if let Some((merge_range, merged)) = completion::make_merge_edit(&source, module, &word) {
                        (merge_range, merged)
                    } else {
                        let line = insert_line;
                        (Range { start: Position { line, character: 0 }, end: Position { line, character: 0 } },
                         format!("use {}.{{ {} }};\n", module, word))
                    };
                actions.push(make_import_action(
                    format!("Importar `{}` desde {}", word, module),
                    uri.clone(),
                    range,
                    import_text,
                    actions.is_empty(),
                ));
            }

            // Archivos externos (.capercfg paths)
            if let Some(cfg) = config.as_ref() {
                if let Some((use_path, label)) = completion::find_external_fn_import(&word, cfg) {
                    let (range, import_text) =
                        if let Some((merge_range, merged)) = completion::make_merge_edit(&source, &use_path, &word) {
                            (merge_range, merged)
                        } else {
                            let line = insert_line;
                            (Range { start: Position { line, character: 0 }, end: Position { line, character: 0 } },
                             format!("use {}.{{ {} }};\n", use_path, word))
                        };
                    actions.push(make_import_action(
                        format!("Importar `{}` desde {}", word, label),
                        uri.clone(),
                        range,
                        import_text,
                        actions.is_empty(),
                    ));
                }
            }
        }

        if actions.is_empty() {
            Ok(None)
        } else {
            Ok(Some(actions))
        }
    }

    // ── Hover ────────────────────────────────────────────────────────────────

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri      = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        let source   = self.workspace.get(&uri).unwrap_or_default();
        let config   = ProjectConfig::find_for_uri(&uri);
        Ok(hover::hover_at(&source, position, config.as_ref()))
    }

    // ── Signature help ───────────────────────────────────────────────────────

    async fn signature_help(&self, params: SignatureHelpParams) -> Result<Option<SignatureHelp>> {
        let uri      = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        let source   = self.workspace.get(&uri).unwrap_or_default();
        let config   = ProjectConfig::find_for_uri(&uri);
        Ok(signature::signature_help(&source, position, config.as_ref()))
    }

    // ── Ir a definición ──────────────────────────────────────────────────────

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri      = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        let source   = self.workspace.get(&uri).unwrap_or_default();

        let word = match hover::word_at_position(&source, position) {
            Some(w) => w.to_string(),
            None    => return Ok(None),
        };

        let config = ProjectConfig::find_for_uri(&uri);
        let Some(location) = symbols::find_definition(&source, &uri, &word, config.as_ref())
        else { return Ok(None); };

        // Si el cursor ya está EN la declaración, buscar todos los usos en el
        // workspace y retornarlos como destinos de navegación.
        if location.uri == uri && location.range.start.line == position.line {
            let decl_line = location.range.start.line;
            let mut usages: Vec<Location> = Vec::new();

            let uris = self.workspace.all_uris();
            for doc_uri in uris {
                let Some(doc_src) = self.workspace.get(&doc_uri) else { continue };
                for r in symbols::find_references(&doc_src, &doc_uri, &word) {
                    // Excluir la declaración misma
                    if doc_uri == uri && r.range.start.line == decl_line { continue; }
                    usages.push(r);
                }
            }

            return match usages.len() {
                0 => Ok(Some(GotoDefinitionResponse::Scalar(location))), // sin usos → quedarse
                1 => Ok(Some(GotoDefinitionResponse::Scalar(usages.into_iter().next().unwrap()))),
                _ => Ok(Some(GotoDefinitionResponse::Array(usages))),
            };
        }

        Ok(Some(GotoDefinitionResponse::Scalar(location)))
    }

    // ── Document highlight ───────────────────────────────────────────────────

    async fn document_highlight(
        &self,
        params: DocumentHighlightParams,
    ) -> Result<Option<Vec<DocumentHighlight>>> {
        let uri      = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        let source   = self.workspace.get(&uri).unwrap_or_default();

        let word = match hover::word_at_position(&source, position) {
            Some(w) => w.to_string(),
            None    => return Ok(None),
        };

        let refs = symbols::find_references_scoped(&source, &uri, &word, position.line);

        eprintln!(
            "[alcaparra-lsp] doc_highlight '{}' at ({},{}) → {} hits: {:?}",
            word,
            position.line, position.character,
            refs.len(),
            refs.iter().map(|l| (l.range.start.line, l.range.start.character)).collect::<Vec<_>>(),
        );

        if refs.is_empty() { return Ok(None); }

        let highlights = refs.into_iter().map(|loc| DocumentHighlight {
            range: loc.range,
            kind:  Some(DocumentHighlightKind::TEXT),
        }).collect();

        Ok(Some(highlights))
    }

    // ── Prepare rename ───────────────────────────────────────────────────────

    async fn prepare_rename(
        &self,
        params: TextDocumentPositionParams,
    ) -> Result<Option<PrepareRenameResponse>> {
        let uri      = params.text_document.uri;
        let position = params.position;
        let source   = self.workspace.get(&uri).unwrap_or_default();

        let word = match hover::word_at_position(&source, position) {
            Some(w) => w,
            None    => return Ok(None),
        };

        // Rechazamos keywords — no tiene sentido renombrarlos
        const KEYWORDS: &[&str] = &[
            "let", "var", "const", "fn", "emit", "if", "else", "match",
            "while", "foreach", "in", "break", "continue", "try", "catch",
            "throw", "use", "header", "main", "return", "and", "or", "not",
            "true", "false", "null",
        ];
        if KEYWORDS.contains(&word) {
            return Ok(None);
        }

        // Calculamos el rango exacto del identificador
        let line_text   = source.lines().nth(position.line as usize).unwrap_or("");
        let byte_cursor = util::utf16_to_byte_offset(line_text, position.character as usize);
        let before      = &line_text[..byte_cursor];
        let start_byte  = before
            .rfind(|c: char| !c.is_alphanumeric() && c != '_')
            .map(|i| { let ch = line_text[i..].chars().next().unwrap_or('\0'); i + ch.len_utf8() })
            .unwrap_or(0);
        let start_col = line_text[..start_byte].chars().count() as u32;
        let end_col   = start_col + word.chars().count() as u32;

        let range = Range {
            start: Position { line: position.line, character: start_col },
            end:   Position { line: position.line, character: end_col },
        };

        Ok(Some(PrepareRenameResponse::RangeWithPlaceholder {
            range,
            placeholder: word.to_string(),
        }))
    }

    // ── Referencias ─────────────────────────────────────────────────────────

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let uri      = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let source   = self.workspace.get(&uri).unwrap_or_default();

        let word = match hover::word_at_position(&source, position) {
            Some(w) => w.to_string(),
            None    => return Ok(None),
        };

        let refs = symbols::find_references(&source, &uri, &word);
        Ok(if refs.is_empty() { None } else { Some(refs) })
    }

    // ── Rename ───────────────────────────────────────────────────────────────

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let uri      = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let new_name = params.new_name;
        let source   = self.workspace.get(&uri).unwrap_or_default();

        let old_name = match hover::word_at_position(&source, position) {
            Some(w) => w.to_string(),
            None    => return Ok(None),
        };

        let edits = symbols::rename_edits(&source, &old_name, &new_name);
        if edits.is_empty() {
            return Ok(None);
        }

        Ok(Some(WorkspaceEdit {
            changes: Some(std::collections::HashMap::from([(uri, edits)])),
            ..Default::default()
        }))
    }

    // ── Outline del documento ────────────────────────────────────────────────

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let uri    = params.text_document.uri;
        let source = self.workspace.get(&uri).unwrap_or_default();

        let syms: Vec<DocumentSymbol> = symbols::extract(&source)
            .into_iter()
            .map(|s| s.to_document_symbol())
            .collect();

        if syms.is_empty() {
            Ok(None)
        } else {
            Ok(Some(DocumentSymbolResponse::Nested(syms)))
        }
    }

    // ── CodeLens ─────────────────────────────────────────────────────────────

    async fn code_lens(&self, params: CodeLensParams) -> Result<Option<Vec<CodeLens>>> {
        let uri    = params.text_document.uri;
        let source = self.workspace.get(&uri).unwrap_or_default();
        let lenses = codelens::code_lenses(&source, &uri);
        Ok(if lenses.is_empty() { None } else { Some(lenses) })
    }

    // ── Formatting ───────────────────────────────────────────────────────────

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        let uri    = params.text_document.uri;
        let source = self.workspace.get(&uri).unwrap_or_default();
        let config = ProjectConfig::find_for_uri(&uri);
        Ok(formatting::format_document(&source, config.as_ref()))
    }

    // ── Inlay hints ──────────────────────────────────────────────────────────

    async fn inlay_hint(&self, params: InlayHintParams) -> Result<Option<Vec<InlayHint>>> {
        let uri    = params.text_document.uri;
        let source = self.workspace.get(&uri).unwrap_or_default();
        let config = ProjectConfig::find_for_uri(&uri);
        let range  = Some(params.range);
        let hints  = inlay::inlay_hints(&source, range, config.as_ref());
        eprintln!("[inlay_hint] uri={uri} hints={}", hints.len());
        Ok(if hints.is_empty() { None } else { Some(hints) })
    }

    // ── Document links ───────────────────────────────────────────────────────

    async fn document_link(&self, params: DocumentLinkParams) -> Result<Option<Vec<DocumentLink>>> {
        let uri    = params.text_document.uri;
        let source = self.workspace.get(&uri).unwrap_or_default();
        let config = ProjectConfig::find_for_uri(&uri);
        let result = links::document_links(&source, config.as_ref());
        Ok(if result.is_empty() { None } else { Some(result) })
    }

    // ── Workspace symbols ────────────────────────────────────────────────────

    async fn symbol(&self, params: WorkspaceSymbolParams) -> Result<Option<Vec<SymbolInformation>>> {
        // Derivar el root desde cualquier documento abierto
        let root = self.workspace.all_uris()
            .into_iter()
            .find_map(|u| ProjectConfig::find_for_uri(&u))
            .map(|c| c.root);

        let Some(root) = root else { return Ok(None) };
        let results = wsymbols::workspace_symbols(&root, &params.query);
        Ok(if results.is_empty() { None } else { Some(results) })
    }

    // ── Call hierarchy ───────────────────────────────────────────────────────

    async fn prepare_call_hierarchy(
        &self,
        params: CallHierarchyPrepareParams,
    ) -> Result<Option<Vec<CallHierarchyItem>>> {
        let uri      = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        let source   = self.workspace.get(&uri).unwrap_or_default();
        Ok(callhier::prepare(&source, &uri, position))
    }

    async fn incoming_calls(
        &self,
        params: CallHierarchyIncomingCallsParams,
    ) -> Result<Option<Vec<CallHierarchyIncomingCall>>> {
        let uri    = params.item.uri.clone();
        let config = ProjectConfig::find_for_uri(&uri);
        let calls  = callhier::incoming_calls(params, config.as_ref());
        Ok(if calls.is_empty() { None } else { Some(calls) })
    }

    async fn outgoing_calls(
        &self,
        params: CallHierarchyOutgoingCallsParams,
    ) -> Result<Option<Vec<CallHierarchyOutgoingCall>>> {
        let uri    = params.item.uri.clone();
        let source = self.workspace.get(&uri).unwrap_or_default();
        let config = ProjectConfig::find_for_uri(&uri);
        let calls  = callhier::outgoing_calls(params, &source, config.as_ref());
        Ok(if calls.is_empty() { None } else { Some(calls) })
    }

    // ── Semantic tokens ──────────────────────────────────────────────────────

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let uri    = params.text_document.uri;
        let source = self.workspace.get(&uri).unwrap_or_default();
        let ctx    = ProjectConfig::find_for_uri(&uri)
            .map(|c| c.context_keys)
            .unwrap_or_default();

        Ok(semantic::semantic_tokens(&source, &ctx).map(|tokens| {
            SemanticTokensResult::Tokens(SemanticTokens {
                result_id: None,
                data: tokens,
            })
        }))
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Si la línea `fn_line` es una declaración `fn` sin DocBlock encima, genera
/// una code action que inserta el esqueleto `///` con @param y @returns.
fn docblock_template_action(source: &str, uri: &Url, fn_line: u32) -> Option<CodeActionOrCommand> {
    let line_text = source.lines().nth(fn_line as usize)?;
    let trimmed   = line_text.trim();

    // Debe ser una línea de declaración de función
    let rest = trimmed.strip_prefix("fn ")?;
    let paren = rest.find('(')?;
    let fn_name = rest[..paren].trim();
    if fn_name.is_empty() { return None; }

    // Si la línea anterior ya tiene ///, no ofrecer
    if fn_line > 0 {
        let prev = source.lines().nth(fn_line as usize - 1).unwrap_or("").trim();
        if prev.starts_with("///") { return None; }
    }

    // Extraer parámetros
    let close = rest.find(')')?;
    let params_str = &rest[paren + 1..close];
    let params: Vec<&str> = params_str
        .split(',')
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .collect();

    // Indentación de la línea fn
    let indent = &line_text[..line_text.len() - line_text.trim_start().len()];

    let mut insert = format!("{}/// Descripción.\n", indent);
    for p in &params {
        insert.push_str(&format!("{}/// @param {:<14} Descripción\n", indent, p));
    }
    insert.push_str(&format!("{}/// @returns        Descripción\n", indent));

    let edit = TextEdit {
        range: Range {
            start: Position { line: fn_line, character: 0 },
            end:   Position { line: fn_line, character: 0 },
        },
        new_text: insert,
    };

    Some(CodeActionOrCommand::CodeAction(CodeAction {
        title: format!("Agregar DocBlock a `{}`", fn_name),
        kind:  Some(CodeActionKind::REFACTOR),
        edit:  Some(WorkspaceEdit {
            changes: Some(std::collections::HashMap::from([(uri.clone(), vec![edit])])),
            ..Default::default()
        }),
        ..Default::default()
    }))
}

fn make_import_action(
    title:       String,
    uri:         Url,
    range:       Range,
    import_text: String,
    preferred:   bool,
) -> CodeActionOrCommand {
    let edit = TextEdit {
        range,
        new_text: import_text,
    };
    CodeActionOrCommand::CodeAction(CodeAction {
        title,
        kind:         Some(CodeActionKind::QUICKFIX),
        edit:         Some(WorkspaceEdit {
            changes: Some(std::collections::HashMap::from([(uri, vec![edit])])),
            ..Default::default()
        }),
        is_preferred: Some(preferred),
        ..Default::default()
    })
}
