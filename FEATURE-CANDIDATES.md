# 機能追加候補カタログ

対象: `nerima-lisp/paredit-cli` v1.2.1
目的: 「次に何を作るか」を選ぶための候補の網羅。採否は未決。
候補数: **314**（A〜U の 21 セクション）

---

## 0. 現状の輪郭（候補を読む前提）

実測ベースの現状把握。候補の価値はここからの差分で決まる。

| 軸 | 現状 |
| --- | --- |
| コマンド数 | 約 275（`inspect` 130 / `edit` 20 / `refactor` 80 / lint ルール 170 弱） |
| 名前空間 | `inspect`（読み取り専用） / `edit`（単一形変換） / `refactor`（plan→preview→verify→apply） |
| 方言 | 宣言上 10 方言。解析の深さは **CL >> Elisp > その他 7 方言**（`dialect/capability.rs` は多くの方言で「定義ヘッドの名前一覧」止まり） |
| lint | within-file の論理バグ中心。`--sarif` `--github` `--fix` `--fix-plan` `--baseline` `--suppressions` を既に持つ |
| 安全機構 | preview manifest + blake3 ハッシュガード、`refactor verify`、`mutation_safety` |
| 出力 | `text` / `json` の 2 形式（lint のみ SARIF・GitHub annotations を追加で持つ） |
| format | フラグは `--indent` `--write` `--diff` `--dialect` の 4 つのみ。`MAX_INLINE_WIDTH` はハードコード定数 |
| workspace 探索 | `max_depth` / `include_hidden` / symlink スキップ / 生成物パス除外。**`.gitignore` 非対応** |
| 配布 | Nix flake / Cachix / GitHub Action（lint・format・fix の 3 モード）/ Claude skill / Git タグ（crates.io publish は方針上なし） |
| **存在しないもの** | LSP、MCP、設定ファイル、カスタムルール機構、watch、WASM、構造 diff、エディタ拡張、`format --check` |

### 掘って分かった 4 つの構造的ギャップ

1. **未露出の実装在庫** — `packages/core/semantics/src/semantics/` に `typing/`（`inference`・`narrowing`・`declarations`・`calls`）と `value/`（`folding`・`propagation`・`literal_reader`）が実装済みだが、対応する CLI コマンドが無い。`common_lisp/` 配下の `reader_condition` / `reader_label` / `reader_literal` も同様。*作らずに出すだけ*で機能になる。
2. **露出面が CLI ひとつだけ** — 275 コマンド分の解析資産が、CLI という単一の口からしか出ていない。
3. **「見つけるが直せない」非対称** — `inspect unused-parameters` はあるが `(declare (ignore x))` を挿入する手段が無い、等。レポートと変換の対応表に穴がある。
4. **意味解析層が CL 専用であることがコードに明記されている** — `application/usecase/semantic_coverage.rs` のドキュメントコメント曰く「`build_binding_table` と `build_value_table` は Common Lisp 以外の全方言で空テーブルを返す」。「10 方言対応」は構文層の話で、意味層は 1 方言。A 群と R 群の根拠。

---

## A. 方言の深さ（22 件）

10 方言を謳いながら実質の解析深度は CL に集中している。「対応方言を増やす」より「宣言済み方言を深くする」ほうが約束と実装の乖離を埋める。

### A-1. Emacs Lisp

| # | 候補 |
| --- | --- |
| A1 | `cl-defstruct` のスロット・アクセサ解析（CL の `defstruct` 相当を Elisp 側にも） |
| A2 | `pcase` / `pcase-let` のパターン束縛解析 |
| A3 | `cl-loop` の節解析 |
| A4 | `defcustom` の `:type` 指定と初期値の整合検証 |
| A5 | `require` / `provide` からのファイル依存グラフ |
| A6 | `;;;###autoload` cookie の抽出と検証 |
| A7 | `lexical-binding: t` ヘッダの有無検証（動的束縛の落とし穴） |
| A8 | `defvar` 無しの free variable 参照検出（byte-compiler 警告相当） |
| A9 | Elisp 版 lint ルール群（`elisp-lint` / `package-lint` 相当のサブセット） |

### A-2. Scheme / Racket

| # | 候補 |
| --- | --- |
| A10 | `define-values` / `let-values` / `letrec` の束縛解析 |
| A11 | named `let` のループ変数解析 |
| A12 | `syntax-rules` / `syntax-case` のパターン変数解析 |
| A13 | R7RS `define-library` の `import`/`export` 依存グラフ |
| A14 | Racket `#lang` 行による方言判定と言語別ルール |
| A15 | Racket の contract（`provide (contract-out ...)`）解析 |

### A-3. Clojure

| # | 候補 |
| --- | --- |
| A16 | `ns` の `:require` / `:refer` / `:as` / `:import` 解析 |
| A17 | destructuring 束縛（`{:keys [...]}`、ベクタ分解）のスコープ解析 |
| A18 | `defprotocol` / `defrecord` / `deftype` の重複・循環検出 |
| A19 | `->` / `->>` / `as->` / `some->` に沿った threading 変換（既存 `thread-expression` の Clojure 慣用化） |
| A20 | Clojure 版 lint ルール群（`clj-kondo` 相当のサブセット） |

### A-4. その他の方言と横断

| # | 候補 |
| --- | --- |
| A21 | Fennel / Janet / Hy / Carp / LFE の底上げ — 最低限 rename とスコープ解析を通す |
| A22 | 方言ケイパビリティ行列の機械可読化 — 「どのコマンドがどの方言でどこまで動くか」を `inspect capabilities` に含め、エージェントの試行錯誤を削る |

---

## B. `inspect` — 新レポート（36 件）

### B-1. 既存の未露出資産を出すだけ（低コスト・高価値）

| # | 候補 | 概要 |
| --- | --- | --- |
| B1 | `inspect types` | `semantics/typing` を露出。`declare`/`the`/`declaim` 由来の型情報と、宣言・使用の矛盾 |
| B2 | `inspect narrowing` | 条件分岐による型の絞り込み結果（`typing/narrowing`） |
| B3 | `inspect constants` | `semantics/value/folding` を露出。定数畳み込み可能箇所 |
| B4 | `inspect value-propagation` | `value/propagation` を露出。定数伝播の到達結果 |
| B5 | `inspect effects` | 副作用推定（純粋関数の判定）。**多くのリファクタ安全判定の共通基盤**になるので優先度が高い |

### B-2. Lisp 固有の解析

| # | 候補 | 概要 |
| --- | --- | --- |
| B6 | `inspect macro-expansion` | `defmacro` テンプレートの局所展開シミュレーション（`macrolet` / `define-symbol-macro` は既に扱えている） |
| B7 | `inspect macro-hygiene` | マクロ本体で `gensym` を使うべき箇所（変数捕捉の危険）を検出 |
| B8 | `inspect loop` | CL `loop` は独自文法で現状ほぼ未解析。節の検証と変数束縛の抽出 |
| B9 | `inspect format-directives` | `format` 制御文字列のディレクティブ数と実引数の照合。CL の頻出バグ |
| B10 | `inspect read-conditionals` | `#+`/`#-` フィーチャ式の一覧、feature 組合せごとの到達コード |
| B11 | `inspect read-time-eval` | `#.` の使用箇所（ビルド再現性・セキュリティの観点） |
| B12 | `inspect circular-literals` | `#1=` / `#1#` の循環データリテラル |
| B13 | `inspect readtable-case` | シンボルの大文字小文字が readtable-case に依存する箇所 |
| B14 | `inspect package-locks` | CL 標準シンボルの再定義・シャドウ（処理系のパッケージロック違反） |
| B15 | `inspect method-combination` | `defmethod` の qualifier（`:before`/`:after`/`:around`）分布と、対応する primary の欠落 |
| B16 | `inspect class-hierarchy` | CLOS 継承ツリーとスロット継承の可視化（`class-cycles` の非循環版） |
| B17 | `inspect generic-dispatch` | `defgeneric` に対する `defmethod` の網羅性・特殊化の重なり |
| B18 | `inspect restarts` | `restart-case` / `handler-bind` の対応関係と、宣言されたが確立されない restart |

### B-3. 品質・メトリクス

| # | 候補 | 概要 |
| --- | --- | --- |
| B19 | `inspect docstrings` | docstring の欠落・書式・引数名との不一致 |
| B20 | `inspect todo` | TODO/FIXME/XXX を構造付きで抽出（コメントは trivia として既に保持している） |
| B21 | `inspect cohesion` | パッケージ単位の凝集度・結合度メトリクス |
| B22 | `inspect hotspots` | git churn × `complexity` でリファクタ優先度を算出 |
| B23 | `inspect duplication-ratio` | 既存 `duplicates` の集約統計（プロジェクト全体の重複率） |
| B24 | `inspect line-metrics` | 行長・ファイルサイズ・定義あたり行数などのスタイル系メトリクス |
| B25 | `inspect indentation` | canonical format との差分ではなく、Emacs/SLIME 慣習インデントからの逸脱 |
| B26 | `inspect debt-score` | 上記メトリクスを 1 スコアに集約し、履歴 JSON として蓄積・トレンド化 |

### B-4. API とプロジェクト構造

| # | 候補 | 概要 |
| --- | --- | --- |
| B27 | `inspect api-surface` | 公開 API（export シンボル + シグネチャ）のスナップショット出力 |
| B28 | `inspect api-diff` | 2 リビジョン間の公開 API 差分 → SemVer 判定支援。B27 の応用 |
| B29 | `inspect test-map` | `deftest` 等とテスト対象定義の対応付け、未テスト定義の列挙 |
| B30 | `inspect symbol-index` | プロジェクト全体のシンボル→定義位置インデックス。エディタ/エージェントのキャッシュ用 |
| B31 | `inspect keyword-arity` | `signature` の厳密版。`&key` / `&optional` / `&rest` まで含めた呼び出し検証 |
| B32 | `inspect unreachable-expressions` | `reachability` の式レベル版（`return-from` 後の死コードなど） |
| B33 | `inspect external-systems` | 依存している外部 ASDF システムの一覧（SBOM 的な用途） |
| B34 | `inspect licenses` | `defsystem` の `:license` を集約し互換性を検査 |
| B35 | `inspect serial-consistency` | ASDF `:serial t` と実際のファイル間依存の不整合 |
| B36 | findings への blame 付与 | 任意のレポートに最終更新者・日付を付ける共通オプション |

---

## C. lint — ルールとルール機構（30 件）

### C-1. 新ルールカテゴリ

現在の 170 ルールは「1 フォームで閉じる冗長性・論理バグ」に強く偏っている。以下は軸が違う。

| # | 候補 | 概要 |
| --- | --- | --- |
| C1 | パフォーマンス | `(length x)` を空判定に使う、ループ内 `append`、リスト末尾追加、`member`/`assoc` の線形探索の多用 |
| C2 | 割り当て | 不要な `copy-seq` / `copy-list`、ループ内での consing |
| C3 | 移植性 | 実装依存の使用検出（`sb-*` パッケージ、処理系拡張、`#+` 分岐の非対称） |
| C4 | 浮動小数 | `*read-default-float-format*` 依存、`=` による float 比較 |
| C5 | 並行性 | グローバル `defparameter` の実行時変更、非スレッドセーフなイディオム |
| C6 | セキュリティ | 信頼できない入力への `eval` / `read`（`*read-eval*`）、`run-program` の文字列連結 |
| C7 | 命名規約 | 述語の `-p`/`p`、破壊的操作の `n` 接頭、定数の `+x+`、スペシャル変数の `*x*`。既存 `naming` は kebab-case のみ |
| C8 | docstring 必須 | 公開定義に docstring が無い |
| C9 | 宣言の一貫性 | `declaim` / `declare` の重複・矛盾・スコープ外 |
| C10 | `defclass` オプション検証 | `:initarg`/`:accessor`/`:initform` の妥当性、`:reader` と `:writer` の非対称 |
| C11 | `defgeneric` / `defmethod` 整合 | lambda list の不一致、宣言されない generic への method |
| C12 | 条件系 | `error` に condition クラスでなく文字列を渡す、`handler-case` で `error` を握り潰す |
| C13 | ストリーム | `with-open-file` を使わない `open`、閉じ忘れ |
| C14 | プロジェクト横断 lint | 現状の lint は明示的に within-file。ファイルをまたぐ呼び出し規約の不一致など |

### C-2. ルール機構そのもの（影響が大きい）

| # | 候補 | 概要 |
| --- | --- | --- |
| C15 | **カスタムルール DSL** | S 式パターンマッチで `.paredit/rules/*.lisp` にルールを書けるようにする（ast-grep 相当）。170 ルールを 1 つずつ Rust で足す運用からの脱却 |
| C16 | ルールのテストハーネス | カスタムルールに対する期待入出力を宣言的に書ける |
| C17 | severity の設定可能化 | ルールごとの `--deny` / `--warn` 昇格・降格 |
| C18 | ルール引数 | 最大ネスト深さ、最大行数など閾値を設定可能に |
| C19 | deprecation ルール | 「この関数はもう使うな」をプロジェクト側で宣言できる |
| C20 | `--explain <rule>` | rustc 風のルール詳細説明（現状は 1 行の説明のみ） |
| C21 | auto-fix 率の底上げ | 現状 auto-fixable は一部。既存ルールの fix 実装を埋める地道な拡張 |
| C22 | fix の衝突解決 | 同一箇所に複数ルールの fix が当たる場合の優先順位と反復適用 |
| C23 | 安定 finding ID | findings に内容ベースの安定 ID を付け、行移動に強い baseline / 抑制を実現 |
| C24 | ルールのプロファイリング | `--timings` で遅いルールを特定（170 ルールの実行コスト可視化） |
| C25 | ルールのタグ付け | `category` に加え「自動修正可能」「破壊的」「実験的」等の直交タグ |
| C26 | 実験的ルールのオプトイン | 安定ルールと実験ルールの分離 |
| C27 | 抑制コメントの拡張 | 範囲抑制（`;; paredit:disable-next-form`）、理由必須モード |
| C28 | ルールドキュメントの自動生成 | ルール定義から個別ページを生成（現状 `commands.md` に 1 行ずつ） |
| C29 | 未使用抑制の自動削除 | `--report-unused-suppressions` はある → 削除まで自動化 |
| C30 | ルールセットのプリセット | `minimal` / `recommended` / `pedantic` の 3 段構え |

---

## D. `edit` / `refactor` — 新変換（44 件）

### D-1. 既存レポートと直結するもの（費用対効果が高い）

| # | 候補 | 概要 |
| --- | --- | --- |
| D1 | `add-ignore-declaration` | `inspect unused-parameters` の結果から `(declare (ignore x))` を自動挿入。**レポートはあるのに修正手段が無い**代表例 |
| D2 | `apply-fix-plan` | lint の `--fix-plan` を refactor manifest に流し込み、ハッシュガード付きで適用する統合 |
| D3 | `dedupe` | `duplicates` / `similarity` の結果から共通関数へ自動抽出（`replacement-plan` の一歩先） |
| D4 | `optimize-imports` | 未使用 import の削除と整列。`unused-exports` はあるが編集側が薄い |
| D5 | `remove-unreachable` | `reachability` / `unused-definitions` の結果を一括削除 |
| D6 | `fold-constants` | B3（定数畳み込みレポート）を編集として適用 |

### D-2. 関数・シグネチャ

| # | 候補 | 概要 |
| --- | --- | --- |
| D7 | `change-signature` | 現在 add/move/swap/reorder/remove に分かれているものを、宣言的な 1 コマンドに統合 |
| D8 | `convert-positional-to-keyword` | 位置引数を `&key` に昇格し、呼び出し側も更新 |
| D9 | `convert-keyword-to-positional` | 逆変換 |
| D10 | `add-optional-parameter` / `add-key-parameter` | 現状の `add-function-parameter` は位置引数中心 |
| D11 | `introduce-parameter` | ハードコード値を引数に昇格（`extract-constant` の呼び出し側版） |
| D12 | `curry` / `uncurry` | 部分適用の導入と解消 |
| D13 | `convert-function-to-macro` / 逆 | |

### D-3. CLOS / 型定義

| # | 候補 | 概要 |
| --- | --- | --- |
| D14 | `extract-method` | 選択式を `defmethod` に抽出 |
| D15 | `generate-defgeneric` | 既存 `defmethod` 群から `defgeneric` を生成 |
| D16 | `add-method` / `remove-method` | |
| D17 | `change-specializer` | method の特殊化を変更し、影響を検証 |
| D18 | `convert-defun-to-defmethod` / 逆 | |
| D19 | `add-slot` / `remove-slot` / `rename-slot` | `defclass` のスロット操作とアクセサ追従 |
| D20 | `convert-defstruct-to-defclass` | |
| D21 | `pull-up-slot` / `push-down-slot` | 継承階層でのスロット移動 |

### D-4. 制御構造・イディオム

| # | 候補 | 概要 |
| --- | --- | --- |
| D22 | 反復形の相互変換 | `loop` ↔ `dolist` / `dotimes` ↔ `mapcar` / `map` |
| D23 | `convert-cond-to-case` / 逆 | 既存の if/cond/when/unless 変換群の延長 |
| D24 | `convert-let-to-multiple-value-bind` / 逆 | `single-value-bind` lint と対になる |
| D25 | `convert-recursion-to-loop` | 末尾再帰のループ化 |
| D26 | `extract-macro` | 重複形をマクロへ抽出（`extract-function` の macro 版） |
| D27 | `wrap-with-handler-case` | エラー処理イディオムの挿入 |
| D28 | `wrap-with-open-file` | リソース管理イディオムの挿入 |
| D29 | `introduce-guard-clause` | ネストした `if` を早期リターンに |
| D30 | `merge-conditionals` | 連続する同一条件の `when` を統合 |

### D-5. ファイル・パッケージ・ビルド

| # | 候補 | 概要 |
| --- | --- | --- |
| D31 | `rename-file` | ファイル名変更に `in-package` / `defsystem` の記述を追従させる |
| D32 | `add-system-dependency` / `remove-system-dependency` | ASDF `defsystem` の編集 |
| D33 | `add-component` / `remove-component` | `defsystem` の `:components` 編集。D31 と組で必要 |
| D34 | `organize-file` | 定義順・セクションコメントを規約に沿って整える（`sort-definitions` の拡張） |
| D35 | `extract-package` | 定義群を新パッケージに切り出し、`defpackage` と `defsystem` を生成 |
| D36 | `merge-packages` | 2 つのパッケージを統合 |

### D-6. 細かい編集操作

| # | 候補 | 概要 |
| --- | --- | --- |
| D37 | `add-docstring` | 定義に docstring テンプレートを挿入 |
| D38 | `toggle-reader-conditional` | `#+` / `#-` の付け外し |
| D39 | `normalize-quotes` | `(quote x)` ↔ `'x`、`#'f` ↔ `(function f)` |
| D40 | `comment` / `uncomment` | 構造単位でのコメント化（`#\|...\|#` と `;;` の切替） |
| D41 | 構造を壊さない行操作 | paredit 由来で未実装の `kill-line` / `reindent-defun` 相当 |
| D42 | `copy` / `duplicate` | 選択形の複製（`kill` の非破壊版） |
| D43 | 複数選択への同一編集 | `--path` を複数受け取り、1 回の実行で同じ変換を適用 |
| D44 | `edit` のスクリプト化 | 変換列を 1 ファイルに書いて一括実行（`sed -f` 相当）。エージェントの往復回数を削る |

---

## E. format — 印字系（16 件）

`format` のフラグは `--indent` / `--write` / `--diff` / `--dialect` の 4 つのみ。`MAX_INLINE_WIDTH` はハードコード定数で、`styles.rs` に bindings / clauses / definitions / loops / general の分類がありながら外から触れない。

| # | 候補 | 概要 |
| --- | --- | --- |
| E1 | `format --check` | CI 用に exit code だけを返す。現状は `--diff` の出力が空かどうかで判定するしかない |
| E2 | `--max-width` の露出 | `MAX_INLINE_WIDTH` を設定可能に |
| E3 | 演算子ごとのインデント規則 | `styles.rs` の分類をユーザ定義で拡張（`(loop ...)` や自作マクロ） |
| E4 | Emacs `lisp-indent-function` 互換モード | 既存プロジェクトの慣習に合わせる |
| E5 | cljfmt 互換モード | Clojure プロジェクトへの導入障壁を下げる |
| E6 | 空行ポリシー | トップレベル定義間の空行数を正規化 |
| E7 | コメント整列 | 行末 `;` コメントの列揃え |
| E8 | 束縛列の縦揃え（`align`） | `let` の初期化式を揃える |
| E9 | 末尾空白・改行コード正規化 | CRLF/LF、ファイル末尾改行 |
| E10 | タブ / スペース変換 | |
| E11 | 部分フォーマット | `--path` / `--at` で選択範囲のみ整形 |
| E12 | `--range` | LSP の range formatting に対応する行範囲指定 |
| E13 | 差分件数の JSON 出力 | CI での閾値判定に使える |
| E14 | フォーマット済みマーカーの尊重 | `;; paredit:format off` 区間のスキップ |
| E15 | 読みやすさ優先モード | 行長を超える場合の折り返し戦略を選択可能に |
| E16 | フォーマットの冪等性テスト | 2 回かけて同一になることを property test で保証 |

---

## F. workspace 探索・入力（12 件）

| # | 候補 | 概要 |
| --- | --- | --- |
| F1 | `.gitignore` 尊重 | 現状非対応。生成物パス除外は独自ロジックのみ |
| F2 | `.pareditignore` | ツール固有の除外 |
| F3 | `--include` / `--exclude` の glob 対応 | `WorkspaceOptions` の `exclude` は `Vec<PathBuf>` でパス完全一致。`include_unknown` / `include_hidden` / `include_generated` も bool。glob パターンが無い |
| F4 | ASDF `defsystem` からのファイル列挙 | 宣言された順序・依存でファイルを解析（ディレクトリ走査より正確） |
| F5 | Clojure `deps.edn` / `project.clj` からのソースパス取得 | |
| F6 | Elisp `Package-Requires` 解析 | |
| F7 | `--since <git-ref>` | 変更ファイルのみ解析。**CI の実行時間に直結** |
| F8 | ファイルリストを stdin から受ける | `git ls-files \| paredit ...` の連携 |
| F9 | 複数リポジトリ横断 | モノレポ / 複数チェックアウト |
| F10 | アーカイブ・リモート入力 | tarball や git URL を直接解析 |
| F11 | 探索結果のキャッシュ | 大規模ツリーでの再走査コスト削減 |
| F12 | シンボリックリンクの追跡オプション | 現状はスキップのみ |

---

## G. 統合・インターフェース層（26 件）

**ここが最も伸びしろが大きい。** 275 コマンド分の解析資産が、CLI という単一の口からしか出ていない。

### G-1. サーバ・プロトコル

| # | 候補 | 概要 |
| --- | --- | --- |
| G1 | **`paredit lsp`（LSP サーバ）** | diagnostics ← lint、code actions ← auto-fix と refactor、rename ← `rename-at`、document symbols ← `outline`、**selection range ← S 式単位の選択拡大**（LSP の中でも本ツールが圧倒的に強い部分）、formatting ← `format`。既存機能をほぼそのまま写せる |
| G2 | **`paredit mcp`（MCP サーバ）** | AI エージェント向け。`inspect capabilities --output json` が既にあるので薄く作れる。現在の `skills/SKILL.md` より密な結合 |
| G3 | `paredit serve`（常駐 HTTP/JSON-RPC） | 起動コストと解析キャッシュを共有。大規模プロジェクトで効く |
| G4 | DAP 的な「リファクタのステップ実行」 | plan の各ステップを対話的に承認 |

### G-2. 出力・相互運用

| # | 候補 | 概要 |
| --- | --- | --- |
| G5 | **`paredit diff`（構造 diff）** | テキスト diff ではなく S 式の木 diff。**このツールの独自性が最も出る候補** |
| G6 | `paredit patch` | 構造 diff を別ファイル・別ブランチに適用（リファクタの移植） |
| G7 | SARIF を lint 以外にも | 全 `inspect` レポートを SARIF で出す |
| G8 | JUnit XML 出力 | CI のテストレポート面に載せる |
| G9 | CodeClimate JSON 出力 | GitLab / Code Climate 連携 |
| G10 | CSV / TSV 出力 | 表計算での集計 |
| G11 | Graphviz / Mermaid 出力 | `call-graph` / `dependencies` / `class-hierarchy` の可視化 |
| G12 | HTML レポート | 単体で共有できる解析結果 |
| G13 | `capabilities` の JSON Schema 公開 | エージェント側の型付けを可能にする |

### G-3. 配布・埋め込み

| # | 候補 | 概要 |
| --- | --- | --- |
| G14 | WASM ビルド | ドキュメントサイトに Playground を置き、導入障壁を下げる |
| G15 | C ABI | 各言語からの利用の土台 |
| G16 | Python バインディング（pyo3） | |
| G17 | Node バインディング（napi） | |
| G18 | Docker イメージ | Nix を前提にできない環境向け |
| G19 | 静的リンクバイナリのリリース添付 | `cargo install` / Nix 以外の導入経路 |

### G-4. エディタ・CI

| # | 候補 | 概要 |
| --- | --- | --- |
| G20 | Emacs パッケージ `paredit-cli.el` | CLI をバックエンドにした構造編集（本家 `paredit.el` とは別物） |
| G21 | VSCode / Neovim 拡張 | G1 ができれば従属的に実現 |
| G22 | pre-commit フレームワーク対応 | `.pre-commit-hooks.yaml` の公式提供 |
| G23 | GitHub Action の拡張 | 現状 lint/format/fix の 3 モード → refactor 検証、PR へのレビューコメント投稿、SARIF アップロードの内蔵 |
| G24 | `paredit watch` | ファイル変更監視で lint / format を再実行 |
| G25 | `paredit init` | 設定ファイルと CI 定義の雛形生成 |
| G26 | man ページ / `--help` の Markdown 出力 | ドキュメント自動生成（現状シェル補完のみ） |

---

## H. 設定・エージェント体験（18 件）

### H-1. 設定

| # | 候補 | 概要 |
| --- | --- | --- |
| H1 | **設定ファイル** `paredit.toml` | ルール有効化、除外パス、方言強制、format 設定。ルール数が 170 に達した以上、フラグ運用は限界 |
| H2 | 設定の階層マージ | リポジトリ / ディレクトリ / ユーザ の 3 層 |
| H3 | 設定の継承（`extends`） | 共通設定を別リポジトリから引く |
| H4 | 設定の検証コマンド | `paredit config check` |
| H5 | 有効設定のダンプ | `paredit config show`（どのファイルのどの行が効いたか） |
| H6 | 環境変数によるオーバーライド | CI での一時的な調整 |

### H-2. エージェント向け

| # | 候補 | 概要 |
| --- | --- | --- |
| H7 | `agent-report` の増分版 | 前回からの差分だけを返す |
| H8 | `--max-tokens` | トークン予算を指定してレポートを切り詰める |
| H9 | `--verbosity` | レポートの要約レベル |
| H10 | 「次に打つべきコマンド」提案 | 現状 `refactor status` が近い。全レポートに拡張 |
| H11 | 失敗時の修復提案の構造化 | エラーに「どう直すか」の機械可読な候補を添える |
| H12 | 決定性の明文化とテスト | 同一入力 → 同一バイト出力を契約として保証 |
| H13 | ドライラン統一 | 全 `refactor` に一貫した `--dry-run` |
| H14 | 変換の説明生成 | 「この編集が何を変えたか」を自然言語で返す（PR 説明の下書き） |
| H15 | バッチ実行の進捗出力 | 長時間の workspace 操作の進捗を JSON Lines で |
| H16 | `--fail-on` の統一 | レポートごとにバラバラな失敗条件を揃える |
| H17 | エラーコードの体系化 | 終了コードだけでなく、機械可読なエラー ID |
| H18 | メッセージの i18n | 日本語出力 |

---

## I. 安全性・検証・性能（16 件）

| # | 候補 | 概要 |
| --- | --- | --- |
| I1 | 往復プロパティ検証 | edit → 逆 edit で元に戻ることを proptest で保証（proptest 基盤は既にある） |
| I2 | マクロ展開を考慮した rename | 現状の rename は構文的。マクロが生成する名前を取りこぼす可能性 |
| I3 | 複数ファイル書き込みの原子性 | トランザクションとロールバック |
| I4 | `paredit undo` | 適用済み manifest からの巻き戻し |
| I5 | 外部処理系との突き合わせ | SBCL の `compile-file` 警告とリファクタ結果を照合 |
| I6 | リファクタ後のテスト実行 | 指定コマンドを走らせ、失敗ならロールバック |
| I7 | 等価性の実行時検証 | 変換前後の関数を SBCL で評価して結果を比較 |
| I8 | ミューテーションテスト | コードを変異させ、テストが落ちるかで検査の質を測る |
| I9 | fuzz ターゲット | `cargo-fuzz` によるパーサの堅牢性検証 |
| I10 | **実世界コーパステスト** | Quicklisp / SBCL ソースを parse して panic しないことを CI で保証。**品質の底上げに直結** |
| I11 | インクリメンタル解析キャッシュ | blake3 は既に依存にある。大規模プロジェクトの再解析コスト削減 |
| I12 | workspace 解析の並列化 | rayon 化。ベンチ基盤は既存 |
| I13 | メモリ使用量の上限設定 | 巨大ファイルでの OOM 防止 |
| I14 | タイムアウト | ルール単位・ファイル単位 |
| I15 | ベンチ回帰の CI ゲート化 | 既存 criterion ベンチの活用 |
| I16 | 権限の最小化 | `cap-std` は既に使用。書き込み範囲の明示的な制限をユーザに見せる |

---

## J. ドキュメント・エコシステム（16 件）

| # | 候補 | 概要 |
| --- | --- | --- |
| J1 | ルールごとの独立ドキュメントページ | 170 ルールは個別ページに値する（C28 と対） |
| J2 | Playground | G14（WASM）の応用 |
| J3 | レシピ集 | 「よくあるリファクタ」をコマンド列で示す |
| J4 | 対話型チュートリアル | |
| J5 | 移行ガイド | 他ツール（`sblint`、`clj-kondo`）からの乗り換え |
| J6 | ベンチマーク結果の公開 | 大規模プロジェクトでの実測値 |
| J7 | 実プロジェクト適用事例 | |
| J8 | アーキテクチャ図の自動生成 | 26 パッケージの依存を図に |
| J9 | 変更履歴の自動生成 | conventional commits からの CHANGELOG |
| J10 | ルール網羅性ダッシュボード | 方言 × ルールの対応状況を一覧に（A22 と対） |
| J11 | コマンド網羅性ダッシュボード | 「レポートはあるが変換が無い」ギャップの可視化（D 群の発見に使った観点の常設化） |
| J12 | エージェント向けチートシート | `skills/SKILL.md` の拡充 |
| J13 | 用語集 | path / span / manifest / trivia などの定義 |
| J14 | トラブルシューティング | よくある parse エラーとその原因 |
| J15 | 貢献者向けのルール追加ガイド | 新 lint ルールを足す手順の定型化 |
| J16 | リリースノートの自動生成 | 追加ルール・追加コマンドの差分から |

---

## K. paredit 本家との対応 — 構造編集の網羅（14 件）

`edit` は 20 コマンドで Emacs `paredit.el` の中核（slurp / barf / splice / raise / convolute / transpose）を押さえているが、いくつか穴がある。`WrapArgs` の `WrapDelimiter` は `Paren` / `Bracket` / `Brace` の 3 値のみで、文字列とリーダマクロ接頭辞が扱えない。

| # | 候補 | 概要 |
| --- | --- | --- |
| K1 | `wrap --delimiter doublequote` | 文字列で包む。Emacs の `paredit-meta-doublequote` 相当。現状 3 delimiter のみ |
| K2 | `wrap --prefix quote\|quasiquote\|sharp-quote\|unquote` | `'x` `` `x `` `#'x` `,x` を付ける。CL/Scheme のマクロ作業で頻出 |
| K3 | `unwrap-prefix` | K2 の逆。接頭辞だけを剥がす |
| K4 | `edit navigate` | forward / backward / up / down の移動先 `--path` を返す。**エージェントが path を手で組み立てる手間を削る** |
| K5 | `edit delete-forward` / `delete-backward` | 構造安全な文字単位削除 |
| K6 | `edit newline` | 構造安全な改行挿入 + 再インデント |
| K7 | `edit copy` | 非破壊の形取得（`kill` の読み取り版。`select` と違い trivia 込み） |
| K8 | kill ring 相当 | `kill` した形を保持し `edit yank` で貼る。セッション状態が要るので設計判断が必要 |
| K9 | `edit reindent-defun` | 選択定義だけ再インデント（E11 部分フォーマットと統合可） |
| K10 | `raise` の多階層版 | `--levels N` で複数階層まとめて持ち上げる |
| K11 | `edit split-string` | 文字列リテラルの分割（`join` の文字列結合は既にある。逆方向が無い） |
| K12 | `edit escape-string` / `unescape-string` | 文字列リテラルのエスケープ操作 |
| K13 | `inspect context-at` | 指定オフセットがコード / 文字列 / コメント / リーダマクロのどこにあるか。編集前の安全確認 |
| K14 | `edit transpose` の非隣接版 | 任意の 2 兄弟を入れ替える（現状は隣接のみ） |

---

## L. セレクタの拡張（8 件）

現在の選択手段は `--path`（ツリーパス）と `--at`（バイトオフセット）の 2 つ。これは正確だが、**人間にもエージェントにも組み立てコストが高い**。

| # | 候補 | 概要 |
| --- | --- | --- |
| L1 | **`--query <pattern>`** | S 式パターンマッチで選択（`(defun ?name ...)` のような形）。C15 カスタムルール DSL とパターン言語を共有できる。**単独で最も効く** |
| L2 | `--name <symbol>` | 定義名から直接選択。`outline` を挟まずに済む |
| L3 | `--line:column` | エディタ由来の座標。LSP 実装の前提にもなる |
| L4 | `--from` / `--to` | 複数フォームにまたがる範囲選択 |
| L5 | `--all` | マッチ全件に同じ変換を適用（D43 と対） |
| L6 | 相対セレクタ | `--parent` / `--child N` / `--sibling +1` の指定 |
| L7 | 安定セレクタ ID | 編集後も同じ形を指せる ID（C23 安定 finding ID と同じ基盤） |
| L8 | セレクタの解決結果を返すコマンド | `paredit inspect resolve --query ...` でマッチ一覧と path を返す。デバッグと 2 段階実行に使う |

---

## M. クローン検出の深化（6 件）

`packages/feature/similarity/src/form_similarity.rs` は `StructuralTree` に対する**予算付きの木編集距離**（`tree_similarity_with_workspace_and_budget`、`similarity_upper_bound` による枝刈り）を実装している。これはかなり本格的な資産で、現状の `duplicates` / `similarity` の 2 コマンドは活用しきれていない。

| # | 候補 | 概要 |
| --- | --- | --- |
| M1 | クローン型の分類 | Type-1（完全一致）/ Type-2（識別子違い）/ Type-3（構造の近似）を明示的にラベル付け |
| M2 | 部分木クローン | 現状はフォーム単位。定義の一部だけが重複するケースを検出 |
| M3 | クロスプロジェクト検出 | 依存ライブラリと自コードの重複（車輪の再発明の発見） |
| M4 | 抽出候補のランキング | 「共通化すると何行減るか」でクローン群を序列化 |
| M5 | 閾値の自動調整 | プロジェクトの性質に応じた類似度しきい値の推定 |
| M6 | クローン系譜 | git 履歴でのコピペ伝播を追跡 |

---

## N. コード生成（6 件）

現在の `refactor` は「既存コードの変形」に閉じている。既存の解析結果から**新しいコードを生む**方向は未着手。

| # | 候補 | 概要 |
| --- | --- | --- |
| N1 | `generate defpackage` | ファイル内で使用しているシンボルから `defpackage` を生成 |
| N2 | `generate defsystem` | ディレクトリ構成と依存解析から ASDF `defsystem` を生成 |
| N3 | `generate tests` | 定義からテストのスケルトンを生成（`inspect test-map` の未テスト定義と直結） |
| N4 | `generate accessors` | `defclass` のスロットから `:accessor` / `:reader` を一括生成 |
| N5 | `generate defgeneric` | 既存 `defmethod` 群から `defgeneric` を導出（D15 と同一） |
| N6 | `generate docstring` | シグネチャからテンプレートを生成（D37 と同一） |

---

## O. 新しい名前空間の可能性（3 件）

現在の 3 名前空間（inspect / edit / refactor）は「読む / 1 形を変える / 意味を変える」という良い分割。ただし 275 コマンドが `inspect` に 130 集中しており、再分割の余地がある。

| # | 候補 | 概要 |
| --- | --- | --- |
| O1 | `paredit query` | パターン検索専用の名前空間（L1 の受け皿）。`inspect` の 130 コマンドから検索系を分離 |
| O2 | `paredit fix` | lint の自動修正専用。現在 `inspect lint --fix` に埋もれていて発見しづらい |
| O3 | `paredit migrate` | 大規模なコード近代化専用（非推奨 API の一括置換など）。`refactor` の workspace 系と役割が近いので統合も選択肢 |

---

## P. このリポジトリ自体の開発体験（6 件）

`scripts/` に feature package の scaffold を担う Python スクリプトが 6 本ある（`scaffold-feature-package.py`、`wire-feature-facade.py`、`move-lint-package.py` 等）。26 パッケージ・236 テストモジュール規模の開発を支える仕組み自体が、機能追加の対象になりうる。

| # | 候補 | 概要 |
| --- | --- | --- |
| P1 | `cargo xtask` 化 | 6 本の Python スクリプトを Rust に統合。ビルド依存を減らし、型で守る |
| P2 | 新 lint ルールのジェネレータ | ルール追加の定型作業（registry 登録・テスト・ドキュメント）を自動化。170 → さらに増やす前提なら必須 |
| P3 | 新コマンドのジェネレータ | args / workflow / render / テストの雛形生成 |
| P4 | ドキュメント同期の contract test | `commands.md` と実装コマンド一覧の乖離を CI で検出 |
| P5 | パッケージ依存の検査 | 26 パッケージ間の許可された依存方向をルール化し、違反を CI で止める |
| P6 | ベンチ自動比較レポート | 2 リビジョンを背中合わせで走らせて差分を出す（絶対値はマシン状態で振れるため） |

---

## Q. 診断とエラー体験（10 件）

`sexpr/error.rs` の `StructureError` は 6 つの typed enum に整理されていて設計は良い。ただし各バリアントは **`#[error("...")]` の文字列だけ**で、位置情報も修復提案も持たない。`"selected expression has no next sibling to transpose"` と言われても、エージェントは次に何を試せばよいか分からない。

| # | 候補 | 概要 |
| --- | --- | --- |
| Q1 | エラーへのスパン付与 | どの位置で失敗したかを構造化して返す |
| Q2 | caret 表示 | ソースの該当箇所を抜粋し `^^^` で指す（rustc / miette 風） |
| Q3 | 修復提案 | 「代わりにこの `--path` を試せ」を機械可読で添える。**エージェントの往復削減に直結** |
| Q4 | エラーコードの体系化 | `E0001` 形式の安定 ID（H17 と統合） |
| Q5 | did-you-mean | 存在しない `--path` / ルール名 / コマンド名に対する近傍候補 |
| Q6 | parse エラーの回復と複数報告 | 最初の 1 件で止めず、まとめて返す |
| Q7 | エラーの JSON 構造化 | `--output json` 指定時はエラーも JSON で（現状の契約を要確認） |
| Q8 | 警告レベルの導入 | 現在は成功 / 失敗の 2 値。「できたが注意」の中間が無い |
| Q9 | ドキュメントリンク | エラーメッセージから該当ドキュメントへ |
| Q10 | 部分的成功の報告 | workspace 操作で一部ファイルだけ失敗した場合の詳細 |

---

## R. 意味解析カバレッジと方言パリティ（5 件）

`examples/semantic_coverage.rs` は「`domain::semantics` が実コーパスに対してどれだけ解決できるか」を測る開発用ハーネスで、未解決原因のヒストグラムを出し「次に登録すべきオペレータ」を示す設計になっている。**ただし CLI ではなく example で、深さは環境変数 `SEMANTIC_COVERAGE_TOP` で渡す。**

そのソース中のコメントが、A 群（方言の深さ）の根拠を裏づけている:

> Only Common Lisp is worth measuring here: `build_binding_table` and `build_value_table` return an empty table for every other dialect.

つまり**意味解析層は現時点で CL 専用**であることが、コード側で明示されている。

| # | 候補 | 概要 |
| --- | --- | --- |
| R1 | `inspect semantic-coverage` への昇格 | example から CLI コマンドへ。利用者が自分のコードベースの解析精度を測れる |
| R2 | 方言別カバレッジ計測 | CL 以外が 0 であることを数字で可視化し、A 群の進捗指標にする |
| R3 | カバレッジの CI トラッキング | 回帰防止。ルール追加でカバレッジが落ちないことを保証 |
| R4 | 未解決原因からの提案 | 「次に登録すべきオペレータ」の自動提示（example が既に持つロジックの製品化） |
| R5 | 評価コーパスの同梱 | Quicklisp のサブセットを固定して、計測を再現可能に（I10 コーパステストと共有） |

---

## S. 抑制機構の拡張（6 件）

`lint_suppression.rs` の抑制は **行ベース**（`; paredit:ignore` が自分の行か次の行を守る）で、全ルール抑制と名前指定の 2 形態を持つ。未使用抑制の検出まであるのは良いが、粒度と運用面に余地がある。

| # | 候補 | 概要 |
| --- | --- | --- |
| S1 | フォーム単位の抑制 | `;; paredit:ignore-form` で次の S 式全体。行ベースだと複数行フォームに毎行書く必要がある |
| S2 | ファイル単位の抑制 | 先頭の 1 行で全体を制御 |
| S3 | 理由必須モード | `; paredit:ignore rule -- なぜ抑制するか` を強制 |
| S4 | 有効期限付き抑制 | `; paredit:ignore-until 2026-12-31`。期限切れを CI で検出 |
| S5 | 設定ファイルからのパス単位抑制 | 生成コードやベンダディレクトリ（H1 と統合） |
| S6 | 抑制の棚卸しレポート | 古い・多すぎる抑制の一覧。`--report-unused-suppressions` の一歩先 |

---

## T. プラットフォームと I/O（9 件）

`packages/core/cli/src/io.rs` は 2,400 行超で、**パーミッション・拡張属性・macOS ACL を保存する原子的書き込みとロールバックライタ**を実装している。README 曰く「the rollback writer is here and is the only one」。かなり真面目な I/O 層だが、`#[cfg(unix)]` を前提にしている。

| # | 候補 | 概要 |
| --- | --- | --- |
| T1 | Windows 対応 | xattr / ACL / パーミッション保存が unix 専用。Windows では書き込み経路が未検証 |
| T2 | ファイルロック | 並行実行時の競合。blake3 の expected-write 前提条件はあるが、排他ロックは無い |
| T3 | git 連携の書き込み | 変更を stage する / 別ブランチにコミットする / worktree に書く |
| T4 | シンボリックリンク越しの書き込みポリシー | 現状は探索時にスキップ。書き込み側の方針を明示 |
| T5 | `MAX_SOURCE_INPUT_BYTES` の設定可能化 | 64 MB のハードコード上限 |
| T6 | 非 UTF-8 ソースの扱い | Shift_JIS など、レガシーな Lisp ソースのエンコーディング指定 |
| T7 | 改行コードの保存 | CRLF ファイルを LF に変えていないかの保証と、明示的な変換オプション |
| T8 | ファイル権限の明示制御 | 新規作成ファイルのモード指定 |
| T9 | 書き込みのドライラン検証 | 実際に書かずに、書き込み可能性（権限・ディスク容量）だけを検査 |

---

## U. 端末 UX（5 件）

`packages/core/cli/src/` に色付けの実装は無い（grep で該当するのはテスト名のみ）。170 ルールの lint 出力と 130 種のレポートが、すべて単色テキストで出ている。

| # | 候補 | 概要 |
| --- | --- | --- |
| U1 | カラー出力 | `--color auto\|always\|never` と `NO_COLOR` 環境変数の尊重。diff と lint 出力の可読性が大きく変わる |
| U2 | プログレス表示 | workspace 操作の進捗（H15 の JSON Lines と対になる人間向け出力） |
| U3 | ページャ連携 | 長いレポートの `$PAGER` 委譲 |
| U4 | 端末幅への適応 | 表形式レポートの折り返し |
| U5 | TUI ブラウザ | 木構造を対話的に辿り、編集を試す（G3 常駐サーバと組み合わせると実用的） |

---

## 優先度の見立て

### 第 1 波 — 既存資産の再利用率が高い

| 順 | 候補 | 理由 |
| --- | --- | --- |
| 1 | **G1 LSP サーバ** | 275 コマンドの価値を、CLI を知らない利用者に届ける唯一の手段。実装の大半は既存呼び出しの薄いラッパ。特に selection range は他の Lisp LSP に対する構造的な優位 |
| 2 | **B5 `inspect effects`** | 多くのリファクタ安全判定の共通基盤。ここが入ると D 群の実装コストが下がる |
| 3 | **D1 `add-ignore-declaration`** | 「レポートはあるが直す手段が無い」ギャップの穴埋め。小さく確実 |
| 4 | **H1 設定ファイル** | ルール数が 170 に達した以上、フラグ運用は限界 |
| 5 | **E1 `format --check`** | 数行の追加で CI 体験が改善する |
| 6 | **F7 `--since <git-ref>`** | CI 実行時間に直結。実装は小さい |
| 7 | **K4 `edit navigate`** | エージェントが `--path` を手で組み立てる往復を削る。既存のツリー走査の露出のみ |
| 8 | **K1/K2 wrap の delimiter 拡張** | 文字列と `'` / `` ` `` / `#'` / `,`。`WrapDelimiter` に値を足すだけ |
| 9 | **Q3 エラーへの修復提案** | 「代わりにこの `--path` を試せ」。エージェント向けツールとして最も効く診断改善 |
| 10 | **R1 `inspect semantic-coverage`** | example から CLI への昇格。利用者が自コードベースの解析精度を測れる。**A 群の進捗指標にもなる** |
| 11 | **S1 フォーム単位の抑制** | 行ベース抑制は複数行フォームで毎行書く必要がある。実用上の摩擦が大きい |
| 12 | **U1 カラー出力** | 色付けの実装が一切無い。lint 170 ルールと diff の可読性が単色のままになっている |

### 第 2 波 — 独自性・拡張性

| 順 | 候補 | 理由 |
| --- | --- | --- |
| 13 | **L1 `--query` パターンセレクタ** | C15 カスタムルール DSL とパターン言語を共有できる。**この 2 つは同じ基盤なので同時に設計すべき** |
| 14 | **C15 カスタムルール DSL** | ルール追加を Rust の作業から利用者の作業に移す |
| 15 | **G5 構造 diff** | 他ツールに代替が無い |
| 16 | **G2 MCP サーバ** | 本プロジェクトの掲げる「AI エージェント向け」に最も直接的 |
| 17 | **C23 / L7 安定 ID** | baseline・抑制・セレクタが同じ基盤を共有できる |
| 18 | **B27/B28 API surface と diff** | SemVer 運用の自動化 |
| 19 | **M1–M4 クローン検出の深化** | 木編集距離の実装は既にある。分類とランキングを足すだけで実用度が跳ねる |

### 第 3 波 — 幅を広げる

| 順 | 候補 | 理由 |
| --- | --- | --- |
| 20 | **A1–A9 Elisp の深化** | 利用者人口が CL に次いで大きい |
| 21 | **A16–A20 Clojure の深化** | 同上 |
| 22 | **R2/R3 方言別カバレッジの計測と CI 追跡** | A 群の進捗を数字で管理する。深化の前に計測を置く |
| 23 | **I10 コーパステスト** | 上記すべての土台になる品質保証 |
| 24 | **P2 lint ルールのジェネレータ** | ルールを 170 からさらに増やす前提なら、先に足場を作るべき |
| 25 | **C1–C14 新 lint カテゴリ** | 継続的に足せる、リスクの低い増分 |
| 26 | **J11 コマンド網羅性ダッシュボード** | 今後のギャップ発見を自動化する |

### 見送り推奨

| 候補 | 理由 |
| --- | --- |
| G15–G17 言語バインディング | 利用者の需要が観測されていない |
| G24 watch | LSP があれば大半の用途を吸収する |
| H18 i18n | 出力がエージェント向けである以上、優先度は低い |
| G4 ステップ実行 | plan / preview / apply の分割で既に達成されている |
| I7 実行時等価性検証 | 処理系への依存が強く、CI の再現性を損なう |

---

## 選び方の指針

この一覧は 3 つの異なる戦略に分解できる。**どれを取るかで第 1 波の中身が変わる。**

| 戦略 | 意味 | 中心となる候補 |
| --- | --- | --- |
| **露出拡大** | 既にある解析力を、より多くの利用者・文脈に届ける | G1 LSP、G2 MCP、G14 WASM、B1–B5 未露出資産、M1–M4 クローン検出 |
| **独自性の深化** | 他ツールに代替が無い領域を伸ばす | G5 構造 diff、C15 カスタムルール DSL、L1 パターンセレクタ、B6/B7 マクロ解析 |
| **幅の拡大** | 「10 方言対応」の約束を実装で埋める | A 群すべて、C1–C14 |
| **摩擦の除去** | 既存機能を使いやすくする | K 群（paredit 網羅）、L 群（セレクタ）、E1 `--check`、H1 設定ファイル |
| **足場固め** | 今後の増分を安くする | P 群（開発体験）、I10 コーパステスト、J11 ダッシュボード |

現時点の観測では、**露出拡大が最も投資対効果が高い**。理由は単純で、275 コマンド・170 ルールという蓄積に対して、それを取り出す口が CLI ひとつしか無いため。

ただし**摩擦の除去**は個々の実装が小さく、単独で価値が閉じる（K1/K2 は enum に値を足すだけ、E1 は exit code を返すだけ）。大きな投資を決める前の助走として適している。

### 設計上、まとめて決めるべき候補群

以下は独立に作ると基盤が分裂する。**設計を同時に行うべき組**として挙げておく。

| 組 | 候補 | 共有する基盤 |
| --- | --- | --- |
| パターン言語 | L1 `--query`、C15 カスタムルール DSL、O1 `paredit query` | S 式パターンマッチの構文と意味論 |
| 安定 ID | C23 finding ID、L7 セレクタ ID、C27 範囲抑制 | 内容ベースのハッシュと位置非依存の同定 |
| 設定 | H1–H6、C17/C18 ルール設定、E2–E5 format 設定 | 設定ファイルのスキーマと階層マージ |
| 部分適用 | E11 部分フォーマット、E12 `--range`、K9 `reindent-defun`、G1 LSP の range formatting | 範囲を限定した書き換えの共通経路 |
| 生成 | N1–N6、D15 `generate-defgeneric`、D37 `add-docstring` | テンプレートとシンボル収集 |
