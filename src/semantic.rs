use alcaparra::lexer::{tokenize_with_comments, FmtSpanned, Token};
use tower_lsp::lsp_types::{SemanticToken, SemanticTokenModifier, SemanticTokenType, SemanticTokensLegend};

// ── Índices en la leyenda ────────────────────────────────────────────────────

const T_KEYWORD:     u32 = 0;
const T_STRING:      u32 = 1;
const T_NUMBER:      u32 = 2;
const T_VARIABLE:    u32 = 3;
const T_FUNCTION:    u32 = 4;
const T_COMMENT:     u32 = 5;
const T_OPERATOR:    u32 = 6;
const T_TYPE:        u32 = 7; // true, false, null
const T_ENUM_MEMBER: u32 = 8; // constantes (header const + uso de ellas)
const T_PROPERTY:    u32 = 9; // keys de objetos (Ident seguido de `:`)
const T_NAMESPACE:   u32 = 10; // segmentos de ruta en `use` (std, @alias, módulo)
const T_REGEXP:      u32 = 11; // patrones regex (2º argumento de regex_*)

// Modificadores (bitmask)
const MOD_MUTABLE:  u32 = 1; // variables declaradas con `var`  (bit 0)
const MOD_DOC:      u32 = 2; // comentarios DocBlock (`///`)     (bit 1)
const MOD_CONTEXT:  u32 = 4; // variables inyectadas del .capercfg (bit 2)

/// Leyenda que se declara en `initialize`. El orden debe coincidir con las
/// constantes T_* y MOD_* de arriba.
pub fn legend() -> SemanticTokensLegend {
    SemanticTokensLegend {
        token_types: vec![
            SemanticTokenType::KEYWORD,     // 0
            SemanticTokenType::STRING,      // 1
            SemanticTokenType::NUMBER,      // 2
            SemanticTokenType::VARIABLE,    // 3
            SemanticTokenType::FUNCTION,    // 4
            SemanticTokenType::COMMENT,     // 5
            SemanticTokenType::OPERATOR,    // 6
            SemanticTokenType::TYPE,        // 7
            SemanticTokenType::ENUM_MEMBER, // 8 — constantes
            SemanticTokenType::PROPERTY,    // 9 — keys de objetos
            SemanticTokenType::NAMESPACE,   // 10 — segmentos de ruta en `use`
            SemanticTokenType::REGEXP,      // 11 — patrones regex
        ],
        token_modifiers: vec![
            SemanticTokenModifier::new("mutable"),          // 0 → bit 1
            SemanticTokenModifier::new("documentation"),    // 1 → bit 2
            SemanticTokenModifier::new("contextVariable"),  // 2 → bit 4
        ],
    }
}

// ── Punto de entrada principal ────────────────────────────────────────────────

/// Genera la lista de semantic tokens para el documento completo.
/// Retorna `None` si el fuente tiene errores de lex (el highlighting básico
/// del TextMate grammar sigue activo).
pub fn semantic_tokens(source: &str, context_keys: &[String]) -> Option<Vec<SemanticToken>> {
    let fmt_tokens = tokenize_with_comments(source).ok()?;
    let lines: Vec<&str> = source.lines().collect();

    // Pre-colectar constantes y vars para clasificarlas correctamente
    let (const_names, var_names) = collect_decl_names(&fmt_tokens);
    let ctx_set:      std::collections::HashSet<&str> = context_keys.iter().map(|s| s.as_str()).collect();
    let use_roles:    Vec<Option<UseRole>>            = build_use_roles(&fmt_tokens);
    let regex_pos:    std::collections::HashSet<usize> = build_regex_positions(&fmt_tokens);

    let mut raw: Vec<RawToken> = Vec::new();

    for (ti, spanned) in fmt_tokens.iter().enumerate() {
        match spanned {
            FmtSpanned::Token(s) => {
                let src_line = lines.get(s.line.saturating_sub(1)).copied().unwrap_or("");

                // Tokens dentro de sentencias `use` tienen prioridad
                if let Token::Ident(name) = &s.token {
                    if let Some(role) = use_roles[ti] {
                        let tt = match role {
                            UseRole::Namespace    => T_NAMESPACE,
                            UseRole::ImportedName => T_FUNCTION,
                        };
                        raw.push(RawToken {
                            line: (s.line.saturating_sub(1)) as u32,
                            col:  (s.col.saturating_sub(1)) as u32,
                            len:  name.len() as u32,
                            tt,
                            mods: 0,
                        });
                        continue;
                    }
                }

                // Patrón regex: 2º argumento de regex_*
                if let Token::StringLit(sv) = &s.token {
                    if regex_pos.contains(&ti) {
                        let src_line = lines.get(s.line.saturating_sub(1)).copied().unwrap_or("");
                        let start    = s.col.saturating_sub(1);
                        let rest     = src_line.get(start..).unwrap_or("");
                        let len = if let Some(q) = rest.chars().next() {
                            rest[1..].find(q).map(|i| i + 2).unwrap_or(sv.len() + 2)
                        } else {
                            sv.len() + 2
                        };
                        raw.push(RawToken {
                            line: (s.line.saturating_sub(1)) as u32,
                            col:  start as u32,
                            len:  len as u32,
                            tt:   T_REGEXP,
                            mods: 0,
                        });
                        continue;
                    }
                }

                // Detecta keys de objeto: Ident seguido de Token::Colon
                if let Token::Ident(name) = &s.token {
                    let next_is_colon = fmt_tokens[ti + 1..].iter()
                        .find_map(|sp| if let FmtSpanned::Token(t) = sp { Some(t) } else { None })
                        .map(|t| matches!(t.token, Token::Colon))
                        .unwrap_or(false);

                    if next_is_colon {
                        raw.push(RawToken {
                            line: (s.line.saturating_sub(1)) as u32,
                            col:  (s.col.saturating_sub(1)) as u32,
                            len:  name.len() as u32,
                            tt:   T_PROPERTY,
                            mods: 0,
                        });
                        continue;
                    }
                }

                if let Some((tt, len, mods)) = classify(&s.token, src_line, s.col, &const_names, &var_names, &ctx_set) {
                    raw.push(RawToken {
                        line: (s.line.saturating_sub(1)) as u32,
                        col:  (s.col.saturating_sub(1)) as u32,
                        len,
                        tt,
                        mods,
                    });
                }
            }
            FmtSpanned::LineComment { text, line } => {
                let src_line = lines.get(line.saturating_sub(1)).copied().unwrap_or("");
                // Buscamos `//` en la línea para obtener la columna real
                if let Some(col) = src_line.find("//") {
                    // `///` DocBlock: text empieza con `/` (el tercer slash)
                    let is_doc = text.starts_with('/');
                    let lnum   = (line.saturating_sub(1)) as u32;

                    if is_doc {
                        // Dividir la línea en segmentos no solapantes:
                        //   `///`       → T_COMMENT + MOD_DOC (prefijo, 3 chars)
                        //   texto libre → T_COMMENT + MOD_DOC
                        //   @tag        → T_KEYWORD + MOD_DOC  (negrita)
                        //
                        // text[1..] = contenido tras `///` (el `text` del lexer ya no incluye `//`)
                        let rest = &text[1..];
                        let base_col = col + 3; // columna del primer char tras `///`

                        // Primero el prefijo `///`
                        raw.push(RawToken { line: lnum, col: col as u32, len: 3, tt: T_COMMENT, mods: MOD_DOC });

                        // Luego los segmentos del contenido
                        let mut pos = 0usize;
                        while pos < rest.len() {
                            match rest[pos..].find('@') {
                                None => {
                                    // Texto de doc comment hasta el final
                                    let seg_len = rest.len() - pos;
                                    raw.push(RawToken {
                                        line: lnum,
                                        col:  (base_col + pos) as u32,
                                        len:  seg_len as u32,
                                        tt:   T_COMMENT,
                                        mods: MOD_DOC,
                                    });
                                    break;
                                }
                                Some(at) => {
                                    // Texto antes del @
                                    if at > 0 {
                                        raw.push(RawToken {
                                            line: lnum,
                                            col:  (base_col + pos) as u32,
                                            len:  at as u32,
                                            tt:   T_COMMENT,
                                            mods: MOD_DOC,
                                        });
                                    }
                                    // Longitud del @word
                                    let after_at = &rest[pos + at + 1..];
                                    let word_len = after_at
                                        .find(|c: char| !c.is_alphanumeric() && c != '_')
                                        .unwrap_or(after_at.len());
                                    let tag_len = 1 + word_len; // '@' + letras
                                    raw.push(RawToken {
                                        line: lnum,
                                        col:  (base_col + pos + at) as u32,
                                        len:  tag_len as u32,
                                        tt:   T_KEYWORD,
                                        mods: MOD_DOC,
                                    });
                                    pos += at + tag_len;
                                }
                            }
                        }
                    } else {
                        raw.push(RawToken {
                            line: lnum,
                            col:  col as u32,
                            len:  (text.len() + 2) as u32,
                            tt:   T_COMMENT,
                            mods: 0,
                        });
                    }
                }
            }
            FmtSpanned::BlockComment { text, line } => {
                let src_line = lines.get(line.saturating_sub(1)).copied().unwrap_or("");
                if let Some(col) = src_line.find("/*") {
                    raw.push(RawToken {
                        line: (line.saturating_sub(1)) as u32,
                        col:  col as u32,
                        len:  (text.len() + 4) as u32,
                        tt:   T_COMMENT,
                        mods: 0,
                    });
                }
            }
            FmtSpanned::BlankLine => {}
        }
    }

    Some(encode(raw))
}

// ── Roles dentro de sentencias `use` ─────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
enum UseRole { Namespace, ImportedName }

/// Pre-pasa el stream de tokens y asigna a cada posición su rol dentro de un
/// `use`: `Namespace` (segmentos de ruta intermedios) o `ImportedName` (el
/// nombre que realmente se importa).
///
/// Reglas:
/// - `@alias` → siempre Namespace
/// - Ident seguido de `.`  → Namespace
/// - Ident dentro de `{}`  → ImportedName
/// - Último ident antes de `;` (sin `{}`) → ImportedName
fn build_use_roles(tokens: &[FmtSpanned]) -> Vec<Option<UseRole>> {
    let mut roles = vec![None; tokens.len()];
    let mut i = 0;

    while i < tokens.len() {
        // Solo tokens reales nos interesan para detectar `use`
        let is_use = matches!(&tokens[i], FmtSpanned::Token(s) if matches!(s.token, Token::Use));
        if !is_use { i += 1; continue; }

        i += 1; // saltar el token `use`
        let mut in_block = false;

        while i < tokens.len() {
            match &tokens[i] {
                FmtSpanned::Token(s) => match &s.token {
                    Token::Semicolon | Token::Eof => break,
                    Token::LBrace => { in_block = true; }
                    Token::RBrace => { in_block = false; }
                    Token::Ident(name) => {
                        if in_block {
                            roles[i] = Some(UseRole::ImportedName);
                        } else if name.starts_with('@') {
                            // @alias es siempre referencia de ruta, nunca el nombre importado
                            roles[i] = Some(UseRole::Namespace);
                        } else {
                            // Buscar el siguiente token real para ver si viene un `.` o `{`
                            let next_tok = tokens[i + 1..].iter().find_map(|sp| {
                                if let FmtSpanned::Token(t) = sp { Some(&t.token) } else { None }
                            });
                            match next_tok {
                                Some(Token::Dot) => roles[i] = Some(UseRole::Namespace),
                                _               => roles[i] = Some(UseRole::ImportedName),
                            }
                        }
                    }
                    _ => {}
                },
                _ => {}
            }
            i += 1;
        }
    }

    roles
}

// ── Posiciones de patrones regex ─────────────────────────────────────────────

const REGEX_FNS: &[&str] = &[
    "regex_match", "regex_test", "regex_find", "regex_find_all",
    "regex_groups", "regex_replace", "regex_replace_all", "regex_split",
];

/// Devuelve los índices en `tokens` de los string literals que son patrones regex.
/// Detecta el 2º argumento (índice 1) de cualquier llamada a función `regex_*`.
fn build_regex_positions(tokens: &[FmtSpanned]) -> std::collections::HashSet<usize> {
    let mut positions = std::collections::HashSet::new();

    // Helper: índice del siguiente token real a partir de `from`
    let next_real = |from: usize| -> Option<(usize, &Token)> {
        tokens[from..].iter().enumerate().find_map(|(j, sp)| {
            if let FmtSpanned::Token(t) = sp { Some((from + j, &t.token)) } else { None }
        })
    };

    for i in 0..tokens.len() {
        let FmtSpanned::Token(s) = &tokens[i] else { continue };
        let Token::Ident(name) = &s.token else { continue };
        if !REGEX_FNS.contains(&name.as_str()) { continue; }

        // Verificar que lo sigue un '('
        let Some((paren_i, Token::LParen)) = next_real(i + 1) else { continue };

        // Escanear los argumentos buscando el 2º (arg_idx == 1)
        let mut depth: i32 = 1;
        let mut arg_idx = 0;
        let mut k = paren_i + 1;

        while k < tokens.len() && depth > 0 {
            if let FmtSpanned::Token(t) = &tokens[k] {
                match &t.token {
                    Token::LParen => depth += 1,
                    Token::RParen => { depth -= 1; }
                    Token::Comma if depth == 1 => { arg_idx += 1; }
                    Token::StringLit(_) if depth == 1 && arg_idx == 1 => {
                        positions.insert(k);
                    }
                    _ => {}
                }
            }
            k += 1;
        }
    }

    positions
}

// ── Clasificación de tokens ───────────────────────────────────────────────────

/// Recolecta los nombres declarados con `const` o `var` para clasificarlos
/// semánticamente de forma diferente.
fn collect_decl_names(
    tokens: &[FmtSpanned],
) -> (std::collections::HashSet<String>, std::collections::HashSet<String>) {
    let mut consts = std::collections::HashSet::new();
    let mut vars   = std::collections::HashSet::new();
    let mut pending: Option<bool> = None; // Some(true) = const, Some(false) = var

    for spanned in tokens {
        if let FmtSpanned::Token(s) = spanned {
            match &s.token {
                Token::Const => { pending = Some(true); }
                Token::Var   => { pending = Some(false); }
                Token::Ident(n) if pending.is_some() => {
                    if pending == Some(true) { consts.insert(n.clone()); }
                    else                     { vars.insert(n.clone()); }
                    pending = None;
                }
                _ => { pending = None; }
            }
        }
    }
    (consts, vars)
}

/// Devuelve `(token_type_index, length, modifier_bitmask)` para un token,
/// o `None` si no se debe emitir semantic token.
fn classify(
    token:       &Token,
    src_line:    &str,
    col:         usize,
    const_names: &std::collections::HashSet<String>,
    var_names:   &std::collections::HashSet<String>,
    ctx_names:   &std::collections::HashSet<&str>,
) -> Option<(u32, u32, u32)> {
    let start = col.saturating_sub(1);
    let rest  = src_line.get(start..).unwrap_or("");

    match token {
        // Identificadores
        Token::Ident(name) => {
            if const_names.contains(name.as_str()) {
                return Some((T_ENUM_MEMBER, name.len() as u32, 0));
            }
            if ctx_names.contains(name.as_str()) {
                // Variable de contexto del .capercfg: bold + underline
                return Some((T_VARIABLE, name.len() as u32, MOD_CONTEXT));
            }
            let mods = if var_names.contains(name.as_str()) { MOD_MUTABLE } else { 0 };
            let after = src_line.get(start + name.len()..).unwrap_or("").trim_start();
            let tt    = if after.starts_with('(') { T_FUNCTION } else { T_VARIABLE };
            Some((tt, name.len() as u32, mods))
        }

        // Literales
        Token::StringLit(s) => {
            let len = if let Some(q) = rest.chars().next() {
                rest[1..].find(q).map(|i| i + 2).unwrap_or(s.len() + 2)
            } else {
                s.len() + 2
            };
            Some((T_STRING, len as u32, 0))
        }
        Token::Number(_) => {
            let len = rest
                .find(|c: char| !c.is_ascii_digit() && c != '.' && c != '_')
                .unwrap_or(rest.len());
            Some((T_NUMBER, len.max(1) as u32, 0))
        }
        Token::Bool(b)  => Some((T_TYPE, if *b { 4 } else { 5 }, 0)),
        Token::Null     => Some((T_TYPE, 4, 0)),

        // Keywords de control de flujo
        Token::If       => Some((T_KEYWORD, 2, 0)),
        Token::Else     => Some((T_KEYWORD, 4, 0)),
        Token::While    => Some((T_KEYWORD, 5, 0)),
        Token::Foreach  => Some((T_KEYWORD, 7, 0)),
        Token::In       => Some((T_KEYWORD, 2, 0)),
        Token::Break    => Some((T_KEYWORD, 5, 0)),
        Token::Continue => Some((T_KEYWORD, 8, 0)),
        Token::Match    => Some((T_KEYWORD, 5, 0)),
        Token::Try      => Some((T_KEYWORD, 3, 0)),
        Token::Catch    => Some((T_KEYWORD, 5, 0)),
        Token::Throw    => Some((T_KEYWORD, 5, 0)),
        Token::Return   => Some((T_KEYWORD, 6, 0)),

        // Keywords de declaración
        Token::Let      => Some((T_KEYWORD, 3, 0)),
        Token::Var      => Some((T_KEYWORD, 3, 0)),
        Token::Const    => Some((T_KEYWORD, 5, 0)),
        Token::Fn       => Some((T_KEYWORD, 2, 0)),
        Token::Emit     => Some((T_KEYWORD, 4, 0)),
        Token::Use      => Some((T_KEYWORD, 3, 0)),
        Token::Header   => Some((T_KEYWORD, 6, 0)),
        Token::Main     => Some((T_KEYWORD, 4, 0)),

        // Operadores lógicos como keywords
        Token::And      => Some((T_KEYWORD, 3, 0)),
        Token::Or       => Some((T_KEYWORD, 2, 0)),
        Token::Not      => Some((T_KEYWORD, 3, 0)),

        // Operadores simbólicos
        Token::DotDotDot      => Some((T_OPERATOR, 3, 0)),
        Token::StarStar       |
        Token::Eq             |
        Token::NotEq          |
        Token::LtEq           |
        Token::GtEq           |
        Token::FatArrow       |
        Token::Arrow          |
        Token::QuestionQuestion |
        Token::DotDot         => Some((T_OPERATOR, 2, 0)),
        Token::Plus           |
        Token::Minus          |
        Token::Star           |
        Token::Slash          |
        Token::Percent        |
        Token::Assign         |
        Token::Lt             |
        Token::Gt             |
        Token::Pipe           |
        Token::Question       => Some((T_OPERATOR, 1, 0)),

        // Delimitadores y EOF → sin semantic token
        Token::LParen | Token::RParen | Token::LBrace | Token::RBrace |
        Token::LBracket | Token::RBracket | Token::Semicolon |
        Token::Colon | Token::Comma | Token::Dot | Token::Eof => None,
    }
}

// ── Codificación LSP ──────────────────────────────────────────────────────────

struct RawToken { line: u32, col: u32, len: u32, tt: u32, mods: u32 }

/// Codifica los tokens en el formato delta de LSP (posiciones relativas al token anterior).
fn encode(tokens: Vec<RawToken>) -> Vec<SemanticToken> {
    let mut result = Vec::with_capacity(tokens.len());
    let mut prev_line = 0u32;
    let mut prev_col  = 0u32;

    for t in tokens {
        let delta_line  = t.line - prev_line;
        let delta_start = if delta_line == 0 { t.col - prev_col } else { t.col };

        result.push(SemanticToken {
            delta_line,
            delta_start,
            length:                 t.len,
            token_type:             t.tt,
            token_modifiers_bitset: t.mods,
        });

        prev_line = t.line;
        prev_col  = t.col;
    }

    result
}
