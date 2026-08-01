//! CommentedReplTranscript (a comment block shaped like a pasted REPL session) detection.

pub use crate::commented_repl_transcript::domain::{
    CommentedReplTranscriptItem, CommentedReplTranscriptPolicy,
    CommentedReplTranscriptPolicyOptions, CommentedReplTranscriptSummary,
    collect_commented_repl_transcript, evaluate_commented_repl_transcript_policy,
    summarize_commented_repl_transcript,
};
