use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::SymbolName;

#[must_use]
pub fn extracted_call(name: &SymbolName, params: &[String]) -> String {
    if params.is_empty() {
        format!("({})", name.as_str())
    } else {
        format!("({} {})", name.as_str(), params.join(" "))
    }
}

#[must_use]
pub fn extracted_definition(
    dialect: Dialect,
    name: &SymbolName,
    params: &[String],
    body: &str,
) -> String {
    let space_params = params.join(" ");
    match dialect {
        Dialect::Scheme | Dialect::Racket if params.is_empty() => {
            format!("(define ({}) {})", name.as_str(), body)
        }
        Dialect::Scheme | Dialect::Racket => {
            format!("(define ({} {}) {})", name.as_str(), space_params, body)
        }
        Dialect::Clojure | Dialect::Hy | Dialect::Carp | Dialect::Janet => {
            format!("(defn {} [{}] {})", name.as_str(), space_params, body)
        }
        Dialect::Fennel => format!("(fn {} [{}] {})", name.as_str(), space_params, body),
        Dialect::CommonLisp | Dialect::EmacsLisp | Dialect::Lfe | Dialect::Unknown => {
            format!("(defun {} ({}) {})", name.as_str(), space_params, body)
        }
    }
}
