use super::ClojureOperator;

/// The canonical line layout for a Clojure form that does not fit on one line.
///
/// These follow the community indentation conventions that `cljfmt` and
/// `clojure-mode` implement. They are expressed as layouts rather than as
/// `cljfmt`'s numeric `:indent` specifiers because this crate's formatter is a
/// canonical printer: it decides *where the line breaks go*, not merely how
/// far to indent lines the author already broke.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClojureIndentStyle {
    /// `(defn name [params]` on the head line, body indented.
    ///
    /// A docstring or attribute map between the name and the parameter vector
    /// moves the parameter vector onto its own line, and a multi-arity form
    /// puts each `([params] body)` clause on its own line.
    Definition,
    /// `(fn [params]` or `(fn name [params]` on the head line, body indented.
    Function,
    /// `(let [bindings]` on the head line with one binding pair per line inside
    /// the vector, then body forms indented.
    BindingVector,
    /// `(-> value` on the head line, then one threading step per line.
    ///
    /// The payload is how many children share the head line, which is 2 for
    /// `as->` (the value and the name) and 1 for every other threading macro.
    Threading(usize),
    /// One test/result pair per line.
    ///
    /// The payload is how many children precede the clauses and stay on the
    /// head line: 0 for `cond`, 1 for `case`, 2 for `condp`.
    PairClauses(usize),
    /// The head plus this many children share the head line; the rest are
    /// indented one per line.
    Body(usize),
    /// The head is alone on its line and every child is indented below it.
    HeadBody,
    /// An ordinary call with no special layout.
    Call,
}

impl ClojureOperator {
    /// Returns the canonical line layout for this operator.
    #[must_use]
    pub const fn indent_style(self) -> ClojureIndentStyle {
        match self {
            Self::Defn | Self::DefnPrivate | Self::Defmacro => ClojureIndentStyle::Definition,
            Self::Fn => ClojureIndentStyle::Function,

            Self::Let
            | Self::Letfn
            | Self::Loop
            | Self::Binding
            | Self::WithLocalVars
            | Self::WithOpen
            | Self::WithRedefs
            | Self::WhenLet
            | Self::WhenSome
            | Self::WhenFirst
            | Self::IfLet
            | Self::IfSome
            | Self::Doseq
            | Self::Dotimes
            | Self::For => ClojureIndentStyle::BindingVector,

            Self::Thread
            | Self::ThreadLast
            | Self::SomeThread
            | Self::SomeThreadLast
            | Self::CondThread
            | Self::CondThreadLast => ClojureIndentStyle::Threading(1),
            // `(as-> value name step ...)` keeps the rebinding name with the
            // value, so the steps below it line up the same way as `->`.
            Self::AsThread => ClojureIndentStyle::Threading(2),

            Self::Cond => ClojureIndentStyle::PairClauses(0),
            Self::Case => ClojureIndentStyle::PairClauses(1),
            Self::Condp => ClojureIndentStyle::PairClauses(2),

            Self::Ns
            | Self::Def
            | Self::Defonce
            | Self::Defmulti
            | Self::Defprotocol
            | Self::Definterface
            | Self::Deftest
            | Self::Testing
            | Self::ExtendType
            | Self::ExtendProtocol
            | Self::Doto
            | Self::If
            | Self::IfNot
            | Self::When
            | Self::WhenNot
            | Self::While
            | Self::Locking => ClojureIndentStyle::Body(1),

            Self::Defrecord | Self::Deftype | Self::Catch | Self::Proxy => {
                ClojureIndentStyle::Body(2)
            }
            // `(defmethod name dispatch-value [params] body)` keeps the head,
            // the multimethod name, the dispatch value, and the parameter
            // vector together, so the body starts at the fourth child.
            Self::Defmethod => ClojureIndentStyle::Body(3),

            Self::Do
            | Self::Try
            | Self::Finally
            | Self::Comment
            | Self::Reify
            | Self::WithOutStr => ClojureIndentStyle::HeadBody,

            Self::InNs | Self::Declare | Self::Defstruct | Self::UseFixtures => {
                ClojureIndentStyle::Call
            }
        }
    }
}

/// Returns the canonical line layout for a Clojure list head.
#[must_use]
pub fn clojure_indent_style_for_head(head: &str) -> ClojureIndentStyle {
    ClojureOperator::from_head(head).map_or(ClojureIndentStyle::Call, ClojureOperator::indent_style)
}
