# 機能追加候補カタログ（2026-08-01 版・重点13区分）

対象: `nerima-lisp/paredit-cli` v1.3.0
目的: 「次に何を作るか」を選ぶための候補の網羅。採否は未決。
候補数: **146**（A〜M の 13 セクション、連番）
各区分の代表候補1件ずつ（計13件）に実装レベルの深掘り（対象ファイル・アプローチ・
依存関係・リスク）を追加済み — 各区分の表の直後の「### 深掘り」を参照。

この版は当初 A〜AC の29セクション・198件で書いていたカタログから、実装優先度の
検討対象として13区分だけを選び、選ばれた区分をさらに項目単位で掘り下げた上で、
区分の文字を **A〜M の連番に振り直した**。除外した区分（元の表記で
B/C/D/E/H/J/L/P/Q/S/T/U/V/X/Z/AB — ライブ処理系連携、監視・常駐ワークフロー、
配布チャネル、エディタ拡張、ドキュメント・可観測性、マルチリポジトリ・組織スケール、
生成系拡張、CI統合、テスト・カバレッジ連携、VCS/フック連携、エクスポート形式、
オンボーディング支援、利用状況可視化、方言パッケージエコシステム、ライセンス監査、
AI生成コード品質ゲート）は今回のスコープ外というだけで、価値が否定されたわけではない。
旧字と新字の対応は次節末尾の表を参照。

---

## 0. 前版との差分 — 何が変わったか

v1.2.1 版は「存在しないもの」として LSP・MCP・設定ファイル・カスタムルール機構・
watch・WASM・構造 diff・エディタ拡張・`format --check` を挙げていたが、
v1.3.0 の実装を `command.rs` / `CHANGELOG.md` で直接確認したところ、
**WASM とエディタ拡張と watch を除く全部が既に存在する**。反省を兼ねて明記する。

| 前版の主張 | 現状（v1.3.0、実地確認済み） |
| --- | --- |
| LSP が無い | `paredit lsp` — 診断・code action・outline・selectionRange・folding・rename 等を持つ LSP 3.17 サーバー |
| MCP が無い | `paredit mcp` — `--read-only` 付き。ただし `docs/src/guide/integrations.md` に未掲載（本版ではドキュメント区分を対象外にしたため未収録） |
| 設定ファイルが無い | `paredit config {check,show,schema,init}` と 5 層の `paredit.toml`（`extends` 対応） |
| カスタムルール機構が無い | `.paredit/rules/*.lisp` に `defrule`/`deftest`/`deprecate` を書く機構が実装済み（→ E 節） |
| 構造 diff が無い | `inspect diff` が実装済み |
| `format --check` が無い | `edit format --check` と `--diff-stat` が実装済み（→ J 節） |
| B1-B5（types/narrowing/constants/value-propagation/effects の露出） | 全て `inspect types` 等として個別コマンド化済み（→ A 節） |
| 方言の深さ（旧 v1.2.1 版の A 節） | v1.3.0 で LFE/Fennel/Janet/Hy/Carp にスコープ解析、Elisp に意味層＋9 lint ルールを追加済み（→ A 節） |
| クローン検出（旧 v1.2.1 版の M 節） | `inspect clone-{classes,sequences,external,threshold,genealogy}` が実装済み（→ B5 で活用側を提案） |
| 名前空間（旧 v1.2.1 版の O 節） | `query`/`fix`/`migrate` は実装済み（前版でも「実装済み」と自己記載していた） |

一方で **今も動かず、コードで直接確認した** 事実:

- `packages/core/semantics/src/semantics/typing/service/declarations.rs:139` — `if dialect != Dialect::CommonLisp { return empty }`。
  型宣言解析は今も CL 専用。`inspect types`/`inspect narrowing` は他方言では常に空を返す（→ A 節）。
- `watch` という語はコード中に検索一致ゼロ。ファイル監視によるインクリメンタル実行は無い（本版では対象外）。
- WASM ターゲットはゼロヒット（本版では対象外）。
- `docs/src/guide/integrations.md` に `mcp` と `tui` のセクションが無い（実装はあるのに、本版では対象外）。

以降は、これらの実地確認を土台にした候補。

### 区分の連番対応表（旧字 → 新字）

29セクション版からこの13区分版に絞り込んだ際の文字の対応。他のメモや過去の会話で
旧字（例: 「G4」「K の fix apply」）を見かけたときはこの表で読み替える。

| 旧字（29区分版） | 新字（本版） | 内容 |
| --- | --- | --- |
| A | A | 意味解析層の方言パリティ |
| F | B | 残る structural edit / refactor 変換 |
| G | C | 新しい分析カテゴリ |
| I | D | エージェント体験の深化 |
| K | E | lint ルール機構のさらなる拡張 |
| M | F | パフォーマンス・スケール |
| N | G | このリポジトリ自身の開発体験 |
| O | H | 未着手の周辺領域 |
| R | I | マクロ作成支援 |
| W | J | format / 印字系のさらなる拡張 |
| Y | K | データ用途の S 式検証 |
| AA | L | 出力全体のアクセシビリティ |
| AC | M | 実行時プロファイラ連携 |

---

## A. 意味解析層の方言パリティ（12 件）

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
| A9 | Fennel/Janet/Hy/Carp/LFE（v1.3.0 でスコープ層のみ追加）への typing 層拡張の要否再評価 — 各方言の型システムの有無を踏まえた優先順位付け |
| A10 | `inspect narrowing` が非 CL 方言で常に空集合を返す挙動を、C14/C18 の `unmodelled` 行と同じ発想で「空」と「未対応」を区別する |
| A11 | `value` 層の定数伝播結果を `inspect effects` の副作用推定にフィードバックする相互作用（現状 2 層は独立計算） |
| A12 | CLOS `defmethod` の引数特化（specializer）を型情報として `typing` 層に取り込む |

### 深掘り: A5 — `inspect effects` の非 CL 対応

`inspect effects`（純粋関数判定）は `packages/feature/semantic-report/src/effect_report`
にレポートとして実装されているが、判定ロジックが依拠する副作用推定は `typing`/`value` と
同じ CommonLisp 分岐（`declarations.rs:139` 相当のガード）に乗っているとみられる。最初の
一歩は Elisp から: `build_binding_table` は既に Elisp 対応済みなので、純粋性判定に必要な
「呼び出しグラフ」と「既知の副作用プリミティブ一覧」を Elisp 版で用意すれば、CL 版の判定
アルゴリズム自体は再利用できる可能性が高い。リスクは「未知の呼び出し先」の扱い —
CL では unresolved call を保守的に「副作用あり」とみなしているはずで、この保守的デフォルト
を各方言に複製し忘れると誤って「純粋」と判定する回帰になる。A1（Elisp typing 層）と
セットで着手するのが自然。

---

## B. 残る structural edit / refactor 変換（20 件）

`paredit.el` パリティ（v1.2.1 版の K 節）は v1.3.0 でほぼ埋まったが、Lisp 方言間の
「よくある書き換え」はまだ手が回っていないものが残る。

| # | 候補 |
| --- | --- |
| B1 | `&optional`/`&key` の相互変換（デフォルト値・destructuring を保持） |
| B2 | 位置引数からキーワード引数への一括変換（呼び出し側も追随） |
| B3 | `dolist`/`dotimes`/`loop` 間の相互変換（意味が保存できる範囲に限定） |
| B4 | ローカル関数のグローバル昇格・グローバル関数のローカル降格 |
| B5 | 重複コードの自動パラメータ化 — `inspect clone-classes` が見つけたクラスから抽出関数を提案 |
| B6 | パッケージの分割・統合（`refactor split-file` はファイル単位、パッケージ境界の再編はまだ無い） |
| B7 | `defmethod` 群からの `defgeneric` 引数リスト精緻化（総称関数のシグネチャ差異検出込み） |
| B8 | マクロの本体を関数に降格する変換（安全な場合に限定するハイジーン検査込み） |
| B9 | `format` 制御文字列の構造化書き換え（`~a` の並びをキーワード引数に展開する等） |
| B10 | let 系束縛からの `defvar`/`defparameter` 抽出（スコープ逸脱している束縛の可視化とセット） |
| B11 | 条件分岐の網羅性を保ったままの `case`→`cond`（あるいは逆）の変換ガード強化 |
| B12 | 複数ファイルにまたがる `edit`/`refactor` のトランザクション化 — 現状 `migrate run` のみ部分失敗耐性がある |
| B13 | `cond` の各節での型絞り込み結果（`narrowing` 層）を使い、後続節で冗長になった型チェックの削除提案 |
| B14 | 連続する `setf` 呼び出しを `psetf`/`rotatef` にまとめる提案 |
| B15 | ネストした `if` を `cond` へ段階的に畳み込む変換（B11 の `case`⇄`cond` とは別に、`if` の入れ子解消） |
| B16 | `let*` の各束縛間に依存が無い箇所を検出し、並列評価可能な `let` へ変換できる部分を提案 |
| B17 | `dotimes` のインデックス変数が本体で書き換えられている危険な使用の検出と防御的コピー挿入 |
| B18 | 複数の `push` 呼び出しをまとめて `nconc`/`append` ベースの一括構築に変換する提案 |
| B19 | `unless`/`when` の否定条件をド・モルガンの法則で能動的に簡約する `edit` 変換（既存 lint の DeMorgan ルールは検出のみ） |
| B20 | `defstruct` と対応する初期化関数（`make-`）の引数順序をスロット定義順に同期させる変換 |

### 深掘り: B5 — クローンクラスからの自動パラメータ化抽出

`inspect clone-classes`（`packages/feature/similarity/src/clone_report`）は既に
「5個の類似フォームが1クラス」まで検出している。B5 はこの検出結果を消費する新しい
`refactor` コマンド（例: `refactor extract-from-clone-class --class-id <id>`）として
素直に積める — クラス内の各メンバーの差分箇所（クローン検出が既に atom 単位のリネーム
差分を `inspect similarity` で報告している）をそのままパラメータ候補として使えるため、
「どこを引数にするか」を新規に推論する必要がない。むずかしいのは安全性判定側:
既存の `extract-function` は単一箇所からの抽出なので mutation_safety の前提が
「1箇所を書き換えて残りを呼び出しに差し替える」だが、B5 は N 箇所を同時に書き換える
必要があり、`refactor verify` のロールバック単位を1トランザクションに広げる必要が
ある（B12 のトランザクション化と依存関係）。

---

## C. 新しい分析カテゴリ（22 件）

`inspect` の既存 231 種は「論理バグ」「重複」「未使用」「型」「効果」に集中している。
まだ触れていない軸。

| # | 候補 |
| --- | --- |
| C1 | シークレットスキャン — `defparameter *api-key* "sk-..."` のような埋め込み秘密情報の検出 |
| C2 | ~~ライセンスヘッダの存在・整合チェック~~ — **実装済み**: `inspect license-headers`（`packages/feature/project-inventory`、既存の `inspect licenses` とは別軸） |
| C3 | ドキュメント文字列カバレッジ（既存の `generate docstring` は生成側、カバレッジ計測側が無い） |
| C4 | テストとプロダクションコードの対応（`inspect test-map` は既存 — 未テスト関数のリスク順ランキングが無いなら追加） |
| C5 | Quicklisp/ASDF 依存の既知脆弱性アドバイザリ照合 |
| C6 | ~~シンボルの export/import 一貫性~~ — **実装済み**: `inspect api-surface`/`api-diff`/`unused-exports`/`duplicate-exports`/`package-boundaries` |
| C7 | ~~循環依存の検出~~ — **実装済み**: `inspect package-cycles`/`call-cycles`/`system-cycles`/`class-cycles`/`struct-cycles`（5コマンドに分かれている） |
| C8 | コメントアウトされたコードの検出・削除提案（`;; (old-code ...)` パターン） |
| C9 | ~~数値リテラルのマジックナンバー検出・`defconstant` への抽出提案~~ — **実装済み**: `inspect magic-numbers`（`packages/feature/semantic-report`） |
| C10 | ~~命名規則の一貫性検査~~ — **実装済み**: `inspect naming`（レポート）と lint ルール `definition-naming` の両方 |
| C11 | 巨大 `let`/`cond`/`case` の分割提案（既存 `debt-score`/`hotspots` の一段掘り下げ） |
| C12 | 副作用を持つトップレベルフォームの実行順序依存性検出（load 順が結果を変える箇所） |
| C13 | ~~condition/error クラス階層の整合性（`define-condition` の継承関係の妥当性）~~ — **実装済み**: lint ルール `define-condition-empty-superclass-list`、`define-condition-missing-report-for-error-type`、`signal-on-error-condition-returns-silently`、`ignore-errors-wraps-non-error-signal`（`packages/feature/lint-condition-system`） |
| C14 | パッケージ間の「循環しないが過度に結合している」度合いの指標化（結合度メトリクス） |
| C15 | 方言横断で統一算出する循環的複雑度と、既存 C11（巨大 `let`/`cond`/`case`）の相関レポート |
| C16 | 同一パッケージ内でのシンボル衝突・シャドーイング（内側の束縛が外側の関数名を隠す等）の検出 |
| C17 | 一度も再代入されない `defparameter`/`defvar` の「実質定数」検出（`defconstant` 化提案とセット） |
| C18 | トップレベルフォームの実行順序に依存しない「宣言的」な書き方への準拠度スコア |
| C19 | ~~`handler-case`/`ignore-errors` が握りつぶすエラー型の広さを検出~~ — **実装済み**: lint ルール `handler-case-swallows-error`（`packages/feature/lint-safety`） |
| C20 | 再帰関数の末尾呼び出し位置の検出と、TCO が保証される方言での最適化可能性の明示 |
| C21 | ~~`defpackage` の `:use` によるシンボル空間の暗黙的な広がり（名前衝突リスク）の可視化~~ — **実装済み**: `inspect use-widening`（`packages/feature/package`） |
| C22 | ~~動的束縛（`special` 変数）のスレッドセーフティ観点でのリスク検出~~ — **実装済み**: lint ルール `global-mutation-in-function`（`packages/feature/lint-safety`） |

### 深掘り: C4 — 未テスト関数のリスク順ランキング

`inspect test-map` は既にテスト定義とプロダクション定義の対応表を持っているはずなので、
C4 の追加分は新しいデータ源を要らない — 既存の `test-map` の出力と `inspect
hotspots`/`debt-score` の出力を同じ定義（関数）キーで JOIN し、「テストが無い」×
「複雑度が高い」の掛け算でソートするだけの合成レポートになる可能性が高く、13区分の
中では最も低コストで着手できる部類。ただし `test-map` が定義とテストの対応をどの粒度
（ファイル単位かトップレベルフォーム単位か）で持っているかを先に確認する必要がある —
ファイル単位までしか対応していない場合、関数単位のランキングを出すには test-map 側の
粒度を先に上げる作業が前提になる。テスト実行結果そのものを test-map に統合する話
（本版のスコープ外にしたテスト連携区分の候補）とは独立して着手可能。

---

## D. エージェント体験の深化（11 件）

MCP は既に厳選済みサーフェス（7 tools + `paredit_run`）であり、CLI 全体を MCP に一対一で
写像する提案はしない（過去に却下済み）。その上でエージェント向けに価値がある候補。

| # | 候補 |
| --- | --- |
| D1 | `refactor plan` の出力に「この変換のリスク見積もり」（影響ファイル数・呼び出し元数から算出）を添える |
| D2 | lint finding や refuse 理由を自然文で説明する `--explain` フラグ（エラーコードの doc_url をその場で展開） |
| D3 | 複数の `edit`/`refactor` 呼び出しをバッチで受け、まとめて一回の再パースで適用するバルク API |
| D4 | エージェントの試行錯誤ログから「よく失敗する selector パターン」を集計し `inspect resolve` の候補提示に使う |
| D5 | `refactor apply`/`fix apply` に dry-run の「想定される次の一手」提案を添える（次に読むべき finding の優先順位） |
| D6 | MCP tool 呼び出しのコスト（トークン数の目安）を tool description に明示 |
| D7 | セッション横断で使う named checkpoint（`refactor step` の命名版、途中経過に戻れる） |
| D8 | 大規模ワークスペースでの部分適用戦略（影響範囲でグルーピングし段階的に fix/migrate を進める） |
| D9 | selector 解決結果のセッション内キャッシュ — 同一ファイルへの繰り返し `resolve` 呼び出しの高速化 |
| D10 | `refactor plan` の JSON 出力に「この操作の後に自然に続く操作」の候補列を添えるワークフロー連鎖提案 |
| D11 | 「今回のコマンドで何が変わり、何が変わらなかったか」を1行要約するエージェント向け compact モード |

### 深掘り: D2 — `--explain` フラグ

エラーコードごとの `doc_url` は既に `docs/src/reference/errors.md`（旧
`docs/src/errors.md`、`af10ef1` で移動済み）の該当見出しへのリンクとして JSON エラー
エンベロープに載っている（契約テストで文書化が保証済み）。`--explain` はこの `doc_url`
が指す Markdown 見出しの本文を、ネットワークアクセスや別プロセス無しに **バイナリに
埋め込んだ同じ Markdown からその場で抜き出して** 表示するだけなので、新しいデータは
要らない。実装の要点は「エラーコードから見出しの本文だけを切り出す」パーサ（見出し
`### code { #code }` から次の `###` までを抜く程度）と、`include_str!` で
`errors.md` をバイナリに埋め込む一手間（`diagnosis.rs` は既に `include_str!` で
Markdown を読んでいる前例がある、`af10ef1` のコミットメッセージ参照）。

---

## E. lint ルール機構のさらなる拡張（11 件）

`.paredit/rules/*.lisp` の `defrule` は実装済み。その上に積む候補。

| # | 候補 |
| --- | --- |
| E1 | カスタムルールのユニットテスト実行を CI ゲートに組み込む標準テンプレート |
| E2 | カスタムルールのパッケージ化・共有（`.paredit/rules/` をパッケージマネージャ経由で配布） |
| E3 | ルールの `:fix` 節が生成する書き換えの安全性を静的に検査する lint-for-lint |
| E4 | `defrule` のパターン言語を `--query` と統合する（[[two-pattern-languages-exist]] の解消） |
| E5 | ルールごとの実行時間計測とワークスペース全体での重いルールの特定 |
| E6 | カスタムルールのバージョニング（`paredit.toml` からピン留め） |
| E7 | 組み込みルールをカスタムルールの記法でオーバーライド・微調整できる仕組み |
| E8 | `deftest` の失敗を `inspect lint --docs` のドキュメントに自動反映（サンプル→ドキュメント同期） |
| E9 | ルールの重大度（severity）をコードベース規模・既存 finding 密度から動的に提案する仕組み |
| E10 | `--query` 側にある型制約付きキャプチャ（`?x:number` 等）を `defrule` のパターンでも使えるよう統合（E4 の前段） |
| E11 | カスタムルールの実行結果が `--baseline`/`--suppress-path` と組み込みルールと区別なく扱われているかの契約テスト明示化 |

### 深掘り: E4 — `defrule` パターン言語と `--query` の統合

`.paredit/rules/*.lisp` の `defrule`（`packages/feature/lint-custom/src/pass.rs`/
`ruleset.rs`）と `--query '(defun ?name ...)'`（`packages/feature/query/src`）は
[[two-pattern-languages-exist]] の通り別実装。E4 は「両者を1つの構文にする」ではなく
「`defrule` の `:pattern` 節が `--query` のパーサ/マッチャを内部で呼ぶようにする」
方向が現実的 — `--query` 側は既にキャプチャ制約（`?x:list` 等）・後方参照
（同名キャプチャの等値制約）・`...`/`?body...` を持っており機能が上位互換なので、
`defrule` 側のパターンパーサを丸ごと `--query` のものに差し替えれば良い。リスクは
既存の `.paredit/rules/*.lisp` の後方互換性 — 両パーサの構文が完全一致していない
場合、既存ルールファイルが書き換えなしで動くかを `deftest` で全数検査する必要がある。

---

## F. パフォーマンス・スケール（9 件）

| # | 候補 |
| --- | --- |
| F1 | `--cache-dir` の効果測定を公開ベンチマークとして継続計測（[[bench-numbers-swing-between-sessions]] の教訓通り区間で報告） |
| F2 | 並列度の自動調整（現在のコア数固定 vs ファイルサイズ分布に応じた動的分割） |
| F3 | 巨大ファイル（生成コード等）向けのインクリメンタルパース（差分のみ再解析） |
| F4 | `similarity`/`clone-*` 系のデフォルトポリシーの計算量プロファイルをドキュメント化（[[similarity-maximal-overlap-is-quadratic]] の教訓を一般化） |
| F5 | メモリ使用量の上限設定・大規模ワークスペースでのストリーミング処理 |
| F6 | `serve` のキャッシュヒット率・レイテンシのメトリクスエンドポイント（Prometheus 形式） |
| F7 | `inspect similarity`/`clone-*` のインクリメンタル計算 — 前回結果のキャッシュに対する差分更新 |
| F8 | 並列実行時のログ出力順序の安定化（マルチスレッドで出力が入り乱れないかの監査） |
| F9 | 巨大ファイル1本のパースにおけるメモリピーク計測と、ストリーミングパーサ化の要否判定 |

### 深掘り: F7 — クローン検出のインクリメンタル計算

`clone-classes`/`clone-sequences`（`packages/feature/similarity/src/clone_report`）は
全ペア比較に近い計算量になりやすい（[[similarity-maximal-overlap-is-quadratic]] の
教訓通り）。`--cache-dir` は既に「選択された file set・設定・ツリーの変更」をキーに
した再利用機構を持っているので、素直な一歩はクローン検出の中間結果（正規化済みの
フォームのハッシュ表）もこのキャッシュに載せること — ファイル単位で正規化ハッシュを
キャッシュしておけば、1ファイルの変更時に「そのファイルを含むペアだけ」を再計算すれば
良くなる。ただしクローンクラス自体はファイルをまたいだグルーピングなので、1ファイルの
変更が別ファイルの所属クラスを変える場合がある — 「このファイルの変更で影響を受ける
既存クラス」を安全に特定できないと、キャッシュヒットが誤った結果を返す。

---

## G. このリポジトリ自身の開発体験（10 件）

| # | 候補 |
| --- | --- |
| G1 | 新規 `inspect` レポート追加のスキャフォールディング（`xtask` にジェネレータを追加、[[wiring-a-new-inspect-command]] の6ファイル手作業を自動化） |
| G2 | pinned counts（[[adding-a-rule-or-command-trips-pinned-counts]]）の一括更新コマンド |
| G3 | `nix flake check` の 35-40 分（[[nix-flake-check-takes-35-40-minutes]]）を短縮する差分実行モード |
| G4 | contract テスト群（capabilities/architecture/readme 等）の失敗理由を一箇所に集約するレポーター |
| G5 | worktree ベースの並行開発（[[dialect-depth-runs-in-parallel-worktrees]]）を支援する CLI ラッパー |
| G6 | `docs/src/project/feature-candidates.md` のような棚卸し文書の陳腐化を自動検知する仕組み（実装状況を grep で検証し警告） |
| G7 | 契約テストの許可リスト（[[feature-dependency-allowlist-contract]] 等）への追加を促す pre-commit ヒント |
| G8 | `xtask` のスキャフォールディングが生成するボイラープレートに対する「生成直後は必ず契約テストを通る」保証テスト |
| G9 | 全パッケージの依存グラフを `inspect dependencies` 自身に食わせて自己診断するドッグフーディング CI |
| G10 | CHANGELOG.md を廃止し GitHub Release description を正典にした直近の規約変更（[[xtask-checklist-rewrites-pinned-count-docs]] と同根）に合わせ、`release.yml` が生成する draft 本文の検証を契約テストとして強化 |

### 深掘り: G1 — 新規 `inspect` レポート追加のスキャフォールディング

[[wiring-a-new-inspect-command]] の通り、新規 `inspect` コマンド追加は
`src/presentation/cli.rs`・`command.rs`・`dispatch.rs`・`contract.rs`（2箇所）・
`tests/cli/dialect_contract.rs`（複数箇所）・`docs/src/reference/api.md`
（`af10ef1` で `commands.md` から改名済み）の**6ファイル手作業**。`xtask` は
既に `new_command.rs`/`new_lint_rule.rs` というスキャフォールディング基盤を
持っているので、G1 はゼロから作るのではなく `new_lint_rule.rs` のパターン
（雛形生成＋pinned count の自動インクリメント）を `inspect` レポート版に複製する
作業に近い。一番の価値は「6ファイルのうちどれか1つを機械的に見落とす」という
[[adding-a-rule-or-command-trips-pinned-counts]] の恒常的な事故を構造的に防げる点
— xtask が生成した差分がそのまま `dialect_contract.rs` を通ることを xtask 自身の
テストで保証すれば、G8（生成直後の契約通過保証）も同時に満たせる。

---

## H. 未着手の周辺領域（13 件）

他のどの区分にも収まらない、まだ触れていない切り口。

| # | 候補 |
| --- | --- |
| H1 | 新方言の追加検討 — Guile Scheme（GNU拡張構文）、Chez Scheme、Shen、Arc、Gerbil Scheme |
| H2 | `.paredit/rules/*.lisp` カスタムルールの実行境界（評価かパターン照合のみか）を契約テストで明文化・監査 |
| H3 | `--output ndjson` — 巨大ワークスペースを1ファイル1行でストリーミング処理するエージェント向け出力 |
| H4 | `paredit history` — リポジトリ横断で過去に適用した edit/refactor/fix/migrate を一覧し、任意の1操作だけをrevertできる仕組み（現状のundoは直近の `refactor step` に限定） |
| H5 | `inspect architecture-diagram` — パッケージ依存・呼び出しグラフ・クラス階層を一枚に合成した俯瞰図 |
| H6 | docstring/コメントの英語以外の言語での一貫性検査（多言語プロジェクト向け） |
| H7 | `paredit tui` のアクセシビリティ（スクリーンリーダー対応、colorblind-safe テーマ） |
| H8 | 方言間の慣用形への「移植」支援 — 例: CL の `loop` を Racket の `for`/`for/list` へ書き換え提案 |
| H9 | `fuzz/` コーパスと lint ルールの相関レポート — どのクラッシュ入力がどのルールで事前に検出できたはずかを `xtask` 経由で集計 |
| H10 | 新規 lint ルールの段階導入プレビュー — `--preview` で既存コードへの finding 数への影響を導入前に見積もる |
| H11 | `.paredit/` 配下（rules/migrations/kill-ring 等）の全体像を一覧する `inspect dotfiles` 的コマンド |
| H12 | 複数の `--select` 表現を組み合わせた集合演算（AND/OR/NOT）による複合 selector |
| H13 | `refactor` 適用後の「意図した変更点」と「実際の差分」を突き合わせ、意図しない副作用を警告する仕組み |

### 深掘り: H4 — `paredit history`

現状の undo は `refactor step`/`refactor undo` が直近の1操作（または1ワークフロー内の
ステップ列）に限定されている（`packages/feature/refactor-workflow`）。`paredit history`
はこれをリポジトリ横断のログに格上げする話で、技術的には「適用済み操作のメタデータ
（コマンド・対象ファイル・適用前後のハッシュ・タイムスタンプ）をどこに永続化するか」が
最初の設計判断になる。`.paredit/kill-ring.json`（既存の `edit copy --to-ring` が使う
永続化先）と同じ「リポジトリ相対のドットファイル」パターンを踏襲するのが一貫性がある。
revert の実装自体は難しくない（記録した適用前ハッシュへ戻すだけ）が、「記録後に他の
手動編集が同じファイルに入った場合」の扱いが誤って上書きするリスクの本丸 — reparse
ガードと同じ発想で、revert 対象ファイルの現在のハッシュが記録時の「適用後ハッシュ」と
一致する場合のみ安全に戻せる、という制約が要る。

---

## I. マクロ作成支援（9 件）

`inspect macro-hygiene`（変数捕捉検出）はあるが、修正提案側・作成支援側はまだ薄い。

| # | 候補 |
| --- | --- |
| I1 | `once-only`/gensym パターンの適用漏れに対する自動修正コード生成（検出は既存、修正は無い） |
| I2 | 意図的な変数捕捉を行う anaphoric マクロのホワイトリスト管理（誤検知の抑制） |
| I3 | マクロ引数の評価順序・複数回評価バグの検出（`macro-hygiene` の一段深い版） |
| I4 | `defmacro` 呼び出し箇所を `macroexpand-1` 結果でその場に展開する一時的デバッグ変換 |
| I5 | `define-compiler-macro` と対応する関数本体の一貫性検証 |
| I6 | マクロのシグネチャ変更が既存呼び出し箇所を壊すかどうかの後方互換性チェック |
| I7 | `symbol-macrolet`/`define-symbol-macro` の展開結果を hygiene 検査の対象に含める |
| I8 | マクロが生成する束縛変数名の衝突可能性をユーザーコードとの突き合わせで検出（既存 gensym 検出のさらに一歩先） |
| I9 | マクロのドキュメント文字列（あれば）とマクロ本体の実際の引数使用パターンの整合性チェック |

### 深掘り: I1 — once-only/gensym 適用漏れの自動修正生成

検出側は `inspect macro-hygiene`（`packages/feature/lisp-analysis/src/
macro_hygiene_report`）が既に持っているので、I1 は「検出済みの findings を消費して
`edit` 変換を出す」新しい `refactor`/`fix` コマンドとして積める（E 節の `fix apply`
と同じ「検出コマンドの書き込み側」パターン）。むずかしいのは機械的な `gensym` 挿入
だけでは「衛生的だが読みにくい」コードになりがちな点 — `once-only`（`lisp-macro`
スキルが参照する On Lisp のテクニック）まで持ち込むには、複数回評価されている変数を
`once-only` の外側の束縛リストにまとめて上げる変換が要り、単純な gensym 置換より
一段複雑。まずは `gensym` 単体の挿入（安全側に倒した最小実装）を出し、`once-only`
適用は I3（複数回評価バグの検出）と合わせて第二段階にするのが妥当。

---

## J. format / 印字系のさらなる拡張（9 件）

`edit format` は `--indent`/`--max-width`/`--write`/`--diff`/`--check`/`--diff-stat` の6フラグまで
確認できたが、印字ポリシー自体の柔軟性はまだ薄い。

| # | 候補 |
| --- | --- |
| J1 | コメント整列 — 行末コメントの列揃えオプション |
| J2 | フォーム種別ごとの `--max-width` プロファイル（`defun` は80、データリテラルは100、等） |
| J3 | `lisp-indent-function` 相当のインデントテーブルを `paredit.toml` でプロジェクト単位に上書き |
| J4 | 空行の正規化ポリシー（連続空行の最大数、トップレベル間の空行数を統一） |
| J5 | ワークスペース全体の `--diff-stat` 集計（現状は1ファイル単位、複数ファイルの変更行数サマリが無いなら追加） |
| J6 | 方言固有の慣用フォーマット（例: Clojure の threading マクロのインデント規則）のオプトイン |
| J7 | ~~`edit format` のカラム幅計算が全角文字（日本語コメント等）を正しく考慮しているかの監査~~ — **監査済み**: 全て東アジア幅（`unicode_width`）ベースで、文字数・バイト数ベースの箇所は無い（下記参照） |
| J8 | 複数行文字列リテラル内のインデント保持ポリシーの明文化・設定化 |
| J9 | `#\|...\|#` ブロックコメントのインデント正規化オプション |

### 深掘り: J7 — カラム幅計算の全角文字監査（監査済み）

監査の結論は「既に正しい」。フォーマッタのカラム計算は例外なく東アジア幅
（East Asian Width, UAX #11）ベースであり、`str::chars().count()` 相当の
「1文字=1カラム」も、バイト長も、幅として使われている箇所は無い。`--max-width`
（既定80）の判定は `Bounded::push_str`（`formatter/core.rs`）で
`UnicodeWidthStr::width` を積算し、行頭カラムは `Formatter::last_line_width` が
同じ関数で測り、`--comment-column` の詰め物、兄弟の桁揃え
（`formatter/lists/general.rs`）、`#|...|#` ブロックコメントの再インデントも
すべて同じ経路を通る。したがって修正も、ゴールデンテストの再生成も不要。

裏付けとなるテスト:

- `wraps_when_display_width_itself_exceeds_the_budget` — 全角文字により
  `--max-width` を超える行が折り返される（従来から存在）
- `binding_continuation_columns_count_display_width_not_bytes` — 束縛の継続行が
  バイト長ではなく表示幅で揃う
- `a_hugged_form_starts_at_its_display_column_not_its_byte_offset` — 演算子行に
  抱き込まれたフォームの開始カラム
- `tests/corpus.rs` の不変条件5（囲みデリミタより左に出る行が無いこと）は
  `display_width` でカラムを測るので、コーパス全体に対する常時監査になっている

残る関連候補は J8（複数行文字列リテラル内のインデント保持）で、これは幅計算では
なく「どの行を verbatim 扱いするか」の問題。

---

## K. データ用途の S 式検証（7 件）

コードではなく設定・データとして書かれた S 式（コード解析の対象外）への対応。

| # | 候補 |
| --- | --- |
| K1 | Emacs customize データ（`custom-set-variables` ブロック）の構造検証 |
| K2 | EDN（`.edn`）データファイルのスキーマ検証（Clojure コードではなくデータとして） |
| K3 | Racket のデータ指向 `#lang` 言語への対応拡大（コード用の `#lang racket/base` 以外） |
| K4 | `.paredit/rules/*.lisp` 等ツール自身が読む S 式設定ファイルの構文検証を `inspect check` から明示的に呼べるオプション |
| K5 | S 式で書かれた独自データフォーマット全般（TOML 風の設定等）へのスキーマ検証 API の一般化 |
| K6 | `.paredit/kill-ring.json` のようなツール自身が書き出すデータファイルの破損検出・自己修復 |
| K7 | `paredit.toml` と S 式データファイルの間で同じ設定を二重管理している場合の不整合検出 |

### 深掘り: K4 — ツール自身の S 式設定ファイルの構文検証

`.paredit/rules/*.lisp`（E 節）・`.paredit/migrations/*.lisp`（v1.2.1 版の L5 相当）は
どちらも「`paredit` 自身が読む Lisp コード」であり、`inspect check`（構文検証）の
対象ファイル選択（`--from-manifest`/`--include` 等の既存フラグ）には現状含まれて
いないはず — ワークスペース探索がソースコードの拡張子・ディレクトリ構造を前提に
しているため、`.paredit/` 配下は素通りされている可能性が高い。K4 の最小実装は
「`inspect check --paredit-config` のような明示フラグで `.paredit/` 配下だけを
対象に構文検証を回す」ことで、既存のワークスペース選択ロジックを一切変えずに
済む。将来的に H11（`.paredit/` 全体像コマンド）と統合すれば、「設定ファイル自体が
壊れている」を診断する唯一の入口になる。

---

## L. 出力全体のアクセシビリティ（7 件）

`paredit tui` のアクセシビリティ（H7）とは別に、通常の text 出力全体を対象にする。

| # | 候補 |
| --- | --- |
| L1 | `NO_COLOR` 環境変数対応が全コマンドで一貫しているかの監査・契約テスト化 |
| L2 | 色覚多様性に配慮した診断重要度の配色ガイドライン化 |
| L3 | スクリーンリーダー向けの出力構造（見出しの読み上げ順序等）の検証 |
| L4 | `--output text` の verbosity レベル調整（quiet/normal/verbose） |
| L5 | `--output json` のフィールド順序・キー命名の一貫性を保証する契約テスト |
| L6 | 診断メッセージの表現レベル（専門用語の言い換え）を `--explain`（D2）とは別軸で調整できるオプション |
| L7 | 端末幅が極端に狭い環境（80列未満）での text 出力の折り返し保証 |

### 深掘り: L1 — `NO_COLOR` 一貫性の監査・契約テスト化

`NO_COLOR` は `packages/core/cli/src/color.rs` に実装があり、`src/presentation/
cli.rs` からも参照されている — つまり集約点は既に1箇所にある。L1 の作業は
「新しいカラー処理を書く」ことではなく、**全コマンドが本当にこの1箇所を経由して
色を出力しているか**を検査する契約テストを書くことに近い。リスクが高いのは
lint の `--sarif`/`--github` や `edit format --diff` のような「独自のフォーマッタを
持つコマンド」— これらが `color.rs` を経由せず自前で ANSI エスケープを埋め込んで
いた場合、`NO_COLOR=1` でも色が消えない回帰を起こす。契約テストの形は
`grep -rn '\x1b\['`（生の ANSI エスケープ）が `color.rs` の外に出現しないことを
確認する静的検査が現実的で、13区分の中でも実装コストが低く即着手できる部類。

---

## M. 実行時プロファイラ連携（6 件）

ライブ処理系連携（本版では対象外にした区分）の隣接領域。マクロ展開ではなく実測
パフォーマンスの取り込み。

| # | 候補 |
| --- | --- |
| M1 | SBCL の statistical profiler 出力の取り込みと `inspect hotspots` との相関 |
| M2 | 実測ホットパスに対する `inspect effects`（純粋性解析）の優先実行 |
| M3 | プロファイル駆動のインライン化候補提案 |
| M4 | Linux `perf` 出力の取り込みと、SBCL 以外の処理系（Guile 等）への対応拡大 |
| M5 | プロファイル結果を `inspect debt-score` のスコアリングに重み付けとして反映する統合 |
| M6 | ベンチマーク（criterion）実行結果の履歴とプロファイル結果を突き合わせた回帰原因の切り分け支援 |

### 深掘り: M1 — SBCL statistical profiler 出力の取り込み

SBCL の `sb-sprof` は決定論的でないサンプリングプロファイラで、出力形式は
関数ごとの自己時間・累積時間の表（`sb-sprof:report`）。取り込みの現実的な形は
ライブ処理系連携（本版では対象外にした区分）と同じく「SBCL 側でユーザーが
プロファイルを取り、その出力ファイルを `paredit inspect hotspots --profile <file>`
に食わせる」非対話型の連携で、そちらのようなライブ接続（SWANK）は不要 —
プロファイラの実行自体は SBCL 側の責務のまま、paredit-cli は結果ファイルの
パーサだけを持てば良い。むずかしいのは「プロファイラが報告する関数名」と
「`inspect hotspots` が知っている定義（S 式の `defun` フォーム）」の対応付け —
インライン化された関数やコンパイラが生成した無名関数はプロファイラの出力と
ソースの対応が1対1にならないことがあり、対応が取れない行を silent に無視せず
「対応不能」として報告する設計が要る（C14/A10 と同じ `unmodelled` の発想を
踏襲できる）。
