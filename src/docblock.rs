/// Soporte de DocBlocks con `///` para funciones de usuario.
///
/// Convención (inspirada en Rust + JSDoc):
///
/// ```caper
/// /// Descripción corta de la función.
/// /// Puede abarcar varias líneas.
/// ///
/// /// @param base  Sueldo bruto del trabajador
/// /// @param tasa  Porcentaje de descuento (0.0 – 1.0)
/// /// @returns     Monto neto a pagar
/// fn calcular(base, tasa) { ... }
/// ```
///
/// Regla de asociación: las líneas `///` deben ser contiguas y estar
/// inmediatamente antes del `fn` (sin líneas en blanco intermedias).

use std::collections::HashMap;

// ── Tipos públicos ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct DocBlock {
    /// Texto libre antes de los tags.
    pub summary: String,
    /// Lista de `(nombre_param, descripción)`.
    pub params: Vec<(String, String)>,
    /// Descripción del valor de retorno.
    pub returns: Option<String>,
}

impl DocBlock {
    pub fn is_empty(&self) -> bool {
        self.summary.is_empty() && self.params.is_empty() && self.returns.is_none()
    }

    /// Renderiza el docblock como Markdown para mostrar en hover.
    pub fn to_hover_markdown(&self, fn_name: &str, param_names: &[String]) -> String {
        let signature = format!("fn {}({})", fn_name, param_names.join(", "));
        let mut out = format!("```caper\n{}\n```", signature);

        if !self.summary.is_empty() {
            out.push_str(&format!("\n\n{}", self.summary));
        }

        if !self.params.is_empty() {
            out.push_str("\n\n**Parámetros**");
            for (name, desc) in &self.params {
                if desc.is_empty() {
                    out.push_str(&format!("\n- `{}`", name));
                } else {
                    out.push_str(&format!("\n- `{}` — {}", name, desc));
                }
            }
        }

        if let Some(ret) = &self.returns {
            out.push_str(&format!("\n\n**Retorna:** {}", ret));
        }

        out
    }

    /// Descripción del parámetro `name`, si está documentado.
    pub fn param_doc(&self, name: &str) -> Option<&str> {
        self.params
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, d)| d.as_str())
            .filter(|d| !d.is_empty())
    }
}

// ── Punto de entrada público ──────────────────────────────────────────────────

/// Busca el DocBlock asociado a la función `fn_name` en el fuente.
pub fn find(source: &str, fn_name: &str) -> Option<DocBlock> {
    extract_all(source).remove(fn_name)
}

// ── Extracción ────────────────────────────────────────────────────────────────

/// Extrae todos los DocBlocks del fuente indexados por nombre de función.
fn extract_all(source: &str) -> HashMap<String, DocBlock> {
    let mut result  = HashMap::new();
    let mut pending: Vec<String> = Vec::new();

    for line in source.lines() {
        let trimmed = line.trim();

        if let Some(rest) = trimmed.strip_prefix("///") {
            // Línea de doc: acumular (quitando el espacio opcional tras `///`)
            pending.push(rest.strip_prefix(' ').unwrap_or(rest).to_string());
        } else if trimmed.starts_with("fn ") && !pending.is_empty() {
            // `fn` inmediatamente después de `///` → asociar
            if let Some(name) = fn_name_from_line(trimmed) {
                result.insert(name, parse(&pending));
            }
            pending.clear();
        } else {
            // Cualquier otra línea (incluso en blanco) rompe la asociación
            pending.clear();
        }
    }

    result
}

// ── Parser de DocBlock ────────────────────────────────────────────────────────

fn parse(lines: &[String]) -> DocBlock {
    let mut summary_lines: Vec<&str> = Vec::new();
    let mut params:  Vec<(String, String)> = Vec::new();
    let mut returns: Option<String>        = None;

    for line in lines {
        if let Some(rest) = line.strip_prefix("@param") {
            let rest = rest.trim_start();
            // @param nombre   descripción (separados por cualquier espacio)
            let (name, desc) = rest
                .split_once(|c: char| c.is_ascii_whitespace())
                .map(|(n, d)| (n.trim().to_string(), d.trim().to_string()))
                .unwrap_or_else(|| (rest.trim().to_string(), String::new()));
            if !name.is_empty() {
                params.push((name, desc));
            }
        } else if let Some(rest) = line.strip_prefix("@returns").or_else(|| line.strip_prefix("@return")) {
            returns = Some(rest.trim().to_string());
        } else {
            summary_lines.push(line.as_str());
        }
    }

    // Unimos el summary eliminando líneas vacías iniciales/finales
    let summary = summary_lines
        .iter()
        .map(|s| s.trim_end())
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string();

    DocBlock { summary, params, returns }
}

// ── Helper ────────────────────────────────────────────────────────────────────

/// Extrae el nombre de la función de una línea como `fn calcular(base, tasa) {`.
fn fn_name_from_line(line: &str) -> Option<String> {
    let after = line.strip_prefix("fn ")?.trim_start();
    let end   = after.find(|c: char| !c.is_alphanumeric() && c != '_')?;
    let name  = &after[..end];
    if name.is_empty() { None } else { Some(name.to_string()) }
}
