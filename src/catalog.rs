/// Catálogo estático de documentación de la stdlib de AlcaparraLang.
/// Alimenta tanto los completion items como el hover.
use std::collections::HashMap;
use std::sync::OnceLock;

pub struct FnDoc {
    pub module:    &'static str,
    pub signature: &'static str,
    pub doc:       &'static str,
}

static CATALOG: OnceLock<HashMap<&'static str, FnDoc>> = OnceLock::new();

pub fn lookup(name: &str) -> Option<&'static FnDoc> {
    CATALOG.get_or_init(build).get(name)
}

/// Devuelve todos los nombres de función de la stdlib (para completions).
pub fn all_fn_names() -> impl Iterator<Item = &'static str> {
    alcaparra::stdlib::MODULES
        .iter()
        .flat_map(|m| m.functions.iter().copied())
}

fn build() -> HashMap<&'static str, FnDoc> {
    let mut m: HashMap<&'static str, FnDoc> = HashMap::new();

    // ── std.math ────────────────────────────────────────────────────────────
    fn_doc(&mut m, "abs",           "std.math", "abs(n) → Number",                      "Valor absoluto de `n`.");
    fn_doc(&mut m, "ceil",          "std.math", "ceil(n) → Number",                     "Redondea hacia arriba al entero más cercano.");
    fn_doc(&mut m, "floor",         "std.math", "floor(n) → Number",                    "Redondea hacia abajo al entero más cercano.");
    fn_doc(&mut m, "round",         "std.math", "round(n, decimales) → Number",         "Redondea `n` al número de `decimales` indicado (banker's rounding).");
    fn_doc(&mut m, "round_half_up", "std.math", "round_half_up(n, decimales) → Number", "Redondea `n` con la regla half-up (el más común en finanzas CL).");
    fn_doc(&mut m, "trunc",         "std.math", "trunc(n) → Number",                    "Trunca `n` descartando los decimales.");
    fn_doc(&mut m, "sign",          "std.math", "sign(n) → Number",                     "Retorna 1, -1 o 0 según el signo de `n`.");
    fn_doc(&mut m, "min",           "std.math", "min(a, b) → Number",                   "El menor de dos valores.");
    fn_doc(&mut m, "max",           "std.math", "max(a, b) → Number",                   "El mayor de dos valores.");
    fn_doc(&mut m, "clamp",         "std.math", "clamp(val, min, max) → Number",        "Limita `val` al rango [`min`, `max`].");
    fn_doc(&mut m, "pow",           "std.math", "pow(base, exp) → Number",              "Potencia: `base ** exp`.");
    fn_doc(&mut m, "sqrt",          "std.math", "sqrt(n) → Number",                     "Raíz cuadrada.");
    fn_doc(&mut m, "log",           "std.math", "log(n) → Number",                      "Logaritmo natural (base e).");
    fn_doc(&mut m, "log10",         "std.math", "log10(n) → Number",                    "Logaritmo en base 10.");
    fn_doc(&mut m, "exp",           "std.math", "exp(n) → Number",                      "e elevado a `n`.");
    fn_doc(&mut m, "pi",            "std.math", "pi() → Number",                        "El número π (3.14159…).");
    fn_doc(&mut m, "e",             "std.math", "e() → Number",                         "El número de Euler (2.71828…).");

    // ── std.strings ──────────────────────────────────────────────────────────
    fn_doc(&mut m, "len",                "std.strings", "len(s) → Number",                        "Largo de la cadena en caracteres.");
    fn_doc(&mut m, "upper",             "std.strings", "upper(s) → String",                      "Convierte a mayúsculas.");
    fn_doc(&mut m, "lower",             "std.strings", "lower(s) → String",                      "Convierte a minúsculas.");
    fn_doc(&mut m, "trim",              "std.strings", "trim(s) → String",                       "Elimina espacios al inicio y al final.");
    fn_doc(&mut m, "trim_start",        "std.strings", "trim_start(s) → String",                 "Elimina espacios solo al inicio.");
    fn_doc(&mut m, "trim_end",          "std.strings", "trim_end(s) → String",                   "Elimina espacios solo al final.");
    fn_doc(&mut m, "starts_with",       "std.strings", "starts_with(s, prefix) → Bool",          "¿Empieza con el prefijo dado?");
    fn_doc(&mut m, "ends_with",         "std.strings", "ends_with(s, suffix) → Bool",            "¿Termina con el sufijo dado?");
    fn_doc(&mut m, "contains",          "std.strings", "contains(s, sub) → Bool",                "¿Contiene la subcadena?");
    fn_doc(&mut m, "replace",           "std.strings", "replace(s, old, new) → String",          "Reemplaza la primera ocurrencia de `old` por `new`.");
    fn_doc(&mut m, "replace_all",       "std.strings", "replace_all(s, old, new) → String",      "Reemplaza todas las ocurrencias de `old` por `new`.");
    fn_doc(&mut m, "split",             "std.strings", "split(s, sep) → Array",                  "Divide la cadena por el separador. Retorna array de strings.");
    fn_doc(&mut m, "join",              "std.strings", "join(arr, sep) → String",                "Une el array con el separador.");
    fn_doc(&mut m, "pad_left",          "std.strings", "pad_left(s, largo, char) → String",      "Rellena con `char` a la izquierda hasta `largo`.");
    fn_doc(&mut m, "pad_right",         "std.strings", "pad_right(s, largo, char) → String",     "Rellena con `char` a la derecha hasta `largo`.");
    fn_doc(&mut m, "repeat",            "std.strings", "repeat(s, n) → String",                  "Repite la cadena `n` veces.");
    fn_doc(&mut m, "char_at",           "std.strings", "char_at(s, i) → String",                 "Carácter en la posición `i` (base 0).");
    fn_doc(&mut m, "substring",         "std.strings", "substring(s, inicio, fin) → String",     "Subcadena de `inicio` a `fin` (base 0, `fin` excluido).");
    fn_doc(&mut m, "index_of",          "std.strings", "index_of(s, sub) → Number",              "Posición de la primera ocurrencia de `sub`, o -1.");
    fn_doc(&mut m, "count_occurrences", "std.strings", "count_occurrences(s, sub) → Number",     "Cuántas veces aparece `sub` en `s`.");
    fn_doc(&mut m, "to_number",         "std.strings", "to_number(s) → Number",                  "Parsea la cadena como número.");
    fn_doc(&mut m, "to_string",         "std.strings", "to_string(v) → String",                  "Convierte cualquier valor a su representación string.");

    // ── std.arrays ───────────────────────────────────────────────────────────
    fn_doc(&mut m, "count",      "std.arrays", "count(arr) → Number",                       "Número de elementos del array.");
    fn_doc(&mut m, "push",       "std.arrays", "push(arr, val) → Array",                    "Retorna un nuevo array con `val` al final.");
    fn_doc(&mut m, "pop",        "std.arrays", "pop(arr) → Array",                          "Retorna un nuevo array sin el último elemento.");
    fn_doc(&mut m, "shift",      "std.arrays", "shift(arr) → Array",                        "Retorna un nuevo array sin el primer elemento.");
    fn_doc(&mut m, "unshift",    "std.arrays", "unshift(arr, val) → Array",                 "Retorna un nuevo array con `val` al inicio.");
    fn_doc(&mut m, "slice",      "std.arrays", "slice(arr, inicio, fin) → Array",           "Subarray de `inicio` a `fin` (base 0).");
    fn_doc(&mut m, "concat",     "std.arrays", "concat(arr1, arr2) → Array",                "Concatena dos arrays.");
    fn_doc(&mut m, "flat",       "std.arrays", "flat(arr) → Array",                         "Aplana un array de arrays un nivel.");
    fn_doc(&mut m, "unique",     "std.arrays", "unique(arr) → Array",                       "Elimina duplicados.");
    fn_doc(&mut m, "reverse",    "std.arrays", "reverse(arr) → Array",                      "Invierte el orden.");
    fn_doc(&mut m, "sum",        "std.arrays", "sum(arr) → Number",                         "Suma todos los elementos numéricos.");
    fn_doc(&mut m, "avg",        "std.arrays", "avg(arr) → Number",                         "Promedio de los elementos numéricos.");
    fn_doc(&mut m, "first",      "std.arrays", "first(arr) → Value",                        "Primer elemento del array, o `null`.");
    fn_doc(&mut m, "last",       "std.arrays", "last(arr) → Value",                         "Último elemento del array, o `null`.");
    fn_doc(&mut m, "includes",   "std.arrays", "includes(arr, val) → Bool",                 "¿El array contiene `val`?");
    fn_doc(&mut m, "zip",        "std.arrays", "zip(arr1, arr2) → Array",                   "Combina dos arrays en pares `[a, b]`.");
    fn_doc(&mut m, "range",      "std.arrays", "range(inicio, fin) → Array",                "Array de enteros de `inicio` a `fin` (fin excluido).");
    fn_doc(&mut m, "map",        "std.arrays", "map(arr, |item| => ...) → Array",           "Transforma cada elemento con el closure.");
    fn_doc(&mut m, "filter",     "std.arrays", "filter(arr, |item| => ...) → Array",        "Filtra elementos que cumplen la condición.");
    fn_doc(&mut m, "reduce",     "std.arrays", "reduce(arr, |acc, item| => ..., init) → Value", "Reduce el array a un valor acumulado.");
    fn_doc(&mut m, "find",       "std.arrays", "find(arr, |item| => ...) → Value",          "Primer elemento que cumple la condición, o `null`.");
    fn_doc(&mut m, "find_all",   "std.arrays", "find_all(arr, |item| => ...) → Array",      "Todos los elementos que cumplen la condición.");
    fn_doc(&mut m, "find_index", "std.arrays", "find_index(arr, |item| => ...) → Number",   "Índice del primer elemento que cumple la condición, o -1.");
    fn_doc(&mut m, "any",        "std.arrays", "any(arr, |item| => ...) → Bool",            "¿Algún elemento cumple la condición?");
    fn_doc(&mut m, "all",        "std.arrays", "all(arr, |item| => ...) → Bool",            "¿Todos los elementos cumplen la condición?");
    fn_doc(&mut m, "sort_by",    "std.arrays", "sort_by(arr, |item| => ...) → Array",       "Ordena por el valor que retorna el closure.");
    fn_doc(&mut m, "group_by",   "std.arrays", "group_by(arr, |item| => ...) → Object",     "Agrupa en un objeto por la clave que retorna el closure.");
    fn_doc(&mut m, "partition",  "std.arrays", "partition(arr, |item| => ...) → Array",     "Divide en `[pasaron, fallaron]` según la condición.");

    // ── std.objects ──────────────────────────────────────────────────────────
    fn_doc(&mut m, "keys",         "std.objects", "keys(obj) → Array",                  "Array con las claves del objeto.");
    fn_doc(&mut m, "values",       "std.objects", "values(obj) → Array",                "Array con los valores del objeto.");
    fn_doc(&mut m, "entries",      "std.objects", "entries(obj) → Array",               "Array de pares `[clave, valor]`.");
    fn_doc(&mut m, "has",          "std.objects", "has(obj, clave) → Bool",             "¿El objeto tiene la clave?");
    fn_doc(&mut m, "get",          "std.objects", "get(obj, clave, default) → Value",   "Valor de la clave, o `default` si no existe.");
    fn_doc(&mut m, "set",          "std.objects", "set(obj, clave, val) → Object",      "Retorna un nuevo objeto con la clave asignada.");
    fn_doc(&mut m, "delete",       "std.objects", "delete(obj, clave) → Object",        "Retorna un nuevo objeto sin la clave.");
    fn_doc(&mut m, "merge",        "std.objects", "merge(obj1, obj2) → Object",         "Une dos objetos. Las claves de `obj2` tienen precedencia.");
    fn_doc(&mut m, "pick",         "std.objects", "pick(obj, claves) → Object",         "Retorna un nuevo objeto solo con las claves indicadas.");
    fn_doc(&mut m, "omit",         "std.objects", "omit(obj, claves) → Object",         "Retorna un nuevo objeto sin las claves indicadas.");
    fn_doc(&mut m, "from_entries", "std.objects", "from_entries(arr) → Object",         "Construye un objeto desde un array de pares `[clave, valor]`.");

    // ── std.types ────────────────────────────────────────────────────────────
    fn_doc(&mut m, "type_of",    "std.types", "type_of(v) → String",    "Tipo del valor: `\"number\"`, `\"string\"`, `\"bool\"`, `\"null\"`, `\"array\"`, `\"object\"`.");
    fn_doc(&mut m, "is_null",    "std.types", "is_null(v) → Bool",      "¿El valor es `null`?");
    fn_doc(&mut m, "is_number",  "std.types", "is_number(v) → Bool",    "¿El valor es un número?");
    fn_doc(&mut m, "is_string",  "std.types", "is_string(v) → Bool",    "¿El valor es un string?");
    fn_doc(&mut m, "is_bool",    "std.types", "is_bool(v) → Bool",      "¿El valor es un booleano?");
    fn_doc(&mut m, "is_array",   "std.types", "is_array(v) → Bool",     "¿El valor es un array?");
    fn_doc(&mut m, "is_object",  "std.types", "is_object(v) → Bool",    "¿El valor es un objeto?");
    fn_doc(&mut m, "is_function","std.types", "is_function(v) → Bool",  "¿El valor es una función o closure?");
    fn_doc(&mut m, "to_bool",    "std.types", "to_bool(v) → Bool",      "Convierte a booleano.");

    // ── std.json ─────────────────────────────────────────────────────────────
    fn_doc(&mut m, "json_encode", "std.json", "json_encode(v) → String",        "Serializa el valor a JSON.");
    fn_doc(&mut m, "json_decode", "std.json", "json_decode(s) → Value",         "Parsea un string JSON a un valor Caper.");
    fn_doc(&mut m, "json_valid",  "std.json", "json_valid(s) → Bool",           "¿El string es JSON válido?");
    fn_doc(&mut m, "json_pretty", "std.json", "json_pretty(v) → String",        "Serializa el valor a JSON formateado (indentado).");

    // ── std.dates ────────────────────────────────────────────────────────────
    fn_doc(&mut m, "today",          "std.dates", "today() → String",                        "Fecha de hoy en formato `YYYY-MM-DD`.");
    fn_doc(&mut m, "now",            "std.dates", "now() → String",                          "Fecha y hora actual en formato ISO 8601.");
    fn_doc(&mut m, "year",           "std.dates", "year(fecha) → Number",                    "Año de la fecha dada.");
    fn_doc(&mut m, "month",          "std.dates", "month(fecha) → Number",                   "Mes de la fecha (1–12).");
    fn_doc(&mut m, "day",            "std.dates", "day(fecha) → Number",                     "Día del mes de la fecha.");
    fn_doc(&mut m, "date_diff",      "std.dates", "date_diff(a, b, unidad) → Number",        "Diferencia entre fechas. `unidad`: `\"days\"`, `\"months\"`, `\"years\"`.");
    fn_doc(&mut m, "date_add",       "std.dates", "date_add(fecha, n, unidad) → String",     "Suma `n` unidades a la fecha.");
    fn_doc(&mut m, "date_sub",       "std.dates", "date_sub(fecha, n, unidad) → String",     "Resta `n` unidades a la fecha.");
    fn_doc(&mut m, "date_format",    "std.dates", "date_format(fecha, formato) → String",    "Formatea la fecha. Ej: `date_format(hoy, \"%d/%m/%Y\")`.");
    fn_doc(&mut m, "date_parse",     "std.dates", "date_parse(s, formato) → String",         "Parsea un string con el formato dado a `YYYY-MM-DD`.");
    fn_doc(&mut m, "is_valid_date",  "std.dates", "is_valid_date(s) → Bool",                 "¿El string es una fecha válida?");
    fn_doc(&mut m, "working_days",   "std.dates", "working_days(inicio, fin) → Number",      "Días hábiles entre dos fechas (excluye fines de semana y feriados CL).");

    // ── std.rand ─────────────────────────────────────────────────────────────
    fn_doc(&mut m, "rand",        "std.rand", "rand() → Number",                  "Número aleatorio en [0, 1).");
    fn_doc(&mut m, "rand_int",    "std.rand", "rand_int(min, max) → Number",      "Entero aleatorio en [`min`, `max`].");
    fn_doc(&mut m, "rand_float",  "std.rand", "rand_float(min, max) → Number",    "Decimal aleatorio en [`min`, `max`).");
    fn_doc(&mut m, "rand_item",   "std.rand", "rand_item(arr) → Value",           "Elemento aleatorio del array.");
    fn_doc(&mut m, "rand_sample", "std.rand", "rand_sample(arr, n) → Array",      "Muestra aleatoria de `n` elementos sin repetición.");
    fn_doc(&mut m, "shuffle",     "std.rand", "shuffle(arr) → Array",             "Retorna el array con los elementos en orden aleatorio.");
    fn_doc(&mut m, "seed",        "std.rand", "seed(n)",                          "Inicializa el generador de números aleatorios con semilla `n`.");
    fn_doc(&mut m, "uuid",        "std.rand", "uuid() → String",                  "Genera un UUID v4 aleatorio.");

    // ── std.regex ────────────────────────────────────────────────────────────
    fn_doc(&mut m, "regex_match",       "std.regex", "regex_match(s, patron) → Bool",         "¿El string coincide con el patrón regex?");
    fn_doc(&mut m, "regex_test",        "std.regex", "regex_test(s, patron) → Bool",          "Alias de `regex_match`.");
    fn_doc(&mut m, "regex_find",        "std.regex", "regex_find(s, patron) → String",        "Primera coincidencia, o `null`.");
    fn_doc(&mut m, "regex_find_all",    "std.regex", "regex_find_all(s, patron) → Array",     "Todas las coincidencias.");
    fn_doc(&mut m, "regex_groups",      "std.regex", "regex_groups(s, patron) → Array",       "Grupos de captura de la primera coincidencia.");
    fn_doc(&mut m, "regex_replace",     "std.regex", "regex_replace(s, patron, repl) → String",     "Reemplaza la primera coincidencia.");
    fn_doc(&mut m, "regex_replace_all", "std.regex", "regex_replace_all(s, patron, repl) → String", "Reemplaza todas las coincidencias.");
    fn_doc(&mut m, "regex_split",       "std.regex", "regex_split(s, patron) → Array",        "Divide el string por el patrón regex.");

    // ── std.crypto ───────────────────────────────────────────────────────────
    fn_doc(&mut m, "md5",              "std.crypto", "md5(s) → String",              "Hash MD5 del string (hex).");
    fn_doc(&mut m, "sha1",             "std.crypto", "sha1(s) → String",             "Hash SHA-1 (hex).");
    fn_doc(&mut m, "sha256",           "std.crypto", "sha256(s) → String",           "Hash SHA-256 (hex).");
    fn_doc(&mut m, "sha512",           "std.crypto", "sha512(s) → String",           "Hash SHA-512 (hex).");
    fn_doc(&mut m, "hmac_sha256",      "std.crypto", "hmac_sha256(msg, key) → String","HMAC-SHA256 (hex).");
    fn_doc(&mut m, "base64_encode",    "std.crypto", "base64_encode(s) → String",    "Codifica a Base64.");
    fn_doc(&mut m, "base64_decode",    "std.crypto", "base64_decode(s) → String",    "Decodifica desde Base64.");
    fn_doc(&mut m, "base64_url_encode","std.crypto", "base64_url_encode(s) → String","Codifica a Base64 URL-safe.");
    fn_doc(&mut m, "hex_encode",       "std.crypto", "hex_encode(s) → String",       "Codifica a hexadecimal.");
    fn_doc(&mut m, "hex_decode",       "std.crypto", "hex_decode(s) → String",       "Decodifica desde hexadecimal.");

    // ── std.time ─────────────────────────────────────────────────────────────
    fn_doc(&mut m, "timestamp",        "std.time", "timestamp() → Number",               "Unix timestamp en segundos.");
    fn_doc(&mut m, "timestamp_ms",     "std.time", "timestamp_ms() → Number",            "Unix timestamp en milisegundos.");
    fn_doc(&mut m, "from_timestamp",   "std.time", "from_timestamp(ts) → String",        "Fecha ISO desde timestamp en segundos.");
    fn_doc(&mut m, "from_timestamp_ms","std.time", "from_timestamp_ms(ts) → String",     "Fecha ISO desde timestamp en milisegundos.");
    fn_doc(&mut m, "to_timestamp",     "std.time", "to_timestamp(fecha) → Number",       "Unix timestamp desde un string de fecha ISO.");
    fn_doc(&mut m, "time_now",         "std.time", "time_now() → String",                "Hora actual en formato `HH:MM:SS`.");
    fn_doc(&mut m, "time_format",      "std.time", "time_format(ts, formato) → String",  "Formatea un timestamp con el patrón dado.");
    fn_doc(&mut m, "elapsed_ms",       "std.time", "elapsed_ms(ts_inicio) → Number",     "Milisegundos transcurridos desde `ts_inicio`.");

    // ── std.sort ─────────────────────────────────────────────────────────────
    fn_doc(&mut m, "sort_asc",      "std.sort", "sort_asc(arr) → Array",                  "Ordena números o strings en orden ascendente.");
    fn_doc(&mut m, "sort_desc",     "std.sort", "sort_desc(arr) → Array",                 "Ordena en orden descendente.");
    fn_doc(&mut m, "sort_by_desc",  "std.sort", "sort_by_desc(arr, |item| => ...) → Array","Ordena descendente por el valor del closure.");
    fn_doc(&mut m, "order_by",      "std.sort", "order_by(arr, clave, dir) → Array",      "Ordena objetos por la clave. `dir`: `\"asc\"` o `\"desc\"`.");
    fn_doc(&mut m, "group_by_key",  "std.sort", "group_by_key(arr, clave) → Object",      "Agrupa objetos por el valor de la clave.");
    fn_doc(&mut m, "chunk",         "std.sort", "chunk(arr, n) → Array",                  "Divide el array en subarrays de tamaño `n`.");

    // ── std.search ───────────────────────────────────────────────────────────
    fn_doc(&mut m, "binary_search", "std.search", "binary_search(arr, val) → Number",          "Índice de `val` en un array ordenado, o -1 (búsqueda binaria).");
    fn_doc(&mut m, "in_range",      "std.search", "in_range(val, min, max) → Bool",            "¿`val` está en el rango [`min`, `max`]?");
    fn_doc(&mut m, "search",        "std.search", "search(arr, val) → Number",                 "Índice de la primera ocurrencia de `val`, o -1.");
    fn_doc(&mut m, "fuzzy",         "std.search", "fuzzy(s, patron) → Bool",                   "Búsqueda difusa: ¿`patron` aparece aproximadamente en `s`?");

    // ── std.xml ──────────────────────────────────────────────────────────────
    fn_doc(&mut m, "xml_encode",      "std.xml", "xml_encode(obj) → String",             "Serializa el valor a XML.");
    fn_doc(&mut m, "xml_encode_opts", "std.xml", "xml_encode_opts(obj, opts) → String",  "Serializa a XML con opciones de formato.");
    fn_doc(&mut m, "xml_decode",      "std.xml", "xml_decode(s) → Value",                "Parsea un string XML a un valor Caper.");
    fn_doc(&mut m, "xml_valid",       "std.xml", "xml_valid(s) → Bool",                  "¿El string es XML válido?");
    fn_doc(&mut m, "xml_get",         "std.xml", "xml_get(s, path) → Value",             "Extrae un valor del XML por ruta (ej: `\"root.item\"`).");
    fn_doc(&mut m, "xml_get_all",     "std.xml", "xml_get_all(s, path) → Array",         "Extrae todos los valores que coinciden con la ruta.");

    // ── std.yaml ─────────────────────────────────────────────────────────────
    fn_doc(&mut m, "yaml_encode",      "std.yaml", "yaml_encode(v) → String",           "Serializa el valor a YAML.");
    fn_doc(&mut m, "yaml_encode_opts", "std.yaml", "yaml_encode_opts(v, opts) → String","Serializa a YAML con opciones.");
    fn_doc(&mut m, "yaml_decode",      "std.yaml", "yaml_decode(s) → Value",            "Parsea un string YAML a un valor Caper.");
    fn_doc(&mut m, "yaml_valid",       "std.yaml", "yaml_valid(s) → Bool",              "¿El string es YAML válido?");

    m
}

fn fn_doc(
    map: &mut HashMap<&'static str, FnDoc>,
    name: &'static str,
    module: &'static str,
    signature: &'static str,
    doc: &'static str,
) {
    map.insert(name, FnDoc { module, signature, doc });
}
