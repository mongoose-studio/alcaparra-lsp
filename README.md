# alcaparra-lsp
### by Mongoose Studio

![Rust](https://img.shields.io/badge/Rust-1.94+-orange)
![Alcaparra](https://img.shields.io/badge/Alcaparra-0.1.0-olive)
![LSP](https://img.shields.io/badge/LSP-3.17-blue)
![License](https://img.shields.io/badge/license-MIT-green)
![Status](https://img.shields.io/badge/status-active-success)

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/mongoose-studio/alcaparra-lang/main/alcaparra-banner-dark.png">
  <source media="(prefers-color-scheme: light)" srcset="https://raw.githubusercontent.com/mongoose-studio/alcaparra-lang/main/alcaparra-banner.png">
  <img alt="AlcaparraLang LSP" height="64px" src="https://raw.githubusercontent.com/mongoose-studio/alcaparra-lang/main/alcaparra-banner-dark.png">
</picture>

**alcaparra-lsp** es el servidor de lenguaje oficial de [AlcaparraLang](https://github.com/mongoose-studio/alcaparra-lang).

Convierte archivos `.caper` en una experiencia de desarrollo completa: diagnósticos en tiempo real, autocompletado inteligente, navegación de código y tooling profesional directamente en tu editor.

Implementa el protocolo LSP 3.17 sobre [tower-lsp](https://github.com/ebkalderon/tower-lsp) y provee diagnósticos en tiempo real, autocompletado contextual, hover con documentación, go-to-definition, rename, formateo y highlighting semántico completo para archivos `.caper` y `.capercfg`.

Funciona con cualquier editor compatible con LSP. Las integraciones oficiales son:

- **[alcaparra-vscode](https://github.com/mongoose-studio/alcaparra-vscode)** — extensión para Visual Studio Code
- **[alcaparra-intellij](https://github.com/mongoose-studio/alcaparra-intellij)** — plugin para JetBrains IDEs (IntelliJ IDEA, PHPStorm, RustRover, etc.)

A diferencia de integraciones básicas o sintaxis highlighting,
**alcaparra-lsp** entiende el código: analiza scopes, detecta errores reales
y provee tooling comparable al de lenguajes modernos.

**En resumen**: **alcaparra-lsp** convierte tu editor en una herramienta de desarrollo real para AlcaparraLang... rápida, precisa y sin dolores de cabeza 🇨🇱.

### En 10 segundos:

- Escribe `.caper`
- Ve errores en tiempo real
- Autocompleta funciones y variables
- Navega código como en cualquier lenguaje moderno

---

## Índice

- [Funcionalidades](#funcionalidades)
- [Instalación](#instalación)
  - [Desde el código fuente](#desde-el-código-fuente)
  - [Script curl (macOS / Linux)](#script-curl-macos--linux)
  - [macOS .pkg](#macos-pkg)
  - [Linux .deb](#linux-deb)
- [Integración con VS Code](#integración-con-vs-code)
- [Integración con JetBrains](#integración-con-jetbrains)
- [Funcionalidades en detalle](#funcionalidades-en-detalle)
  - [Diagnósticos](#diagnósticos)
  - [Autocompletado](#autocompletado)
  - [Hover](#hover)
  - [Signature Help](#signature-help)
  - [Go to Definition](#go-to-definition)
  - [Referencias y Rename](#referencias-y-rename)
  - [Highlighting semántico](#highlighting-semántico)
  - [Code Lens](#code-lens)
  - [Formateo](#formateo)
  - [Document Symbols](#document-symbols)
- [Color Scheme](#color-scheme)
- [Configuración del proyecto (.capercfg)](#configuración-del-proyecto-capercfg)
- [Arquitectura](#arquitectura)
- [Desarrollo](#desarrollo)
- [Estado del Proyecto](#estado-del-proyecto)
- [Licencia](#licencia)
- [Apoyo](#apoyo)
- [Autor](#autor)
- [Contribuciones](#contribuciones)

---

## Funcionalidades

| Capacidad | Descripción |
|-----------|-------------|
| **Diagnósticos** | Errores de sintaxis, variables no declaradas, `MISSING_EMIT`, `DEAD_EMIT`, importaciones no usadas, reasignación de inmutables, redeclaraciones |
| **Autocompletado** | Funciones stdlib, variables locales, funciones de usuario, rutas de `use`, variables de contexto |
| **Hover** | Documentación de stdlib y DocBlocks `///` de funciones de usuario |
| **Signature Help** | Parámetros de la función activa al escribir argumentos |
| **Go to Definition** | Variables, funciones locales, funciones importadas, variables de contexto → `.capercfg` |
| **References** | Todas las ocurrencias de un identificador en el documento |
| **Document Highlight** | Resaltado de ocurrencias en el scope de la función actual |
| **Rename** | Renombrado seguro de identificadores en todo el documento |
| **Semantic Tokens** | Colores diferenciados para keywords, strings, funciones, namespaces, constantes, variables de contexto, patrones regex, `true`/`false`/`null` |
| **Code Lens** | Botón ▶ Ejecutar sobre cada bloque `main {}` |
| **Formateo** | Delega en el formateador de `alcaparra` con soporte de `.capercfg` |
| **Document Symbols** | Outline del archivo (funciones, variables, constantes) |
| **Workspace Cache** | Caché de documentos abiertos para respuestas inmediatas |

---

## Instalación

### Script curl (macOS / Linux)

```bash
curl -fsSL https://alcaparra.mongoosestudio.cl/install.sh | sh
```

Detecta automáticamente la plataforma e instala `caper` y `alcaparra-lsp` en `/usr/local/bin`.

### macOS .pkg

Descarga el instalador desde [Releases](https://github.com/mongoose-studio/alcaparra-lang/releases):

```
alcaparra-macos-arm64.pkg    # Apple Silicon (M1/M2/M3/M4/M5)
alcaparra-macos-x86_64.pkg   # Intel
```

### Linux .deb

```bash
wget https://github.com/mongoose-studio/alcaparra-lang/releases/latest/download/alcaparra-linux-x86_64.deb
sudo dpkg -i alcaparra-linux-x86_64.deb
```

### Desde el código fuente

Requiere tener instalado [AlcaparraLang](https://github.com/mongoose-studio/alcaparra-lang) como crate local (el LSP lo usa como dependencia para parsear y formatear).

```bash
git clone https://github.com/mongoose-studio/alcaparra-lsp
cd alcaparra-lsp
cargo build --release
```

El binario queda en `target/release/alcaparra-lsp`. Cópialo a tu `PATH` o crea un symlink:

```bash
sudo ln -sf "$(pwd)/target/release/alcaparra-lsp" /usr/local/bin/alcaparra-lsp
```

Verifica la instalación:

```bash
alcaparra-lsp --version
```

---

## Integración con VS Code

1. Instala la extensión **AlcaparraLang** desde el Marketplace (o abre el `.vsix` descargado desde Releases).
2. Abre cualquier archivo `.caper` — el LSP se inicia automáticamente.

La extensión detecta el binario `alcaparra-lsp` en el PATH. Si lo tienes en otra ubicación, configúralo en Settings:

```json
{
  "alcaparra.lspPath":   "/ruta/a/alcaparra-lsp",
  "alcaparra.caperPath": "/ruta/a/caper"
}
```

**Comandos disponibles** (`Ctrl/Cmd + Shift + P`):

| Comando | Descripción |
|---------|-------------|
| `AlcaparraLang: New Script` | Crea un archivo `.caper` con template `header + main` |
| `AlcaparraLang: New Library` | Crea una librería `.caper` con template de funciones |
| `AlcaparraLang: New Config` | Crea un `.capercfg` con todos los campos |
| `AlcaparraLang: Run Script` | Ejecuta el archivo actual con `caper run` en terminal reutilizable |
| `AlcaparraLang: New Project` | Ejecuta `caper new <nombre>` para crear un proyecto completo |

---

## Integración con JetBrains

1. Instala el plugin **AlcaparraLang** desde el Marketplace (o desde `Settings → Plugins → Install from disk`).
2. El plugin detecta automáticamente `alcaparra-lsp` en el PATH.
3. Si es necesario, configura la ruta en `Settings → Tools → AlcaparraLang`.

**Características adicionales del plugin:**

- **Color Scheme propio** — configurable en `Settings → Editor → Color Scheme → AlcaparraLang`
- **Cmd+Click** en variables de contexto → navega al `.capercfg`
- **Cmd+Click** en funciones importadas → navega al archivo `.caper` externo
- **Run Configuration** — ejecuta scripts `.caper` directamente desde el IDE con parámetros de contexto
- **New → AlcaparraLang Script / Library** en el menú de proyecto
- **Folding** de bloques `main {}`, `fn`, `header {}`, `if`, `foreach`
- **Structure View** — outline de funciones y variables del archivo
- **JSON Schema** para `.capercfg` con validación y autocompletado

---

## Funcionalidades en detalle

### Diagnósticos

El LSP ejecuta tres pasadas de análisis con debounce de 300ms después de cada cambio:

```
Interpreter::validate(source)   → errores léxicos y de parse
Interpreter::lint(source)       → MISSING_EMIT, DEAD_EMIT, NO_RETURN
scope::check_undefined(source)  → variables/funciones no declaradas
```

**Diagnósticos disponibles:**

| Código | Severidad | Descripción |
|--------|-----------|-------------|
| `UNDEFINED_VARIABLE` | Error | Variable o función usada sin declarar |
| `IMMUTABLE_ASSIGN` | Error | Reasignación de variable `let` o `const` |
| `VARIABLE_REDECLARATION` | Error | Redeclaración de nombre en el mismo scope |
| `MISSING_EMIT` | Warning | El script no produce ninguna salida |
| `DEAD_EMIT` | Warning | `emit` inalcanzable (hay otro `emit` antes) |
| `NO_RETURN` | Warning | Función sin `emit` en al menos una rama |
| `UNUSED_IMPORT` | Warning | Nombre importado con `use` que no se usa |

El análisis de scope es **conservador**: nunca reporta falsos positivos. Reconoce automáticamente variables de contexto del `.capercfg`, constantes del `header {}`, bindings de `match` (`n if n <= 30`), parámetros de closures, variables de `foreach` y errores de `catch`.

### Autocompletado

El autocompletado es contextual según la posición del cursor:

- **Después de `use `** — módulos stdlib disponibles (`std.math`, `std.arrays`, `std.regex`, …) y aliases del `.capercfg` (`@formulas`, `@lib`, …)
- **Después de `use std.math.`** — funciones del módulo (`round`, `abs`, `ceil`, …)
- **En expresiones** — funciones stdlib importadas, funciones de usuario, variables declaradas, variables de contexto
- **Prioridad** — las funciones locales aparecen primero, luego las stdlib importadas

```caper
use std.math.{ ro| }   // → round, round_half_up, …

let x = rou|           // → round(n, decimales)
```

### Hover

Muestra documentación al posicionar el cursor sobre un identificador:

- **Funciones stdlib** — firma, módulo y descripción del catálogo
- **Funciones de usuario** — DocBlock `///` escrito encima de la función
- **Variables de contexto** — valor del `.capercfg` si está disponible

```caper
/// Calcula la gratificación legal anual.
/// @param sueldo   Sueldo base mensual
/// @param meses    Meses trabajados en el año
/// @returns        Monto de gratificación
fn calcular_gratificacion(sueldo, meses) { … }
```

Al hacer hover sobre `calcular_gratificacion(…)` en cualquier parte del archivo se muestra el DocBlock completo.

### Signature Help

Al abrir un paréntesis o escribir una coma dentro de una llamada, el LSP muestra los parámetros de la función con el argumento activo resaltado:

```
round( n,  decimales )
       ^              ← cursor aquí: primer parámetro activo
```

### Go to Definition

`Cmd+Click` (macOS) o `Ctrl+Click` navega a:

| Identificador | Destino |
|---|---|
| Función local | Declaración `fn` en el mismo archivo |
| Función en `main {}` | Declaración textual (fallback) |
| Función importada | Archivo `.caper` externo en la línea de declaración |
| Variable local | Declaración `let` / `var` |
| Variable de contexto | Línea de la clave en el `.capercfg` del proyecto |

### Referencias y Rename

- **Find References** — lista todas las ocurrencias del identificador en el documento
- **Document Highlight** — resalta ocurrencias dentro del scope de la función actual (no cruza bloques)
- **Rename** — renombra el símbolo en todas sus ocurrencias con un solo comando

### Highlighting semántico

Los semantic tokens diferencian visualmente cada categoría de token. Los colores por defecto son:

| Token | Color por defecto |
|---|---|
| Keywords (`let`, `fn`, `emit`, …) | Según tema del IDE |
| Strings | Según tema del IDE |
| Números | Según tema del IDE |
| Funciones (declaración e invocación) | Azul `#5fb4e8` |
| Módulos / namespaces en `use` | Púrpura `#c678dd` |
| Nombres importados en `use` | Azul `#5fb4e8` |
| Constantes (`const`) | `enumMember` del tema |
| Variables de contexto (`.capercfg`) | Verde `#9bba2c`, **negrita**, subrayado |
| Variables mutables (`var`) | Con subrayado sutil |
| Patrones regex | Dorado `#e5c07b` |
| `true` / `false` / `null` | Naranja `#d19a66` |
| DocBlocks `///` | Color de doc comment del tema |
| Tags `@param`, `@returns` | Color de doc comment tag, negrita |

Todos los colores son personalizables en ambos IDEs (ver [Color Scheme](#color-scheme)).

### Code Lens

Sobre cada bloque `main {}` aparece el lens **▶ Ejecutar** que lanza `caper run` en la terminal del IDE.

```caper
▶ Ejecutar          ← lens clickeable
main {
    emit { resultado: 42 };
}
```

### Formateo

El comando **Format Document** delega en el formateador de `alcaparra`. Respeta la configuración de la sección `fmt` del `.capercfg`:

```json
{
  "fmt": {
    "indent_size":     4,
    "quotes":          "double",
    "max_blank_lines": 1
  }
}
```

### Document Symbols

El outline del archivo lista funciones, variables y constantes para navegación rápida. Disponible en:

- VS Code: barra de breadcrumbs y `Cmd+Shift+O`
- JetBrains: Structure View (`Cmd+7`)

---

## Color Scheme

### VS Code

Los colores por defecto se aplican automáticamente vía `configurationDefaults` en el `package.json` de la extensión. Para personalizarlos, edita `editor.semanticTokenColorCustomizations` en tu `settings.json`:

```json
{
  "editor.semanticTokenColorCustomizations": {
    "rules": {
      "function":  "#5fb4e8",
      "namespace": "#c678dd",
      "regexp":    "#e5c07b",
      "type":      "#d19a66",
      "*.contextVariable": {
        "bold":      true,
        "underline": true
      }
    }
  }
}
```

### JetBrains

Ve a `Settings → Editor → Color Scheme → AlcaparraLang`. El panel muestra un fragmento de código de ejemplo con cada token coloreado y permite personalizar cada categoría:

```
Variables
  ├─ Variable mutable (var)
  ├─ Variable inmutable (let)
  └─ Variable de contexto (.capercfg)
Funciones
  └─ Nombre de función
Importaciones
  ├─ Módulo / namespace (use)
  └─ Nombre importado
Literales
  ├─ true / false / null
  └─ Patrón regex
DocBlock
  ├─ Comentario de documentación
  └─ Tag (@param, @returns)
```

---

## Configuración del proyecto (.capercfg)

El LSP detecta automáticamente el `.capercfg` más cercano subiendo desde el directorio del archivo abierto. Extrae:

- **`context`** — variables inyectadas disponibles en los scripts (se excluyen de `UNDEFINED_VARIABLE`)
- **`paths`** — aliases de ruta para resolver imports externos (`use @formulas.lib`)
- **`fmt`** — configuración del formateador

Ejemplo de `.capercfg` completo:

```json
{
  "name":    "mis-formulas",
  "version": "1.0.0",
  "entry":   "main.caper",
  "context": {
    "SUELDO_BASE":  850000,
    "FONASA_TASA":  0.07,
    "PAIS":         "CL"
  },
  "paths": {
    "@formulas": "./formulas",
    "@lib":      "./lib"
  },
  "fmt": {
    "indent_size":     4,
    "quotes":          "double",
    "max_blank_lines": 1
  }
}
```

El LSP provee validación JSON, autocompletado y hover dentro del `.capercfg` a través del JSON Schema embebido en el plugin/extensión.

---

## Arquitectura

```
src/
  main.rs           → inicializa tower-lsp, registra módulos
  backend.rs        → todos los handlers LSP (impl LanguageServer)
  analysis.rs       → orquesta 3 pasadas: validate → lint → scope
  scope.rs          → detecta variables/funciones no declaradas
  completion.rs     → autocompletado contextual (use, stdlib, general)
  hover.rs          → hover para stdlib y funciones de usuario
  signature.rs      → signature help al escribir argumentos
  symbols.rs        → outline, goto-definition, references, rename
  codelens.rs       → CodeLens "▶ Ejecutar" sobre main {}
  docblock.rs       → parser de /// DocBlocks
  formatting.rs     → delega en alcaparra::formatter
  semantic.rs       → semantic tokens (11 tipos, delta-encoded)
  catalog.rs        → catálogo estático de ~120 funciones stdlib
  workspace.rs      → caché de documentos abiertos
  project_config.rs → lectura de .capercfg + resolución de aliases
```

El análisis pesado (parsing, validación) corre en `tokio::task::spawn_blocking` para no bloquear el event loop del LSP. Los diagnósticos tienen debounce de 300ms.

---

## Desarrollo

```bash
# Build de desarrollo
cargo build

# Build de release (el binario que usa el IDE)
cargo build --release

# Verificar tipos sin compilar
cargo check
```

Después de cualquier cambio, hacer `cargo build --release` y recargar la ventana del IDE:

- **VS Code**: `Cmd+Shift+P → Developer: Reload Window`
- **JetBrains**: `File → Invalidate Caches → Restart`

### Generar instaladores de release

El script `scripts/build-release.sh` produce los artefactos para distribución. Requiere ejecutarse en ambas arquitecturas:

```bash
# Mac Apple Silicon (arm64) → produce macos-arm64
./scripts/build-release.sh --version 0.1.0

# Mac Intel (x86_64) → produce macos-x86_64 + linux-x86_64
./scripts/build-release.sh --version 0.1.0
```

Luego combinar los artefactos y publicar:

```bash
gh release create v0.1.0 dist/* --title 'v0.1.0' --notes 'Early Access'
```

**Dependencias** (Mac Intel solamente):

```bash
rustup target add x86_64-unknown-linux-gnu
cargo install cargo-zigbuild
brew install zig dpkg
```

---

## Estado del proyecto

**alcaparra-lsp** está en fase Early Access.

- API estable
- Funcionalidades principales
- Posibles cambios breaking
- Pendiente publicación en marketplace de plugins

---

## Licencia

Este proyecto está licenciado bajo la Licencia MIT — ver el archivo [LICENSE](LICENSE) para más detalles.

## Autor

**Marcel Rojas**  
[marcelrojas16@gmail.com](mailto:marcelrojas16@gmail.com)  
__Mongoose Studio__

## Apoyo

Si te gusta este proyecto, puedes apoyarme aquí:

[![ko-fi](https://ko-fi.com/img/githubbutton_sm.svg)](https://ko-fi.com/mongoosestudio)

## Contribuciones

Las contribuciones son bienvenidas. Por favor:

1. Fork el proyecto
2. Crea una rama para tu feature (`git checkout -b feature/amazing-feature`)
3. Commit tus cambios (`git commit -m 'Add amazing feature'`)
4. Push a la rama (`git push origin feature/amazing-feature`)
5. Abre un Pull Request

---

💚 **alcaparra-lsp** by Mongoose Studio
