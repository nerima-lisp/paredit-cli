# 機能追加候補カタログ（2026-08-01 版）

対象: `nerima-lisp/paredit-cli` v1.3.0
目的: 「次に何を作るか」を選ぶための候補の網羅。採否は未決。
候補数: **198**（A〜AC の 29 セクション）

前版（v1.2.1 時点、`git show v1.2.1:FEATURE-CANDIDATES.md` で参照可能。
当時はリポジトリ直下に置かれていた）は今回すべて破棄し、
現在の `main` を実地に確認した上でゼロから書き直した。理由は次節。

---

## 0. 前版との差分 — 何が変わったか

v1.2.1 版は「存在しないもの」として LSP・MCP・設定ファイル・カスタムルール機構・
watch・WASM・構造 diff・エディタ拡張・`format --check` を挙げていたが、
v1.3.0 の実装を `command.rs` / `CHANGELOG.md` で直接確認したところ、
**WASM とエディタ拡張と watch を除く全部が既に存在する**。反省を兼ねて明記する。

| 前版の主張 | 現状（v1.3.0、実地確認済み） |
| --- | --- |
| LSP が無い | `paredit lsp` — 診断・code action・outline・selectionRange・folding・rename 等を持つ LSP 3.17 サーバー |
| MCP が無い | `paredit mcp` — `--read-only` 付き。ただし `docs/src/guide/integrations.md` に未掲載（→ H1） |
| 設定ファイルが無い | `paredit config {check,show,schema,init}` と 5 層の `paredit.toml`（`extends` 対応） |
| カスタムルール機構が無い | `.paredit/rules/*.lisp` に `defrule`/`deftest`/`deprecate` を書く機構が実装済み |
| 構造 diff が無い | `inspect diff` が実装済み |
| `format --check` が無い | `edit format --check` と `--diff-stat` が実装済み |
| B1-B5（types/narrowing/constants/value-propagation/effects の露出） | 全て `inspect types` 等として個別コマンド化済み |
| 方言の深さ（旧 A 節） | v1.3.0 で LFE/Fennel/Janet/Hy/Carp にスコープ解析、Elisp に意味層＋9 lint ルールを追加済み |
| クローン検出（旧 M 節） | `inspect clone-{classes,sequences,external,threshold,genealogy}` が実装済み |
| 名前空間（旧 O 節） | `query`/`fix`/`migrate` は実装済み（前版でも「実装済み」と自己記載していた） |

一方で **今も動かず、コードで直接確認した** 事実:

- `packages/core/semantics/src/semantics/typing/service/declarations.rs:139` — `if dialect != Dialect::CommonLisp { return empty }`。
  型宣言解析は今も CL 専用。`inspect types`/`inspect narrowing` は他方言では常に空を返す（→ A 節）。
- `watch` という語はコード中に検索一致ゼロ。ファイル監視によるインクリメンタル実行は無い（→ C 節）。
- WASM ターゲットはゼロヒット（→ D1）。
- `docs/src/guide/integrations.md` に `mcp` と `tui` のセクションが無い（実装はあるのに）（→ H1）。

以降は、これらの実地確認を土台にした新規候補。

---

## A. 意味解析層の方言パリティ（8 件）

`build_binding_table`（スコープ）は v1.3.0 で 6 方言に広がったが、
`typing`（宣言型）と `value`（定数畳み込み・伝播）の 2 層は今も CommonLisp 分岐一本槍。
「effects/purity まで露出しているのに CL でしか動かない」のが一番惜しい非対称。

| # | 候補 |
| --- | --- |
| A1 | Emacs Lisp への typing 層拡張 — `cl-defstruct`/`defcustom :type` からの宣言型抽出 |
| A2 | Clojure への typing 層拡張 — `:tag` メタデータ、Spec/Malli の `s/def` からの型情報 |
| A3 | Scheme/Racket への typing 層拡張 — Typed Racket の型注釈、R7RS `define-record-type` |
| A4 | value 層（定数畳み込み・伝播）の Elisp 対応 — `defconst`/`defcustom` の初期値追跡 |
| A5 | `inspect effects`（純粋関数判定）の非 CL 対応 — 現状 CL 以外は `unmodelled` 一色 |
| A6 | `inspect semantic-coverage --fail-under` を方言別に設定できる閾値に分割 |
| A7 | 型層の「宣言はあるが値がそれと矛盾する」検出を非 CL 方言へ拡張 |
| A8 | `inspect capabilities` の `tier` に typing/value 層の方言別到達度を明示 |

---

## B. ライブ Lisp 処理系との連携（7 件）

現状は完全に静的解析。マクロ展開シミュレーション（`inspect macro-expansion`）は
テンプレートの局所展開に留まり、実際のマクロ環境やコンパイラ警告とは無関係。
SWANK/SLY・nREPL 等の既存プロトコルに乗ることで「本物の展開結果」を扱える。

| # | 候補 |
| --- | --- |
| B1 | SBCL への SWANK 接続オプション — 実際の `macroexpand-1` 結果で `inspect macro-expansion` を裏付ける |
| B2 | 実行時コンパイラ警告（`style-warning`/`unused variable` 等）を `inspect lint` の findings に統合 |
| B3 | nREPL 接続による Clojure の実マクロ展開・`*ns*` 解決 |
| B4 | 生きた処理系から `defgeneric`/`defmethod` の実引数型を取得し `generate defgeneric` の精度を上げる |
| B5 | REPL 経由でのフォーム単位の即時評価コマンド（`edit`/`refactor` の適用前プレビューに実値を添える） |
| B6 | 接続断・処理系なしでも静的解析にフォールバックする明示的な二段構え（誤って必須化しない） |
| B7 | 複数方言の処理系（SBCL/Guile/Babashka）を切り替える統一プロトコル層の抽象化 |

---

## C. 監視・常駐ワークフロー（6 件）

`serve` は常駐キャッシュを持つが、外部からのリクエスト起点。
ファイル変更を起点にした自動実行は無い。

| # | 候補 |
| --- | --- |
| C1 | `paredit watch` — ファイル変更を検知して `lint`/`format --check` を再実行し、差分だけ再表示 |
| C2 | `serve` にファイルシステム監視を足し、キャッシュを変更のあった範囲だけ無効化 |
| C3 | エディタの保存フックから叩く軽量プロトコル（LSP の `textDocument/didSave` 経由で十分か検討） |
| C4 | `watch --exec <command>` — 変更のあったファイル集合を渡して任意コマンドを起動するプラガブルフック |
| C5 | CI 以外の「常時バックグラウンドで走らせておく」モードの systemd/launchd ユニット例をドキュメント化 |
| C6 | 変更が伝播する範囲（呼び出し元）を `inspect impact` と組み合わせて再解析範囲を絞る差分実行 |

---

## D. 配布チャネルの拡張（10 件）

Nix flake / Cachix / GitHub Action / Git タグ配布は既にある。crates.io publish は方針上ずっと非対応
（[[paredit-cli-ships-as-a-git-tag]] のメモ通り、これは変更提案しない）。

| # | 候補 |
| --- | --- |
| D1 | WASM ターゲット（`wasm32-unknown-unknown` or `wasip1`）— ブラウザ上の Lisp プレイグラウンド向け |
| D2 | Homebrew formula（tap リポジトリ） |
| D3 | apt/deb パッケージ（GitHub Release の asset として） |
| D4 | Docker イメージ（CI での `docker run` 一発利用、Nix より起動が軽い代替） |
| D5 | `mise`（旧 rtx）plugin — バージョン管理ツール経由のインストール |
| D6 | asdf（バージョンマネージャ、CL の同名ライブラリと紛らわしいので明記）plugin |
| D7 | scoop/winget（Windows 対応が視野に入るなら） |
| D8 | `cargo install --git` 経由のインストール手順のドキュメント整備（crates.io 非公開の代替導線） |
| D9 | pre-built musl バイナリで Alpine/コンテナベースの CI を軽くする |
| D10 | GitHub Releases の checksum/署名（cosign 等）による配布物の検証手段 |

---

## E. エディタ統合の専用化（9 件）

LSP はプロトコルとして汎用だが、エディタ側の「導入の摩擦」を下げる専用パッケージは無い。

| # | 候補 |
| --- | --- |
| E1 | VS Code 拡張 — LSP クライアントの自動起動・`paredit.toml` スキーマの JSON Schema 補完 |
| E2 | Emacs パッケージ（MELPA）— 本家 `paredit.el`/`smartparens` と役割を分けた「意味解析」側の薄いラッパー |
| E3 | Neovim プラグイン（`nvim-lspconfig` プリセットの提供） |
| E4 | Zed 拡張 |
| E5 | Sublime Text の LSP クライアント設定プリセット |
| E6 | JetBrains（IntelliJ 系）向け LSP 統合の設定例 |
| E7 | エディタ非依存の「保存時に `fix apply --fixable` を走らせる」サンプルフックの配布 |
| E8 | `--select` の compact grammar を使うエディタ拡張向けのカーソル位置→selector 変換ヘルパー API |
| E9 | 各エディタパッケージの「対応表」を `docs/src/guide/integrations.md` に追加（現状 LSP と serve のみ記載） |

---

## F. 残る structural edit / refactor 変換（16 件）

`paredit.el` パリティ（旧 K 節）は v1.3.0 でほぼ埋まったが、Lisp 方言間の「よくある書き換え」は
まだ手が回っていないものが残る。

| # | 候補 |
| --- | --- |
| F1 | `&optional`/`&key` の相互変換（デフォルト値・destructuring を保持） |
| F2 | 位置引数からキーワード引数への一括変換（呼び出し側も追随） |
| F3 | `dolist`/`dotimes`/`loop` 間の相互変換（意味が保存できる範囲に限定） |
| F4 | ローカル関数のグローバル昇格・グローバル関数のローカル降格 |
| F5 | 重複コードの自動パラメータ化 — `inspect clone-classes` が見つけたクラスから抽出関数を提案 |
| F6 | パッケージの分割・統合（`refactor split-file` はファイル単位、パッケージ境界の再編はまだ無い） |
| F7 | `defmethod` 群からの `defgeneric` 引数リスト精緻化（総称関数のシグネチャ差異検出込み） |
| F8 | マクロの本体を関数に降格する変換（安全な場合に限定するハイジーン検査込み） |
| F9 | `format` 制御文字列の構造化書き換え（`~a` の並びをキーワード引数に展開する等） |
| F10 | let 系束縛からの `defvar`/`defparameter` 抽出（スコープ逸脱している束縛の可視化とセット） |
| F11 | 条件分岐の網羅性を保ったままの `case`→`cond`（あるいは逆）の変換ガード強化 |
| F12 | 複数ファイルにまたがる `edit`/`refactor` のトランザクション化 — 現状 `migrate run` のみ部分失敗耐性がある |
| F13 | `cond` の各節での型絞り込み結果（`narrowing` 層）を使い、後続節で冗長になった型チェックの削除提案 |
| F14 | 連続する `setf` 呼び出しを `psetf`/`rotatef` にまとめる提案 |
| F15 | ネストした `if` を `cond` へ段階的に畳み込む変換（F11 の `case`⇄`cond` とは別に、`if` の入れ子解消） |
| F16 | `let*` の各束縛間に依存が無い箇所を検出し、並列評価可能な `let` へ変換できる部分を提案 |

---

## G. 新しい分析カテゴリ（18 件）

`inspect` の既存 228 種は「論理バグ」「重複」「未使用」「型」「効果」に集中している。
まだ触れていない軸。

| # | 候補 |
| --- | --- |
| G1 | シークレットスキャン — `defparameter *api-key* "sk-..."` のような埋め込み秘密情報の検出 |
| G2 | ライセンスヘッダの存在・整合チェック |
| G3 | ドキュメント文字列カバレッジ（既存の `generate docstring` は生成側、カバレッジ計測側が無い） |
| G4 | テストとプロダクションコードの対応（`inspect test-map` は既存 — 未テスト関数のリスク順ランキングが無いなら追加） |
| G5 | Quicklisp/ASDF 依存の既知脆弱性アドバイザリ照合 |
| G6 | シンボルの export/import 一貫性（`defpackage` の `:export` と実際の外部参照の乖離） |
| G7 | 循環依存の検出（パッケージ間・ファイル間の import グラフの閉路） |
| G8 | コメントアウトされたコードの検出・削除提案（`;; (old-code ...)` パターン） |
| G9 | 数値リテラルのマジックナンバー検出・`defconstant` への抽出提案 |
| G10 | 命名規則の一貫性検査（`*special*` 記法、`-p`/`p` 述語サフィックス等の方言慣習違反） |
| G11 | 巨大 `let`/`cond`/`case` の分割提案（既存 `debt-score`/`hotspots` の一段掘り下げ） |
| G12 | 副作用を持つトップレベルフォームの実行順序依存性検出（load 順が結果を変える箇所） |
| G13 | condition/error クラス階層の整合性（`define-condition` の継承関係の妥当性） |
| G14 | パッケージ間の「循環しないが過度に結合している」度合いの指標化（結合度メトリクス） |
| G15 | 方言横断で統一算出する循環的複雑度と、既存 G11（巨大 `let`/`cond`/`case`）の相関レポート |
| G16 | 同一パッケージ内でのシンボル衝突・シャドーイング（内側の束縛が外側の関数名を隠す等）の検出 |
| G17 | 一度も再代入されない `defparameter`/`defvar` の「実質定数」検出（`defconstant` 化提案とセット） |
| G18 | トップレベルフォームの実行順序に依存しない「宣言的」な書き方への準拠度スコア |

---

## H. ドキュメント・可観測性（6 件）

| # | 候補 |
| --- | --- |
| H1 | `docs/src/guide/integrations.md` に `mcp` と `tui` のセクションを追加（実装済み・未文書化の是正） |
| H2 | lint findings のトレンド — 複数コミットの baseline を並べて「増減の推移」を出す `inspect lint-trend` |
| H3 | 実行時間のプロファイル出力（どのルール/どのファイルが遅いか）— 大規模ワークスペースでの `--profile` |
| H4 | `paredit.toml` の設定差分を環境間（ローカル/CI）で比較する `config diff` |
| H5 | 生成された全 40 エラーコードの「よくある原因と対処」を機械可読 JSON でも提供（現状 Markdown のみ） |
| H6 | `inspect capabilities` の結果を静的サイト化し、方言×コマンドの現状を GitHub Pages で常時公開 |

---

## I. エージェント体験の深化（8 件）

MCP は既に厳選済みサーフェス（7 tools + `paredit_run`）であり、CLI 全体を MCP に一対一で
写像する提案はしない（過去に却下済み）。その上でエージェント向けに価値がある候補。

| # | 候補 |
| --- | --- |
| I1 | `refactor plan` の出力に「この変換のリスク見積もり」（影響ファイル数・呼び出し元数から算出）を添える |
| I2 | lint finding や refuse 理由を自然文で説明する `--explain` フラグ（エラーコードの doc_url をその場で展開） |
| I3 | 複数の `edit`/`refactor` 呼び出しをバッチで受け、まとめて一回の再パースで適用するバルク API |
| I4 | エージェントの試行錯誤ログから「よく失敗する selector パターン」を集計し `inspect resolve` の候補提示に使う |
| I5 | `refactor apply`/`fix apply` に dry-run の「想定される次の一手」提案を添える（次に読むべき finding の優先順位） |
| I6 | MCP tool 呼び出しのコスト（トークン数の目安）を tool description に明示 |
| I7 | セッション横断で使う named checkpoint（`refactor step` の命名版、途中経過に戻れる） |
| I8 | 大規模ワークスペースでの部分適用戦略（影響範囲でグルーピングし段階的に fix/migrate を進める） |

---

## J. マルチリポジトリ・組織スケール（6 件）

| # | 候補 |
| --- | --- |
| J1 | 複数リポジトリを横断した `inspect clone-external` の常設コーパス管理（社内ライブラリ集約） |
| J2 | 組織共通のポリシーバンドル（`paredit.toml` の `extends` を URL 参照にする、または社内レジストリ経由） |
| J3 | monorepo 内の複数 ASDF システムをまたぐ `refactor workspace-plan` の対象範囲指定強化 |
| J4 | 複数リポジトリを串刺しにした `query find`（現状は単一ワークスペース前提） |
| J5 | 共有 baseline/suppression の中央管理（現状はリポジトリごとのファイル） |
| J6 | 組織全体の lint ルール採用状況ダッシュボード（どのリポジトリがどのルールを deny/warn しているか） |

---

## K. lint ルール機構のさらなる拡張（8 件）

`.paredit/rules/*.lisp` の `defrule` は実装済み。その上に積む候補。

| # | 候補 |
| --- | --- |
| K1 | カスタムルールのユニットテスト実行を CI ゲートに組み込む標準テンプレート |
| K2 | カスタムルールのパッケージ化・共有（`.paredit/rules/` をパッケージマネージャ経由で配布） |
| K3 | ルールの `:fix` 節が生成する書き換えの安全性を静的に検査する lint-for-lint |
| K4 | `defrule` のパターン言語を `--query` と統合する（[[two-pattern-languages-exist]] の解消） |
| K5 | ルールごとの実行時間計測とワークスペース全体での重いルールの特定 |
| K6 | カスタムルールのバージョニング（`paredit.toml` からピン留め） |
| K7 | 組み込みルールをカスタムルールの記法でオーバーライド・微調整できる仕組み |
| K8 | `deftest` の失敗を `inspect lint --docs` のドキュメントに自動反映（サンプル→ドキュメント同期） |

---

## L. 生成系のさらなる拡張（6 件）

`generate` は 6 種（defpackage/defsystem/tests/accessors/defgeneric/docstring）。

| # | 候補 |
| --- | --- |
| L1 | `generate changelog-entry` — 変更差分から CHANGELOG の下書き（このプロジェクト自身のドッグフーディングにもなる） |
| L2 | `generate api-docs` — docstring から静的な API リファレンスサイトを生成 |
| L3 | `generate condition-hierarchy` — 既存 error クラスからの新規 `define-condition` テンプレート |
| L4 | `generate property-test` — 既存 `deftest` に加えて QuickCheck 風のプロパティテスト骨格 |
| L5 | `generate migration` — `migrate run` の recipe（`.paredit/migrations/*.lisp`）の雛形生成 |
| L6 | `generate benchmark` — 既存関数から criterion 風ベンチマーク骨格（Lisp 側に対応するベンチ機構がある方言向け） |

---

## M. パフォーマンス・スケール（6 件）

| # | 候補 |
| --- | --- |
| M1 | `--cache-dir` の効果測定を公開ベンチマークとして継続計測（[[bench-numbers-swing-between-sessions]] の教訓通り区間で報告） |
| M2 | 並列度の自動調整（現在のコア数固定 vs ファイルサイズ分布に応じた動的分割） |
| M3 | 巨大ファイル（生成コード等）向けのインクリメンタルパース（差分のみ再解析） |
| M4 | `similarity`/`clone-*` 系のデフォルトポリシーの計算量プロファイルをドキュメント化（[[similarity-maximal-overlap-is-quadratic]] の教訓を一般化） |
| M5 | メモリ使用量の上限設定・大規模ワークスペースでのストリーミング処理 |
| M6 | `serve` のキャッシュヒット率・レイテンシのメトリクスエンドポイント（Prometheus 形式） |

---

## N. このリポジトリ自身の開発体験（7 件）

| # | 候補 |
| --- | --- |
| N1 | 新規 `inspect` レポート追加のスキャフォールディング（`xtask` にジェネレータを追加、[[wiring-a-new-inspect-command]] の6ファイル手作業を自動化） |
| N2 | pinned counts（[[adding-a-rule-or-command-trips-pinned-counts]]）の一括更新コマンド |
| N3 | `nix flake check` の 35-40 分（[[nix-flake-check-takes-35-40-minutes]]）を短縮する差分実行モード |
| N4 | contract テスト群（capabilities/architecture/readme 等）の失敗理由を一箇所に集約するレポーター |
| N5 | worktree ベースの並行開発（[[dialect-depth-runs-in-parallel-worktrees]]）を支援する CLI ラッパー |
| N6 | `docs/src/project/feature-candidates.md` のような棚卸し文書の陳腐化を自動検知する仕組み（実装状況を grep で検証し警告） |
| N7 | 契約テストの許可リスト（[[feature-dependency-allowlist-contract]] 等）への追加を促す pre-commit ヒント |

---

## O. 未着手の周辺領域（10 件）

A〜N のどのセクションにも収まらない、まだ触れていない切り口。

| # | 候補 |
| --- | --- |
| O1 | 新方言の追加検討 — Guile Scheme（GNU拡張構文）、Chez Scheme、Shen、Arc、Gerbil Scheme |
| O2 | `.paredit/rules/*.lisp` カスタムルールの実行境界（評価かパターン照合のみか）を契約テストで明文化・監査 |
| O3 | `--output ndjson` — 巨大ワークスペースを1ファイル1行でストリーミング処理するエージェント向け出力 |
| O4 | `paredit history` — リポジトリ横断で過去に適用した edit/refactor/fix/migrate を一覧し、任意の1操作だけをrevertできる仕組み（現状のundoは直近の `refactor step` に限定） |
| O5 | `inspect architecture-diagram` — パッケージ依存・呼び出しグラフ・クラス階層を一枚に合成した俯瞰図 |
| O6 | docstring/コメントの英語以外の言語での一貫性検査（多言語プロジェクト向け） |
| O7 | `paredit tui` のアクセシビリティ（スクリーンリーダー対応、colorblind-safe テーマ） |
| O8 | 方言間の慣用形への「移植」支援 — 例: CL の `loop` を Racket の `for`/`for/list` へ書き換え提案 |
| O9 | `fuzz/` コーパスと lint ルールの相関レポート — どのクラッシュ入力がどのルールで事前に検出できたはずかを `xtask` 経由で集計 |
| O10 | 新規 lint ルールの段階導入プレビュー — `--preview` で既存コードへの finding 数への影響を導入前に見積もる |

---

## P. GitHub Action / CI 統合の拡張（6 件）

`action.yml` は `mode: lint|format|fix` の3種のみ（`fix` は実際には `format` を
`--check` 無しで走らせているだけで、`inspect lint --fix` とは別物）。
`query`/`migrate`/`fix`（lintの書き込み側）を Action から直接使えない。

| # | 候補 |
| --- | --- |
| P1 | `action.yml` に `query`/`migrate`/`lint-fix` モードを追加（現状 lint/format/format-fix の3つのみ） |
| P2 | `refactor plan`/`inspect impact` の結果を PR コメントとして自動投稿し、変更の影響範囲をレビュアーに提示 |
| P3 | `--since` を PR のベースブランチから自動検出するオプション（現状は明示的な git-ref 指定が前提） |
| P4 | composite action に加えて `workflow_call` の reusable workflow としての配布 |
| P5 | GitHub Actions 以外（GitLab CI、CircleCI）向けのテンプレート・使用例の提供 |
| P6 | pre-commit.com フレームワーク向けの `.pre-commit-hooks.yaml` 配布 |

---

## Q. テスト・カバレッジ連携（6 件）

`inspect test-map` はテストと定義の対応を報告するが、実際の実行結果とは繋がっていない。

| # | 候補 |
| --- | --- |
| Q1 | FiveAM/Prove/ERT/`clojure.test` 等の実行結果を `inspect test-map` に統合し、対応表を実測カバレッジにする |
| Q2 | ミューテーションテスト — 既存の `edit`/`refactor` 変換群を使い、既存テストが検出できない変異を報告する `inspect mutation-coverage` |
| Q3 | テスト実行時間のプロファイルとフレーク検出（同一テストを複数回実行し結果の揺れを検知） |
| Q4 | `generate tests` が生成した骨格の充足率トラッキング — TODO のまま放置された生成テストの検出 |
| Q5 | プロパティベーステストの反例から回帰テストケースを自動生成 |
| Q6 | カバレッジと `inspect hotspots`/`debt-score` を突き合わせた「テストされていない複雑な箇所」の優先度付け |

---

## R. マクロ作成支援（6 件）

`inspect macro-hygiene`（変数捕捉検出）はあるが、修正提案側・作成支援側はまだ薄い。

| # | 候補 |
| --- | --- |
| R1 | `once-only`/gensym パターンの適用漏れに対する自動修正コード生成（検出は既存、修正は無い） |
| R2 | 意図的な変数捕捉を行う anaphoric マクロのホワイトリスト管理（誤検知の抑制） |
| R3 | マクロ引数の評価順序・複数回評価バグの検出（`macro-hygiene` の一段深い版） |
| R4 | `defmacro` 呼び出し箇所を `macroexpand-1` 結果でその場に展開する一時的デバッグ変換 |
| R5 | `define-compiler-macro` と対応する関数本体の一貫性検証 |
| R6 | マクロのシグネチャ変更が既存呼び出し箇所を壊すかどうかの後方互換性チェック |

---

## S. VCS/フック連携の深化（5 件）

| # | 候補 |
| --- | --- |
| S1 | `paredit install-hooks` — pre-commit/pre-push フックの雛形をワンコマンドで設置 |
| S2 | `git blame` を使った `refactor plan` のリスクスコアリング（最近変更が集中する箇所ほど慎重に扱う） |
| S3 | コミットメッセージと変更ファイル種別（lint修正/リファクタ/機能追加）の整合性チェック |
| S4 | `--since` の拡張 — 直近マージされた PR の差分を自動検出するショートハンド |
| S5 | リベース・チェリーピック後に `refactor verify` の適用範囲を自動再検証する仕組み |

---

## T. エクスポート形式・相互運用（6 件）

`--output` は sarif/junit/code-climate 等 CI 向けが中心。可視化・他ツール連携向けの形式が薄い。

| # | 候補 |
| --- | --- |
| T1 | LSIF（Language Server Index Format）出力 — GitHub Code Navigation 等への取り込み |
| T2 | ctags/etags 互換出力 — LSP 未対応エディタ向けのフォールバック |
| T3 | コールグラフ/依存グラフの GraphML/GEXF 出力（Gephi 等の外部可視化ツール向け、現状は dot/mermaid のみ） |
| T4 | SBOM（CycloneDX/SPDX）生成 — 依存関係の可監査化 |
| T5 | OpenTelemetry 形式での lint/analysis 実行トレース出力 |
| T6 | Debug Adapter Protocol（DAP）対応の探索的検討 |

---

## U. オンボーディング・学習支援（5 件）

| # | 候補 |
| --- | --- |
| U1 | `paredit learn` — selector/query の文法をインタラクティブに学べるチュートリアルモード |
| U2 | `paredit doctor` — 処理系検出・設定ファイルの妥当性・キャッシュ状態をワンコマンドで診断 |
| U3 | `inspect errors --explain <code>` — エラーコード別のよくある原因と対処をコマンドから直接引ける |
| U4 | 新規参加者向けに `hotspots` ベースで「このリポジトリの複雑な箇所トップ10」ツアーを自動生成 |
| U5 | サンプル方言ファイル集を使った読み取り専用の「素振り」サンドボックスモード |

---

## V. 利用状況の可視化（プライバシー配慮のオプトイン、4 件）

外部送信ではなくローカル集計のみを前提にした自己観測系。開発優先度を実データから決める。

| # | 候補 |
| --- | --- |
| V1 | `paredit stats --usage` — 自分のワークフローでよく使うコマンド/フラグのローカル集計・可視化 |
| V2 | エラー発生頻度のローカルログ — 次に直すべき UX（頻出する refuse 理由）の特定 |
| V3 | 大規模ワークスペースでの実行時間ヒートマップ（どのコマンド・どのファイルがボトルネックか） |
| V4 | オプトインの匿名コマンド利用頻度収集（外部送信は伴わない、ローカルJSON出力のみ） |

---

## W. format / 印字系のさらなる拡張（6 件）

`edit format` は `--indent`/`--max-width`/`--write`/`--diff`/`--check`/`--diff-stat` の6フラグまで
今回確認できたが、印字ポリシー自体の柔軟性はまだ薄い。

| # | 候補 |
| --- | --- |
| W1 | コメント整列 — 行末コメントの列揃えオプション |
| W2 | フォーム種別ごとの `--max-width` プロファイル（`defun` は80、データリテラルは100、等） |
| W3 | `lisp-indent-function` 相当のインデントテーブルを `paredit.toml` でプロジェクト単位に上書き |
| W4 | 空行の正規化ポリシー（連続空行の最大数、トップレベル間の空行数を統一） |
| W5 | ワークスペース全体の `--diff-stat` 集計（現状は1ファイル単位、複数ファイルの変更行数サマリが無いなら追加） |
| W6 | 方言固有の慣用フォーマット（例: Clojure の threading マクロのインデント規則）のオプトイン |

---

## X. 方言固有パッケージエコシステムとの連携（5 件）

| # | 候補 |
| --- | --- |
| X1 | Quicklisp/Ultralisp ローカルディストの依存整合性チェック |
| X2 | Leiningen（`project.clj`）/`deps.edn`/`raco pkg` 等パッケージマネージャコマンドとの橋渡し |
| X3 | Emacs `package-lint`/`checkdoc` 相当ルールの取り込み（既存 Elisp ルールとの重複回避を明示した上で） |
| X4 | ASDF システムの `:depends-on` バージョン制約とロックファイルの整合性チェック |
| X5 | Babashka/Clojure CLI のタスク定義（`bb.edn`）からのタスク一覧取得と `inspect workspace` への統合 |

---

## Y. データ用途の S 式検証（4 件）

コードではなく設定・データとして書かれた S 式（コード解析の対象外）への対応。

| # | 候補 |
| --- | --- |
| Y1 | Emacs customize データ（`custom-set-variables` ブロック）の構造検証 |
| Y2 | EDN（`.edn`）データファイルのスキーマ検証（Clojure コードではなくデータとして） |
| Y3 | Racket のデータ指向 `#lang` 言語への対応拡大（コード用の `#lang racket/base` 以外） |
| Y4 | `.paredit/rules/*.lisp` 等ツール自身が読む S 式設定ファイルの構文検証を `inspect check` から明示的に呼べるオプション |

---

## Z. 依存関係のライセンス監査（3 件）

G2（自リポジトリのライセンスヘッダ）とは別に、外部依存側のライセンスを見る。

| # | 候補 |
| --- | --- |
| Z1 | Quicklisp/ASDF 依存のライセンス一覧化 |
| Z2 | 自プロジェクトのライセンスと依存ライセンスの互換性マトリクス |
| Z3 | SPDX 識別子の一貫性チェック（ライセンスヘッダの文言ではなく識別子として） |

---

## AA. 出力全体のアクセシビリティ（4 件）

`paredit tui` のアクセシビリティ（O7）とは別に、通常の text 出力全体を対象にする。

| # | 候補 |
| --- | --- |
| AA1 | `NO_COLOR` 環境変数対応が全コマンドで一貫しているかの監査・契約テスト化 |
| AA2 | 色覚多様性に配慮した診断重要度の配色ガイドライン化 |
| AA3 | スクリーンリーダー向けの出力構造（見出しの読み上げ順序等）の検証 |
| AA4 | `--output text` の verbosity レベル調整（quiet/normal/verbose） |

---

## AB. AI 生成コードの品質ゲート（4 件）

paredit-cli は既に MCP 経由でエージェントに使われている前提がある
（[[mcp-already-curates-agent-surface]]）。生成コード特有の失敗モードに絞った候補。

| # | 候補 |
| --- | --- |
| AB1 | 既存 `inspect` 群を束ねた一括採点コマンド（LLM 生成コードのレビュー専用プリセット） |
| AB2 | 生成コードにありがちなパターン（過剰なコメント、存在しない関数の呼び出し）の検出ルール |
| AB3 | 生成 → lint → fix → 再 lint のラウンドトリップをワンコマンド化 |
| AB4 | コミット単位で「この変更が lint 状態をどれだけ悪化/改善させたか」のトレンドレポート |

---

## AC. 実行時プロファイラ連携（3 件）

B 節（ライブ処理系連携）の隣接領域。マクロ展開ではなく実測パフォーマンスの取り込み。

| # | 候補 |
| --- | --- |
| AC1 | SBCL の statistical profiler 出力の取り込みと `inspect hotspots` との相関 |
| AC2 | 実測ホットパスに対する `inspect effects`（純粋性解析）の優先実行 |
| AC3 | プロファイル駆動のインライン化候補提案 |
