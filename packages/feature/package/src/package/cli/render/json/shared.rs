use paredit_core_syntax::sexpr::ByteSpan;

use serde_json::{Value, json};

pub fn span_json(span: ByteSpan) -> Value {
    json!({
        "start": span.start().get(),
        "end": span.end().get(),
    })
}
