use super::ClojureOperator;

/// Strips the `clojure.core/` (or `cljs.core/`) qualifier from a head.
///
/// Clojure code may refer to a core form by its fully qualified name, and
/// `(clojure.core/defn f [] nil)` is the same definition as `(defn f [] nil)`.
/// Any other qualifier names a different var and is left alone, so
/// `my.ns/defn` never resolves to the core operator.
#[must_use]
pub fn normalize_clojure_operator_head(head: &str) -> &str {
    for prefix in ["clojure.core/", "cljs.core/"] {
        match head.strip_prefix(prefix) {
            Some(rest) if !rest.is_empty() => return rest,
            _ => {}
        }
    }
    head
}

pub(super) fn clojure_operator_from_head(head: &str) -> Option<ClojureOperator> {
    Some(match normalize_clojure_operator_head(head) {
        "ns" => ClojureOperator::Ns,
        "in-ns" => ClojureOperator::InNs,
        "def" => ClojureOperator::Def,
        "defonce" => ClojureOperator::Defonce,
        "declare" => ClojureOperator::Declare,
        "defn" => ClojureOperator::Defn,
        "defn-" => ClojureOperator::DefnPrivate,
        "defmacro" => ClojureOperator::Defmacro,
        "defmulti" => ClojureOperator::Defmulti,
        "defmethod" => ClojureOperator::Defmethod,
        "defprotocol" => ClojureOperator::Defprotocol,
        "definterface" => ClojureOperator::Definterface,
        "defrecord" => ClojureOperator::Defrecord,
        "deftype" => ClojureOperator::Deftype,
        "defstruct" => ClojureOperator::Defstruct,

        "deftest" => ClojureOperator::Deftest,
        "testing" => ClojureOperator::Testing,
        "use-fixtures" => ClojureOperator::UseFixtures,

        "let" => ClojureOperator::Let,
        "letfn" => ClojureOperator::Letfn,
        "loop" => ClojureOperator::Loop,
        "binding" => ClojureOperator::Binding,
        "with-local-vars" => ClojureOperator::WithLocalVars,
        "with-open" => ClojureOperator::WithOpen,
        "with-redefs" => ClojureOperator::WithRedefs,
        "with-out-str" => ClojureOperator::WithOutStr,
        "when-let" => ClojureOperator::WhenLet,
        "when-some" => ClojureOperator::WhenSome,
        "when-first" => ClojureOperator::WhenFirst,
        "if-let" => ClojureOperator::IfLet,
        "if-some" => ClojureOperator::IfSome,
        "doseq" => ClojureOperator::Doseq,
        "dotimes" => ClojureOperator::Dotimes,
        "for" => ClojureOperator::For,
        "fn" | "fn*" => ClojureOperator::Fn,
        "reify" => ClojureOperator::Reify,
        "extend-type" => ClojureOperator::ExtendType,
        "extend-protocol" => ClojureOperator::ExtendProtocol,
        "proxy" => ClojureOperator::Proxy,

        "if" => ClojureOperator::If,
        "if-not" => ClojureOperator::IfNot,
        "when" => ClojureOperator::When,
        "when-not" => ClojureOperator::WhenNot,
        "cond" => ClojureOperator::Cond,
        "condp" => ClojureOperator::Condp,
        "case" => ClojureOperator::Case,
        "do" => ClojureOperator::Do,
        "doto" => ClojureOperator::Doto,
        "try" => ClojureOperator::Try,
        "catch" => ClojureOperator::Catch,
        "finally" => ClojureOperator::Finally,
        "while" => ClojureOperator::While,
        "locking" => ClojureOperator::Locking,
        "comment" => ClojureOperator::Comment,

        "->" => ClojureOperator::Thread,
        "->>" => ClojureOperator::ThreadLast,
        "as->" => ClojureOperator::AsThread,
        "some->" => ClojureOperator::SomeThread,
        "some->>" => ClojureOperator::SomeThreadLast,
        "cond->" => ClojureOperator::CondThread,
        "cond->>" => ClojureOperator::CondThreadLast,
        _ => return None,
    })
}
