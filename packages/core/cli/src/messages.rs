//! The messages that get translated, and the ones that deliberately do not.
//!
//! The line is drawn at *who reads it*. A repair suggestion, a gate message,
//! and an error category are read by a person deciding what to do next, and a
//! person reading them in their own language decides faster. A finding's
//! `kind`, a rule's name, an error `code`, and every JSON key are read by a
//! program matching on them; translating those would break every consumer to
//! help nobody, so they stay in English in every language.
//!
//! Scoped deliberately narrow. `output.language` is documented as covering
//! diagnostics and gate messages, and the 275 reports' payloads are documented
//! as staying English. A half-translated report is worse than an untranslated
//! one, because a reader cannot tell which half they are looking at.

use crate::runtime::Language;

/// A message that has a translation.
///
/// An enum rather than a string key so a message added on one side and not the
/// other fails to compile rather than falling back silently at run time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Message {
    // --- the error rendering ---
    ErrorPrefix,
    RepairPrefix,
    // --- run controls ---
    DryRunSuppressedWrite,
    // --- the configuration ---
    ConfigIgnored,
    ConfigHasErrors,
    ConfigKeysNotApplied,
    // --- the failure categories, as a reader sees them ---
    CategoryArgument,
    CategorySelection,
    CategoryInput,
    CategoryRefusal,
    CategoryEnvironment,
    CategoryGate,
    CategoryInternal,
    // --- repair suggestions ---
    RepairNameTheSource,
    RepairListPaths,
    RepairSelectByOffset,
    RepairPathShape,
    RepairCheckLength,
    RepairSelectByPath,
    RepairRereadFile,
    RepairOneSelector,
    RepairAddFile,
    RepairDropWrite,
    RepairGetParseOffset,
    RepairCloseLists,
    RepairConfirmSelection,
    RepairDifferentShape,
    RepairSymbolShape,
    RepairSmallerInput,
    RepairWriteTargetShape,
    RepairNothingWritten,
    RepairReportDefect,
    RepairRegeneratePlan,
    RepairBoundTheWalk,
    RepairConfigureExcludes,
    RepairPartialWrite,
    RepairBackupLeftover,
    RepairReadTheReport,
    RepairRaiseBudget,
    RepairConvertEncoding,
    RepairSpanBoundaries,
    RepairDropDryRun,
    RepairUsePreviewFlag,
    RepairDropEncodingForWrite,
    RepairKillRingIndex,
    RepairNarrowSelector,
    RepairWidenSelector,
    RepairShowMatches,
    RepairPatternSyntax,
    RepairRerunWithoutAll,
    RepairSelectorSyntax,
    RepairOneInputSelector,
    RepairGlobSyntax,
    RepairNeedRepository,
    RepairNoInputsProduced,
    RepairArchiveDestination,
}

impl Message {
    /// This message in `language`.
    #[must_use]
    pub const fn text(self, language: Language) -> &'static str {
        match language {
            Language::English => self.english(),
            Language::Japanese => self.japanese(),
        }
    }

    #[must_use]
    pub const fn english(self) -> &'static str {
        match self {
            Self::ErrorPrefix => "Error",
            Self::RepairPrefix => "try",
            Self::DryRunSuppressedWrite => "--dry-run suppressed --write; nothing will be written.",
            Self::ConfigIgnored => "ignoring the configuration",
            Self::ConfigHasErrors => {
                "the configuration has errors and was not applied; \
                 run `paredit config check` for the details"
            }
            Self::ConfigKeysNotApplied => {
                "these configured keys do not combine with the given flags and were not applied"
            }
            Self::CategoryArgument => "the command line does not describe a runnable request",
            Self::CategorySelection => "the selection did not resolve",
            Self::CategoryInput => "the input is not what the operation needs",
            Self::CategoryRefusal => "declined for safety",
            Self::CategoryEnvironment => "the filesystem or environment failed",
            Self::CategoryGate => "a requested gate tripped after the report was printed",
            Self::CategoryInternal => "unclassified; a defect in this tool",
            Self::RepairNameTheSource => "name the source with --file <path>, or pipe it on stdin",
            Self::RepairListPaths => {
                "list the paths this document actually has, then select one of them"
            }
            Self::RepairSelectByOffset => {
                "select by byte offset instead of by path, with --at <offset>"
            }
            Self::RepairPathShape => {
                "a path is dot-separated child indexes counted from zero, such as 0.2.1"
            }
            Self::RepairCheckLength => {
                "check the document's length and structure before choosing an offset"
            }
            Self::RepairKillRingIndex => {
                "read the ring file, or pass --index 0 for the most recent entry"
            }
            Self::RepairSelectByPath => "select by path instead of by offset, with --path <a.b.c>",
            Self::RepairRereadFile => {
                "the file changed since it was read; read it again and redo the operation"
            }
            Self::RepairOneSelector => "pass --path or --at, not both",
            Self::RepairAddFile => "add --file <path>, which is what --write writes to",
            Self::RepairDropWrite => "or drop --write and take the rewritten document from stdout",
            Self::RepairGetParseOffset => "get the byte offset where the parse failed",
            Self::RepairCloseLists => "if the only problem is unclosed lists, this can close them",
            Self::RepairConfirmSelection => {
                "confirm what the selection actually points at before retrying"
            }
            Self::RepairDifferentShape => {
                "this operation needs a different shape of form; select its parent or a sibling"
            }
            Self::RepairSymbolShape => {
                "a symbol may not be empty or contain whitespace or reader delimiters"
            }
            Self::RepairSmallerInput => "split the input, or point the command at a smaller file",
            Self::RepairWriteTargetShape => {
                "the write target must be a regular file this process may replace; resolve symlinks and check the parent directory's permissions"
            }
            Self::RepairNothingWritten => {
                "nothing was written. See what the rewrite would have produced"
            }
            Self::RepairReportDefect => {
                "this is a defect in the transform for this input; please report it with the source that triggered it"
            }
            Self::RepairRegeneratePlan => {
                "the edit spans do not fit the current source; regenerate the plan against it"
            }
            Self::RepairBoundTheWalk => {
                "bound the walk with --max-depth, or narrow the roots you passed"
            }
            Self::RepairConfigureExcludes => {
                "[paths] in paredit.toml can exclude directories for every command"
            }
            Self::RepairPartialWrite => {
                "a write failed and undoing it also failed; the working tree may be partly written, so check it before retrying"
            }
            Self::RepairBackupLeftover => {
                "the writes did land; only removing the backup did not. Do not redo the operation, remove the leftover backup file"
            }
            Self::RepairReadTheReport => {
                "the report was printed before this; the gate named in the message is what tripped"
            }
            Self::RepairRaiseBudget => {
                "nothing is wrong with the input; re-run with a larger --timeout-ms, or without one"
            }
            Self::RepairConvertEncoding => {
                "pass --encoding <label> (e.g. shift_jis, euc-jp) naming the file's actual encoding — or, if one was already given, its label is wrong — or convert the file to UTF-8 first"
            }
            Self::RepairSpanBoundaries => {
                "the span does not lie on character boundaries of the current source"
            }
            Self::RepairDropDryRun => {
                "drop --dry-run (or unset PAREDIT_DRY_RUN) to let the write happen"
            }
            Self::RepairUsePreviewFlag => {
                "or use this command's own preview: --diff on an edit, --fix --diff on lint"
            }
            Self::RepairDropEncodingForWrite => {
                "drop --encoding to write in UTF-8, or drop --write and pipe stdout through your own re-encoder"
            }
            Self::RepairNarrowSelector => {
                "narrow the selector, or pass --all to act on every match"
            }
            Self::RepairWidenSelector => {
                "nothing matched; widen the selector or check the spelling of the name"
            }
            Self::RepairShowMatches => "see what the selector matches before acting on it",
            Self::RepairPatternSyntax => {
                "a pattern is an s-expression with ?name holes and ... for the remaining forms"
            }
            Self::RepairRerunWithoutAll => "re-run without --all and act on one match at a time",
            Self::RepairSelectorSyntax => {
                "the selector is not well formed; `paredit inspect capabilities` lists each selector's spelling"
            }
            Self::RepairOneInputSelector => {
                "pass one input selector only: each of --since, --from-git, --from-manifest, --from-archive and --paths-from replaces the whole file set"
            }
            Self::RepairGlobSyntax => {
                "a glob matches path segments with * and **, and a character class with [...]; quote it so the shell does not expand it first"
            }
            Self::RepairNeedRepository => {
                "run inside the git repository that contains the scanned roots, or drop --since and name the files"
            }
            Self::RepairNoInputsProduced => {
                "the selector resolved to no file inside the scanned roots; widen the roots or check what it produced"
            }
            Self::RepairArchiveDestination => {
                "--from-archive needs somewhere to extract to; add --extract-to <DIR>"
            }
        }
    }

    #[must_use]
    pub const fn japanese(self) -> &'static str {
        match self {
            Self::ErrorPrefix => "エラー",
            Self::RepairPrefix => "対処",
            Self::DryRunSuppressedWrite => {
                "--dry-run により --write を無効化しました。ファイルは変更されません。"
            }
            Self::ConfigIgnored => "設定を無視します",
            Self::ConfigHasErrors => {
                "設定にエラーがあるため適用しませんでした。詳細は \
                 `paredit config check` を実行してください"
            }
            Self::ConfigKeysNotApplied => {
                "次の設定キーは指定されたフラグと併用できないため適用しませんでした"
            }
            Self::CategoryArgument => "コマンドラインが実行可能な要求になっていません",
            Self::CategorySelection => "選択が解決できませんでした",
            Self::CategoryInput => "入力がこの操作の求める形ではありません",
            Self::CategoryRefusal => "安全のため実行を見送りました",
            Self::CategoryEnvironment => "ファイルシステムまたは環境の障害です",
            Self::CategoryGate => "レポート出力後に、要求されたゲートが作動しました",
            Self::CategoryInternal => "分類できませんでした。本ツール側の不具合です",
            Self::RepairNameTheSource => {
                "--file <path> で入力を指定するか、標準入力に流し込んでください"
            }
            Self::RepairListPaths => "この文書に実在するパスを一覧し、その中から選び直してください",
            Self::RepairSelectByOffset => {
                "パスではなくバイト位置で選択してください（--at <offset>）"
            }
            Self::RepairPathShape => {
                "パスは 0 から数えた子の添字をドットでつないだものです（例: 0.2.1）"
            }
            Self::RepairCheckLength => "位置を決める前に、文書の長さと構造を確認してください",
            Self::RepairKillRingIndex => {
                "kill ring ファイルを確認するか、最新の項目を指す --index 0 を渡してください"
            }
            Self::RepairSelectByPath => {
                "バイト位置ではなくパスで選択してください（--path <a.b.c>）"
            }
            Self::RepairRereadFile => {
                "読み込み後にファイルが変化しています。読み直して操作をやり直してください"
            }
            Self::RepairOneSelector => "--path と --at のどちらか一方だけを指定してください",
            Self::RepairAddFile => "--write の書き込み先である --file <path> を指定してください",
            Self::RepairDropWrite => {
                "あるいは --write を外し、書き換え後の文書を標準出力から受け取ってください"
            }
            Self::RepairGetParseOffset => "解析が失敗したバイト位置を調べてください",
            Self::RepairCloseLists => "未閉じの括弧だけが原因なら、これで閉じられます",
            Self::RepairConfirmSelection => {
                "再実行の前に、選択が実際に何を指しているか確認してください"
            }
            Self::RepairDifferentShape => {
                "この操作は別の形のフォームを必要とします。親か兄弟を選び直してください"
            }
            Self::RepairSymbolShape => {
                "シンボルは空にできず、空白やリーダ区切り文字も含められません"
            }
            Self::RepairSmallerInput => "入力を分割するか、より小さいファイルを指定してください",
            Self::RepairWriteTargetShape => {
                "書き込み先はこのプロセスが置き換えられる通常ファイルである必要があります。シンボリックリンクを解決し、親ディレクトリの権限を確認してください"
            }
            Self::RepairNothingWritten => {
                "何も書き込まれていません。書き換え結果がどうなるはずだったかを確認してください"
            }
            Self::RepairReportDefect => {
                "この入力に対する変換の不具合です。再現したソースを添えて報告してください"
            }
            Self::RepairRegeneratePlan => {
                "編集範囲が現在のソースに一致しません。現在のソースに対して計画を作り直してください"
            }
            Self::RepairBoundTheWalk => {
                "--max-depth で探索を制限するか、指定したルートを絞ってください"
            }
            Self::RepairConfigureExcludes => {
                "paredit.toml の [paths] で、全コマンド共通のディレクトリ除外を設定できます"
            }
            Self::RepairPartialWrite => {
                "書き込みに失敗し、取り消しにも失敗しました。作業ツリーが中途半端な状態の可能性があるため、再実行の前に確認してください"
            }
            Self::RepairBackupLeftover => {
                "書き込み自体は成功し、バックアップの削除だけが失敗しました。操作をやり直さず、残ったバックアップファイルを削除してください"
            }
            Self::RepairReadTheReport => {
                "レポートはこの前に出力済みです。メッセージが示すゲートが作動しました"
            }
            Self::RepairRaiseBudget => {
                "入力に問題はありません。--timeout-ms を大きくするか、指定せずに再実行してください"
            }
            Self::RepairConvertEncoding => {
                "--encoding <label>（例: shift_jis、euc-jp）でファイルの実際の文字コードを指定してください。指定済みならその値が誤っている可能性があります。あるいは先にファイルを UTF-8 に変換してください"
            }
            Self::RepairSpanBoundaries => "範囲が現在のソースの文字境界に一致していません",
            Self::RepairDropDryRun => {
                "書き込みを行うには --dry-run を外してください（PAREDIT_DRY_RUN も解除）"
            }
            Self::RepairUsePreviewFlag => {
                "あるいは各コマンドのプレビューを使ってください（編集系は --diff、lint は --fix --diff）"
            }
            Self::RepairDropEncodingForWrite => {
                "UTF-8 で書き込むには --encoding を外してください。あるいは --write を外し、標準出力を自分で再エンコードしてください"
            }
            Self::RepairNarrowSelector => {
                "セレクタを絞り込むか、全一致に対して実行するなら --all を指定してください"
            }
            Self::RepairWidenSelector => {
                "何も一致しませんでした。セレクタを広げるか、名前の綴りを確認してください"
            }
            Self::RepairShowMatches => "実行前に、そのセレクタが何に一致するかを確認してください",
            Self::RepairPatternSyntax => {
                "パターンは S 式で、穴は ?name、残りのフォームは ... で書きます"
            }
            Self::RepairRerunWithoutAll => "--all を外し、一致を 1 つずつ処理してください",
            Self::RepairSelectorSyntax => {
                "セレクタの書式が正しくありません。各セレクタの綴りは `paredit inspect capabilities` で確認できます"
            }
            Self::RepairOneInputSelector => {
                "入力セレクタは 1 つだけ指定してください。--since / --from-git / --from-manifest / --from-archive / --paths-from はいずれもファイル集合そのものを置き換えます"
            }
            Self::RepairGlobSyntax => {
                "グロブは * と ** でパス要素に、[...] で文字クラスに一致します。シェルが先に展開しないよう引用してください"
            }
            Self::RepairNeedRepository => {
                "走査対象を含む git リポジトリ内で実行するか、--since をやめてファイルを直接指定してください"
            }
            Self::RepairNoInputsProduced => {
                "走査対象の中に該当ファイルがありませんでした。ルートを広げるか、セレクタの出力を確認してください"
            }
            Self::RepairArchiveDestination => {
                "--from-archive には展開先が必要です。--extract-to <DIR> を指定してください"
            }
        }
    }

    /// Every message, so a contract test can check both sides are complete.
    pub const ALL: [Self; 55] = [
        Self::ErrorPrefix,
        Self::RepairPrefix,
        Self::DryRunSuppressedWrite,
        Self::ConfigIgnored,
        Self::ConfigHasErrors,
        Self::ConfigKeysNotApplied,
        Self::CategoryArgument,
        Self::CategorySelection,
        Self::CategoryInput,
        Self::CategoryRefusal,
        Self::CategoryEnvironment,
        Self::CategoryGate,
        Self::CategoryInternal,
        Self::RepairNameTheSource,
        Self::RepairListPaths,
        Self::RepairSelectByOffset,
        Self::RepairPathShape,
        Self::RepairCheckLength,
        Self::RepairSelectByPath,
        Self::RepairRereadFile,
        Self::RepairOneSelector,
        Self::RepairAddFile,
        Self::RepairDropWrite,
        Self::RepairGetParseOffset,
        Self::RepairCloseLists,
        Self::RepairConfirmSelection,
        Self::RepairDifferentShape,
        Self::RepairSymbolShape,
        Self::RepairSmallerInput,
        Self::RepairWriteTargetShape,
        Self::RepairNothingWritten,
        Self::RepairReportDefect,
        Self::RepairRegeneratePlan,
        Self::RepairBoundTheWalk,
        Self::RepairConfigureExcludes,
        Self::RepairPartialWrite,
        Self::RepairBackupLeftover,
        Self::RepairReadTheReport,
        Self::RepairConvertEncoding,
        Self::RepairSpanBoundaries,
        Self::RepairDropDryRun,
        Self::RepairUsePreviewFlag,
        Self::RepairDropEncodingForWrite,
        Self::RepairNarrowSelector,
        Self::RepairWidenSelector,
        Self::RepairShowMatches,
        Self::RepairPatternSyntax,
        Self::RepairRerunWithoutAll,
        Self::RepairSelectorSyntax,
        Self::RepairOneInputSelector,
        Self::RepairGlobSyntax,
        Self::RepairNeedRepository,
        Self::RepairNoInputsProduced,
        Self::RepairArchiveDestination,
        Self::RepairKillRingIndex,
    ];
}

/// This message in the language the run is configured for.
#[must_use]
pub fn say(message: Message) -> &'static str {
    message.text(crate::runtime::current().language)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    /// A message with no translation would fall back to English invisibly,
    /// which reads as "this one is untranslatable" rather than "we forgot".
    #[test]
    fn every_message_has_a_distinct_string_in_both_languages() {
        for language in [Language::English, Language::Japanese] {
            let texts: BTreeSet<&str> = Message::ALL
                .iter()
                .map(|message| message.text(language))
                .collect();
            assert_eq!(
                texts.len(),
                Message::ALL.len(),
                "two messages share a string in {}",
                language.label()
            );
        }
    }

    #[test]
    fn no_message_is_left_in_english_on_the_japanese_side() {
        for message in Message::ALL {
            assert_ne!(
                message.english(),
                message.japanese(),
                "{message:?} was not translated"
            );
            assert!(
                message.japanese().chars().any(|c| c as u32 > 0x2500),
                "{message:?}'s Japanese is not Japanese"
            );
        }
    }

    /// The translated text may reference a command, and a command name is an
    /// identifier: translating one would produce advice that does not run.
    #[test]
    fn a_command_named_in_a_message_is_the_same_in_both_languages() {
        assert!(
            Message::ConfigHasErrors
                .english()
                .contains("paredit config check")
        );
        assert!(
            Message::ConfigHasErrors
                .japanese()
                .contains("paredit config check")
        );
        assert!(
            Message::DryRunSuppressedWrite
                .english()
                .contains("--dry-run")
        );
        assert!(
            Message::DryRunSuppressedWrite
                .japanese()
                .contains("--dry-run")
        );
    }

    /// The default, and what every test in this workspace runs under.
    #[test]
    fn english_is_what_an_uninstalled_runtime_says() {
        assert_eq!(say(Message::RepairPrefix), "try");
    }
}
