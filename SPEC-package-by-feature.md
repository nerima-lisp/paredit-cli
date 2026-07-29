# Package-by-feature 移行仕様書

対象リポジトリ: `nerima-lisp/paredit-cli`
前提調査: `RESEARCH.md`（本書はその再検証と、実行可能な手順への落とし込み）

---

## 0. この文書の位置づけ

`RESEARCH.md` は「package-by-feature への移行は妥当か」を問うた。本書はその問いを
**実測でフラットに引き直した上で**、採否の判断材料と、採る場合の具体的な移行手順を示す。

`RESEARCH.md` の主要な事実認識（単一クレート・4層構成・規模・crane 未導入）は再検証の結果
すべて正しかった。本書はそこに **依存グラフの実測**と**ビルド時間の実測**を追加し、
結論の一部を修正する。

### 本書が扱う範囲

| # | ワークストリーム | 節 | 独立性 |
| --- | --- | --- | --- |
| A | `packages/{core,feature}/<name>` への workspace 分割 | §1〜§7 | 本体 |
| B | 全パッケージへの README 必須化 | §3.3 | A に従属 |
| C | crane による Nix ビルドの再構成 | §1.3 / Phase 2.5 | **A から独立**。単独で実施可 |
| D | 型の厳格化（`anyhow` → `thiserror` ほか） | §9 | 一部は A と同時が最安 |
| E | Rust ベストプラクティスに沿った機械的修正 | §9.5 / Phase −1 | **A から独立**。A より先に実施 |

**C・E は A を中止しても成果が残る。** 逆に D の中核（`anyhow` → `thiserror`）は
**パッケージ境界が決まって初めて意味を持つ**ため、A と不可分に進める（§9.5）。

すべて実測に基づく。推測で書いた箇所は §5.2.2 の lint 分割軸のみで、
そこには明示的にその旨を記した。

---

## 1. 結論

### 1.1 移行は妥当。ただし目的を組み替えること

**ビルド/テスト時間の短縮を第一目的にするなら、この移行は割に合わない。**
実測（§2.3）では、209k 行・2031 ファイルの単一クレートが cold 49 秒 / 1 ファイル変更の
incremental 11 秒でチェックを終える。分割で削れるのはこの一部にすぎず、
一方で移行コストは §7 のとおり大きい。

**採るべき目的は次の 3 つである。**

| 目的 | 現状の問題 | 分割で得られるもの |
| --- | --- | --- |
| 変更の局所性 | `src/domain/mod.rs` が 220 行の `pub mod` 宣言、1 機能の変更が 3 層 4 ディレクトリに散る | 1 機能 = 1 ディレクトリ。追加・削除が `packages/feature/<name>/` の中で閉じる |
| 境界の強制 | 現状どの domain モジュールも他の任意の domain モジュールを import できる。層のルールは守られているが**機能間の境界は存在しない** | `Cargo.toml` の `[dependencies]` が機能間依存の唯一の宣言箇所になり、コンパイラが強制する |
| 並行作業 | 複数エージェント/開発者が同じ `mod.rs`・`command.rs`・`dispatch.rs` を編集して衝突する | 合成ポイント以外は独立ディレクトリ。衝突面が縮む |

つまりこれは **ビルド最適化ではなくモジュラリティの投資**である。仕様書・PR の説明・
`docs/src/architecture.md` の書き換えは、すべてこの前提で書くこと。

### 1.2 分割は技術的に容易な部類

依存グラフの実測（§2.2）が異例に良好である。

- 206 の feature スライスのうち **146 が他 feature への依存ゼロ**
- feature 間の依存辺はわずか **89 本**（総当たり 42,230 通りに対して）
- **相互循環は 1 組のみ**（`call_cycle_report ↔ package_cycle_report`。同一パッケージに同居させれば解消）

クレート分割を頓挫させる最大要因は循環依存だが、本リポジトリにはほぼ存在しない。

### 1.3 crane は導入する。ただし目的は「feature 単位キャッシュ」ではない

`RESEARCH.md` は crane を「workspace 分割後に各メンバーを個別デリベーションにする手段」
として検討していた。その効果は §1.1 と同じ理由で限定的である。
**しかし調査の過程で、workspace 化とは独立した crane の導入価値が見つかった。**

現行 `flake.nix` は `clippy` と `nextest` を `package` derivation の `overrideAttrs` で
定義している（`flake.nix` の `checks` セクション）。

```nix
clippy  = (self.packages.${system}.default).overrideAttrs (old: { buildPhase = "cargo clippy ..."; });
nextest = (self.packages.${system}.default).overrideAttrs (old: { buildPhase = "cargo nextest run ..."; });
msrv    = mkPareditWithPlatform pkgs msrvRustPlatform;
package = self.packages.${system}.default;
```

これは `nix flake check` のたびに **依存クレート 157 個を 4 回ビルドしている**ことを意味する。
crane の `buildDepsOnly` は `package` / `clippy` / `nextest` で `cargoArtifacts` を共有するため、
**同一ツールチェーンの 3 重ビルドが 1 回に減る**
（`msrv` は別ツールチェーンなので独自の `cargoArtifacts` が必要）。

**削減幅は正直に見積もること。** 実測（§2.3）では:

| | wall | 全体に占める割合 |
| --- | ---: | ---: |
| 依存のみ release ビルド | 43.8 s | 18% |
| 自コード込み release ビルド全体 | 239.8 s | 100% |

crane が消せるのは **依存ビルド 2 回分 ≒ 88 秒**であり、自コードの 4 重ビルド（残り 82%）は
**crane では消えない**（clippy / nextest / buildPackage はそれぞれ異なるコンパイルを行うため）。
`nix flake check` の直列相当時間を約 960 秒とすると **削減率はおよそ 9%**。

小さいが、この 9% は **workspace 化を一切行わなくても得られ、PR 1 本・`flake.nix` のみの変更で、
revert 1 回で戻せる**。費用対効果としては十分に見合う。

したがって crane の導入根拠は次のとおり整理される。

| crane の効果 | 本リポジトリでの評価 |
| --- | --- |
| **依存ビルドの共有（`buildDepsOnly`）** | **有効。** workspace 化と無関係に今すぐ効く。約 88 秒 / `nix flake check`（≒9%）の削減見込み |
| `cargoAudit` のサンドボックス化 | **見送り。** hermetic にはなるが、契約テストの期待値更新が必要で（§2.5.4）、`advisory-db` の鮮度が `flake.lock` 更新周期に縛られる（§8-6） |
| feature メンバー単位の差分キャッシュ | **限定的。** §2.4 のとおり E2E テスト 50,492 行は合成バイナリに依存し続ける |
| `src` fileset の細分化 | **やらない。** §2.5.2 のとおり contract test が README/docs/action.yml をフィクスチャとして読むため |

導入は **workspace 化とは独立した Phase として、Phase 2 の直後に置く**（§6 Phase 2.5）。
`RESEARCH.md` の「crane 導入とクレート構造変更は独立したリスクなので段階を分ける」という
判断は正しく、本書もそれを踏襲する。

---

## 2. 実測データ

計測日: 2026-07-26 / ブランチ `feat/semantic-analysis-layer` / macOS aarch64

### 2.1 規模

| 層 | ファイル数 | 行数 |
| --- | ---: | ---: |
| `src/domain` | 871 | 149,177 |
| `src/presentation` | 877 | 48,441 |
| `src/application` | 274 | 10,258 |
| `src/infrastructure` | 7 | 1,631 |
| **src 合計** | **2,031** | **209,507** |
| `tests/` | 235 ファイル | 50,492 |

### 2.2 feature 境界は既に存在する

`ls src/application/usecase` と `ls src/presentation/cli` のディレクトリ名を突き合わせると
**206 個が完全一致**した。これが本移行の出発点である。

```
application/usecase にのみ存在 (17): call_graph_report, definition_report, impact_report,
  let_report, package_report, semantic_coverage, signature_report, ... (mod.rs 含む)
presentation/cli にのみ存在 (17): args, io, diff, shared, dispatch, command, contract,
  gate, macos_acl, refactor, basic_edit, ... (= 共有カーネル & 合成ルート)
一致 (206): = feature スライス
```

各スライスの実体（例: `call_report`）:

```
src/domain/call_report.rs                   ドメインロジック
src/application/usecase/call_report/        mod.rs + tests/{basics,property,...}.rs
src/presentation/cli/call_report/           args.rs workflow.rs render.rs types.rs mod.rs
src/domain/lint/rules/call_report.rs        （lint ルールを持つ機能のみ）
tests/cli/call_report.rs                    E2E テスト
```

**スライスあたり行数**: 中央値 481 / 2,000 行超は 9 件のみ / 500 行未満が 132 件。

**feature 間依存グラフ**:

| 指標 | 値 |
| --- | ---: |
| スライス数 | 206 |
| feature 間依存辺 | 89 |
| 他 feature への依存を持たないスライス | 146 |
| 相互循環（2-cycle） | 1 組 |
| 連結成分数 | 140（うち 121 が単独スライス） |

最大の連結成分は 25 スライス / 15,600 行のプロジェクト解析クラスタ
（`call_graph_report`, `*_cycle_report`, `dependency_report`, `impact_report`,
`workspace_report`, `signature_report`, `complexity_report` 等）。

### 2.3 ビルド時間（分割の動機にならないことの根拠）

| 計測 | wall | CPU |
| --- | ---: | ---: |
| `cargo check --all-targets` cold（target 全消去） | **48.8 s** | 106 s / 251% |
| `cargo test --no-run` cold（テストバイナリまでリンク） | +58.3 s | 186 s / 341% |
| `cargo check --all-targets`（lint ルール 1 ファイル touch 後） | **11.4 s** | — |
| `cargo test --no-run`（lib 変更後） | 23.1 s | — |
| **依存クレートのみ** の release ビルド（空 lib/bin で 157 パッケージ） | **43.8 s** | 141 s / 362% |
| `cargo build --release --all-targets` cold（自コード込み・`lto="fat"`） | **239.8 s** | 652 s / 282% |

→ **release ビルド全体に占める依存クレートの割合は wall で 18%、CPU で 22%。**
残る 8 割は自コード（209k 行 + `lto = "fat"` / `codegen-units = 1`）である。

CI は `nix flake check` 一本（`clippy` / `nextest` / `msrv` / `package` を並列実行）で
タイムアウト 30 分設定。**自コードのコンパイル自体はボトルネックではないが、
依存クレート 157 個が 4 つの check で重複ビルドされている**（§1.3）。
これが crane 導入の唯一かつ十分な根拠である。

### 2.4 テストの構造（移行の最大の制約）

- `tests/cli.rs` が **単一の統合テストバイナリ**。235 個の `#[path = "cli/*.rs"] mod` を宣言。
- 内容は全て `assert_cmd::Command` による **実 `paredit` バイナリの起動**。
- `src` 側には別途 **339 ファイルに `#[cfg(test)] mod tests`**（ユニットテスト）が存在。

→ **feature クレートに移せるのはユニットテストのみ。** E2E テストはバイナリを合成する
ルートパッケージに残る。`cargo test -p feature-rename` で回るのはユニットテストだけ、
という制約を最初から仕様に織り込むこと。

### 2.5 可視性の実測（機械的コストの主因）

| 指定 | 出現数 | 移行時の扱い |
| --- | ---: | --- |
| `pub(in crate::presentation::cli)` | **963** | クレート境界を越えるため全て `pub` 化が必要 |
| その他 `pub(in crate::...)` | 977（177 種のパス） | パスをクレート内相対に書き換え |
| `pub(crate)` | 575 | 移送先クレートの外から使われるものは `pub` 化 |
| `pub(super)` | 1,943 | 親子関係が同一クレート内に保たれる限り**変更不要** |

`pub(in ...)` 系 1,940 箇所が本移行の機械的作業量の中心。ただし
**コンパイラが漏れなくエラーで指摘する**ため、危険な作業ではない（§7.1）。

### 2.6 移行を制約する既存の契約テスト

| ファイル | 検査内容 | 影響 |
| --- | --- | --- |
| `tests/cli/crate_metadata_contract.rs` | ルート `Cargo.toml` に `name = "paredit-cli"` / `publish = false` / `rust-version = "1.85"` 等の文字列が存在すること、`src/lib.rs` に `pub use domain::dialect;` `pub use domain::sexpr;` が存在すること | **§4.1 の設計により無改修で通る** |
| `tests/cli/public_module_docs_contract.rs` | `src/{domain,application,infrastructure,presentation}/mod.rs` 等 8 ファイルの先頭 doc コメント文字列 | ルートに façade として残すため無改修で通る |
| `tests/cli/public_api_docs_contract.rs` ほか | README / docs / action.yml との整合 | 変更なし |
| `flake.nix` | `src = ./.`（リポジトリ全体が 1 ビルド入力） | 変更なし。§1.3 のとおり fileset 細分化はしない |

---

## 3. 目標とするディレクトリ構造

```
paredit-cli/
├── Cargo.toml              # [workspace] + [package] paredit-cli（ルートパッケージ兼合成ルート）
├── Cargo.lock
├── src/                    # ルートのみ旧 4 層構造を保つ（façade のため / §4.1）
│   ├── lib.rs              # façade: 旧 4 層のパスを再エクスポートし公開 API を維持
│   ├── main.rs             # 変更なし
│   ├── domain/mod.rs       # `pub use paredit_core_sexpr as sexpr;` 等の再エクスポートのみ
│   ├── application/mod.rs  # 同上
│   ├── infrastructure/mod.rs
│   └── presentation/
│       └── cli/
│           ├── mod.rs
│           ├── command.rs  # clap の Command enum（全 feature の Args を合成）
│           └── dispatch.rs # match による全 feature へのディスパッチ
├── packages/
│   ├── core/
│   │   ├── sexpr/          # crate: paredit-core-sexpr（Cargo.toml + README.md + src/）
│   │   ├── dialect/
│   │   ├── semantics/
│   │   ├── lint-engine/
│   │   ├── edit/
│   │   ├── workspace/
│   │   └── cli/
│   └── feature/
│       ├── rename/         # crate: paredit-feature-rename
│       ├── function-parameter/
│       ├── ...
│       └── lint-conditional/
│           ├── Cargo.toml
│           ├── README.md
│           └── src/
│               ├── lib.rs
│               ├── if_not/          # スライス優先。層ディレクトリは作らない（§3.1）
│               │   ├── mod.rs domain.rs usecase.rs rule.rs
│               │   └── cli/{args,workflow,render,types}.rs
│               └── if_to_unless/
├── tests/                  # 変更なし（E2E は合成ルートに残る）
├── benches/                # 変更なし
└── docs/
```

### 3.1 パッケージ内部は「スライス優先」にする（層ディレクトリを作らない）

**パッケージの中に `domain/` `application/` `presentation/` ディレクトリを作ってはならない。**

これは意図的な判断であり、`RESEARCH.md` が想定していた「feature の中に層をネストする」案を
**却下する**ものである。理由は移行の目的そのものにある。

#### なぜ層ディレクトリを作らないのか

現状の最大の問題は「1 機能の変更が 3 層 4 ディレクトリに散る」ことだった（§1.1）。
パッケージの中で層優先にすると、**同じ問題がパッケージの中に再現する。**

```
# 層優先（却下）— lint ルール 1 個を直すのに 3 ディレクトリを開く
packages/feature/lint-conditional/src/
├── domain/{if_not.rs, if_to_unless.rs, constant_if_test.rs, ...}
├── application/{if_not.rs, if_to_unless.rs, ...}
└── presentation/{if_not.rs, if_to_unless.rs, ...}
```

```
# スライス優先（採用）— 1 ルール = 1 ディレクトリ
packages/feature/lint-conditional/src/
├── lib.rs            # #![doc = include_str!("../README.md")]
├── if_not/
│   ├── mod.rs
│   ├── domain.rs     # 旧 src/domain/if_not_report.rs
│   ├── usecase.rs    # 旧 src/application/usecase/if_not_report/
│   ├── rule.rs       # 旧 src/domain/lint/rules/if_not.rs
│   └── cli/{args.rs, workflow.rs, render.rs, types.rs}
├── if_to_unless/
└── constant_if_test/
```

**層は「ディレクトリ」ではなく「ファイル名」として残る。**
`domain.rs` / `usecase.rs` / `cli/` という命名が層を表現し、
`packages/*/*/` というパッケージ境界と `<slice>/` というスライス境界が
ディレクトリを担当する。1 スライスの中央値は 481 行（§2.2）なので、
この粒度なら 1 ディレクトリに収まる。

#### 依存規則はどう守るのか

`docs/src/architecture.md` の一方向依存則は**捨てない**が、
**ディレクトリ構造ではなく型と依存で守る**ように切り替える。

| 旧（ディレクトリで表現） | 新（機械的に検査可能） |
| --- | --- |
| `domain` は `presentation` を import しない | **`clap` は `cli` モジュール以外に現れてはならない**。契約テストで grep 検査する |
| `application` は `domain` のみ | **`application` 相当のコードは source port トレイトを自分で定義する**。§4 の Request→usecase→Plan パターンは維持 |
| `presentation` が全層を合成 | パッケージの `lib.rs` が公開する `Args` 型と `run` 関数がその役割（§4.2） |

`clap` の grep 検査が本質である。層規則が本当に守りたかったのは
「ドメインロジックが CLI の都合を知らない」という一点であり、
それは**依存クレートの位置で表現できる**。ディレクトリ階層より確実で、安い。

```rust
#[test]
fn domain_logic_never_depends_on_the_cli_argument_parser() {
    // packages/*/*/src/**/*.rs のうち、パスに /cli/ を含まないファイルが
    // `use clap` を含まないことを検査する。
}
```

#### 例外: 大きいパッケージ

`feature/rename`（18,895 行）や `feature/project-analysis`（12,776 行）のように
1 スライスが数千行に達するものは、**そのスライスの中で**さらに分割してよい。
`rename/` は既に 11 階層以上の内部構造を持っている（§5.2.1）。
その内部構造は現状のまま持ち込む。

**判断基準は「層で割るか」ではなく「読む人が探す単位で割るか」。**

### 3.1.1 ルート `src/` の層ディレクトリはどうするか

`packages/**` を層優先にしない（§3.1）なら、**ルートの `src/domain/` などは何なのか**。
答えは「**移行中は必須。移行後は意味が変わる**」である。

#### 移行中（Phase 1〜5）— 議論の余地なく必須

§4.1 の façade がこの 4 ディレクトリそのものである。
`src/domain/mod.rs` に `pub use paredit_core_sexpr::sexpr;` と書けるからこそ、
既存の `crate::domain::sexpr::X` **882 箇所を 1 行も変えずに**
1 パッケージずつ切り出せる。これを外すと移行が一括作業になり破綻する。

#### 移行後（Phase 6 以降）— 中身は再エクスポートだけになる

Phase 6 完了時点でルート `src/` に残るのは、実測ベースで次のものだけである。

| ファイル | 中身 | 性質 |
| --- | --- | --- |
| `lib.rs` | `pub mod` 4 行 + `pub use domain::{dialect, sexpr};` | 公開 API の入口 |
| `main.rs` | 1 行 | — |
| `domain/mod.rs` | `pub use paredit_core_*::*;` の羅列のみ | **エイリアス表** |
| `application/mod.rs` | 同上 | **エイリアス表** |
| `infrastructure/mod.rs` | 同上 | **エイリアス表** |
| `presentation/cli/{mod,command,dispatch}.rs` | clap の enum と `match`（約 1,760 行） | **合成ルート** |

つまり `domain/` `application/` `infrastructure/` は
**「アーキテクチャの層」ではなくなり、「公開 API の名前空間」になる。**
`presentation/` に至っては層ではなく合成ルートである。

#### では畳んでよいか — 公開 API の制約がある

一見すると `src/{domain,application,infrastructure}/` を消して
`lib.rs` に直接 `pub use paredit_core_sexpr as sexpr;` と書けばよさそうに見える。
実測すると **これは公開 API の破壊になる。**

- `CHANGELOG.md` の 1.1.0 エントリが **`paredit_cli::domain::semantics` を
  「Added」として告知している**。直近リリースで公開したパスを次のリリースで消すことになる
- `benches/` が `paredit_cli::domain::{sexpr,dialect,lint_report}` と
  `paredit_cli::application::usecase::similarity_report` を使っている
- `crate_metadata_contract.rs` が `src/lib.rs` に
  `pub use domain::dialect;` / `pub use domain::sexpr;` の literal を要求している
- `public_module_docs_contract.rs` が 4 層すべての `mod.rs` の doc コメントを検査している

> 一方 `tests/` は既にフラットなエイリアス（`paredit_cli::sexpr::` / `paredit_cli::dialect::`）を
> 使っており、README も「`src/lib.rs` に文書化された API」としか書いていない。
> **フラット API は既に存在し、機能している。**

#### 推奨: 残す。ただし「コードを置けない場所」に変える

| 判断 | 内容 |
| --- | --- |
| `src/domain/` `src/application/` `src/infrastructure/` | **残す。** 公開 API の名前空間として。ただし **`mod.rs` 1 ファイルのみ**とし、中身は `pub use` と doc コメントに限定する |
| `src/presentation/` | **残す**（`public_module_docs_contract.rs` のため）が、`docs/src/architecture.md` では「presentation 層」ではなく **「合成ルート」** と呼び直す |
| 畳んでフラット化する案 | **やらない。** 公開 API の破壊に見合う利益がない（§8-11 で再検討の余地は残す） |

重要なのは名前ではなく、**そこにコードが溜まらないことを機械で保証する**ことである。
Phase 6 の契約テストに追加する:

```rust
#[test]
fn root_layer_modules_stay_pure_reexport_facades() {
    // src/{domain,application,infrastructure} 直下に mod.rs 以外のファイルが無いこと。
    // mod.rs の非空行が `//!` / `pub use` / `pub mod` のみで構成されること。
}
```

このテストが無いと、**移行後に「とりあえず domain に置く」が復活する。**
層ディレクトリを残す判断のコストはこの 1 テストで払う。

| 種別 | ディレクトリ | crate name | lib name |
| --- | --- | --- | --- |
| core | `packages/core/sexpr` | `paredit-core-sexpr` | `paredit_core_sexpr` |
| feature | `packages/feature/rename` | `paredit-feature-rename` | `paredit_feature_rename` |

全メンバーに `publish = false` を設定する（[このリポジトリは crates.io に publish せず、
Git タグが唯一のリリース成果物](docs/src/releasing.md) であるため）。

### 3.3 全パッケージに README.md を必ず置く

**`packages/**/README.md` は必須とし、欠落を CI で検出する。**

パッケージ分割の目的は §1.1 のとおり「変更の局所性」と「境界の強制」である。
境界を宣言しただけで**その境界が何を意味するのかを書かなければ、
`Cargo.toml` の `[dependencies]` は単なる機械的な事実に留まる。**
README はその境界の**意図**を記録する場所であり、分割の価値を実現する要素そのものとして扱う。

#### 記載必須項目

| 見出し | 内容 |
| --- | --- |
| `# <crate-name>` | クレート名（`paredit-core-sexpr` 等） |
| 1 行要約 | `Cargo.toml` の `description` と一致させる |
| `## 責務` | このパッケージが引き受けるもの。**および引き受けないもの**（例: `core/lint-engine` は「ルールの実行機構。個別ルールは持たない」） |
| `## 依存` | `[dependencies]` の各エントリが**なぜ**必要かを 1 行ずつ。依存が増えたらここも増える |
| `## 公開している型・関数` | 他パッケージが使う入口。feature なら `Args` 型と `run` 関数（§4.2） |
| `## 変更が必要になる典型的なケース` | 「この修正はここに来る」の道案内。`docs/src/architecture.md` の「変更がどこに属するか」表の粒度を細かくしたもの |

`## 責務` に**引き受けないもの**を書くことを特に重視する。
core と feature の境界侵食（core が feature の知識を持ち始める劣化）は、
この 1 行があるかどうかで気づきやすさが変わる。

#### `Cargo.toml` との接続

```toml
[package]
name = "paredit-core-sexpr"
description = "Typed S-expression parsing, tree navigation, spans, and balanced edits"
readme = "README.md"
publish = false
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
repository.workspace = true
```

`src/lib.rs` の先頭は、ルートクレートと同じ規約に揃える。

```rust
#![doc = include_str!("../README.md")]
```

これにより README が**放置されると rustdoc がおかしくなる**位置に置かれ、
腐敗が検知されやすくなる。

> **crane 側の注意**: `craneLib.fileset.commonCargoSources` は `*.md` を拾わない。
> `include_str!` がコンパイル時に README を要求するため、
> **§2.5.2 の fileset に `packages` 配下の README.md を明示的に追加しなければビルドが落ちる。**

#### 強制方法

Phase 6 で契約テストを追加する（§6 Phase 6-4）。

```rust
#[test]
fn every_workspace_package_documents_itself() {
    // packages/*/*/Cargo.toml を走査し、同じディレクトリに README.md があること、
    // README.md の見出しが Cargo.toml の name と一致すること、
    // Cargo.toml に readme = "README.md" があることを検査する。
}
```

ルートの `crate_metadata_contract.rs` が `paredit-cli` 本体に対して行っている検査
（`readme = "README.md"` の存在、`lib.rs` の `include_str!` 規約）を
**全メンバーに一般化したもの**と位置づける。

---

## 4. 移行を成立させる 2 つの仕組み

この 2 つを最初に用意することが、本移行の成否を分ける。

### 4.1 ルート façade による無停止移行

**ルートパッケージ `paredit-cli` を残し、旧パスを再エクスポートし続ける。**

`src/domain/mod.rs`:

```rust
//! Core Lisp parsing, dialect, and semantic refactoring rules that stay
//! independent from CLI delivery and filesystem adapters.
//                     ^^^ public_module_docs_contract.rs のためこの doc コメントは維持

pub use paredit_core_sexpr as sexpr;
pub use paredit_core_dialect as dialect;
pub use paredit_feature_rename::domain as rename;
// 未移送のモジュールは従来どおり
pub mod similarity_report;
```

これにより:

- `crate::domain::sexpr::SyntaxTree` という **既存の 882 箇所の import が一切変わらない**
- 公開ライブラリ API（`paredit_cli::domain::sexpr`）が維持され、`benches/*.rs` も無改修
- §2.6 の契約テスト群が全て無改修で通る
- **1 パッケージずつ切り出しても、常にツリー全体がコンパイル可能**

façade は移行完了後も残す（公開 API の安定面として）。

### 4.2 合成ルートの分離

`presentation/cli/command.rs`（690 行の clap enum、全 feature を `use super::{...}` で列挙）と
`dispatch.rs`（759 行の `match`）は、**定義上あらゆる feature に依存する**。
これらは分割せず、ルートパッケージに置く。

feature クレート側の契約は次の 2 点のみ:

```rust
// packages/feature/project-analysis/src/call_report/mod.rs
pub use args::CallReportArgs;             // clap::Args を derive した引数型
pub use workflow::call_report;            // fn(CallReportArgs) -> anyhow::Result<()>
```

ルートの `command.rs` はこれを列挙する:

```rust
use paredit_feature_call_report::presentation::CallReportArgs;

pub enum InspectCommand {
    Calls(CallReportArgs),
    // ...
}
```

lint の `REGISTRY`（`const` 配列）も同じ扱いで、**ルート側に移す**。
`RuleEntry::new(&paredit_feature_lint_conditional::rules::if_not::META, ...)` の形で
各 feature クレートの `const` を参照する。const 配列はクレートを跨いでも
コンパイル時に評価されるため、`RULE_COUNT` の `const` アサーションも維持される。

> **重要**: `REGISTRY` を `core/lint-engine` に置いてはならない。engine が全ルールに依存し、
> ルールが engine に依存する循環になる。engine（トレイトと 1 パス実行）と
> registry（全ルールの列挙）は必ず別クレートに分ける。

---

## 5. パッケージ分割案

### 5.1 core パッケージ（7 個 / 約 41,400 行）

依存の向きは上から下への一方向。

| # | パッケージ | 実測行数 | 中身 | 依存先 |
| --- | --- | ---: | --- | --- |
| C1 | `core/sexpr` | 7,798 | `domain::{sexpr, leading_trivia, expression_equality, form_shape, graph, view_query}` | なし（葉） |
| C2 | `core/dialect` | 4,684 | `domain::{dialect, common_lisp}` | C1 |
| C3 | `core/semantics` | 13,747 | `domain::{semantics, lexical_scope, binding_index, callable_scope, definition, definition_reference}` | C1, C2 |
| C4 | `core/edit` | 4,266 | `domain::{mutation_safety, refactor_plan, refactor_preview, refactor_execute, extract_shared, let_binding, progn, local_function_binding, let_composition, let_star_composition, flet_composition, convert_control}` | C1, C2, C3 |
| C5 | `core/lint-engine` | 2,472 | `domain::lint::{rule, model, policy, engine}`, `domain::{lint_report, lint_suppression, report_policy}` — **registry と rules は含まない** | C1, C2, C3 |
| C6 | `core/workspace` | 1,631 | `infrastructure::{workspace, fs_identity}` | C2 |
| C7 | `core/cli` | 6,839 | `presentation::cli::{args, io, diff, shared, gate, contract, macos_acl}` | C1〜C6 |

`core/sexpr` が全依存の起点（882 箇所から参照）。`core/cli` の実体は
`io.rs` 4,782 行 + `diff.rs` 797 行で、CLI の I/O 規約そのもの。

> **【実装時の訂正 — 上表の「依存先」列は誤り】**
>
> Phase 1 の着手時に、上表の core モジュール全件について
> `crate::domain::<module>` 参照（コメント除去済み）を抽出し Tarjan で
> 強連結成分を求めたところ、**C1 は葉ではなかった**。
>
> ```
> sexpr       -> dialect       8   (parser / tree / edit / reader_policy が Dialect)
> sexpr       -> common_lisp   4   (CommonLispOperator と各種 eq ヘルパ)
> common_lisp -> sexpr        13   (ByteSpan / ExpressionView / SyntaxTree)
> dialect     -> sexpr         2
> common_lisp -> definition    4   (DefinitionCategory)
> dialect     -> definition    1
> definition  -> sexpr / dialect / common_lisp
> ```
>
> すべて本番コードであり、doc リンクではない。結果:
>
> | SCC | 行数 | 判定 |
> | --- | ---: | --- |
> | `{sexpr, dialect, common_lisp, definition}` | 12,936 | **1 パッケージにするしかない** |
> | 他の core モジュール全て | — | 単独ノード（非循環） |
>
> **C1 → C2 → C3 という依存順は Cargo で表現できない。**
> §1.2 の「相互循環は 1 組のみ」は feature スライスに対する実測であり、
> core は対象外だった。
>
> したがって Phase 1 で切り出したのは上表の C1 ではなく:
>
> | 実装 | 内容 | 行数 |
> | --- | --- | ---: |
> | `packages/core/syntax`<br>(`paredit-core-syntax`) | 上記 SCC + `leading_trivia`, `expression_equality`, `form_shape`, `graph`, `view_query` | **13,603**（62 ファイル） |
>
> 外向きコード依存ゼロ・`application`/`presentation`/`infrastructure` 参照ゼロを
> 実測で確認済み。`sexpr` ではなく `syntax` と命名したのは dialect と
> Common Lisp 知識を持つため。
>
> **上表への影響**: C1 と C2 は消滅し `core/syntax` に統合。C3 は
> `definition` を失い `semantics, lexical_scope, binding_index,
> callable_scope, definition_reference` になる。
> **C4・C5・C6・C7 は上表のまま有効**（実測でいずれも非循環）。
> core パッケージ数は 7 → 6。
>
> なお `benches/` だけでなく **`examples/semantic_coverage.rs` も façade 経由の
> 利用者**である（§11.3 の「参照ゼロ」も参照）。

### 5.2 feature パッケージ（約 153,900 行）

#### 5.2.1 確度の高い分割（実測の連結成分に基づく）

| # | パッケージ | 実測行数 | 主な中身 |
| --- | --- | ---: | --- |
| F1 | `feature/rename` | 18,895 | `rename`, `rename_control`, `rename_types` |
| F2 | `feature/project-analysis` | 12,776 | 最大連結成分。`call_report`, `call_graph_report`, `*_cycle_report`, `dependency_report`, `impact_report`, `signature_report`, `workspace_report`, `definition_report`, `complexity_report`, `naming_report`, `form_report` ほか |
| F3 | `feature/function-parameter` | 10,121 | `function_parameter` + `*_parameter_report`, `lambda_list_keyword_*` |
| F4 | `feature/package` | 8,392 | `package` + `package_*_report`, `system_*_report`, `unused_{package,nickname,export}_report` |
| F5 | `feature/binding` | 8,244 | `let_report`, `introduce_let`, `split_let*`, `merge_nested_let*`, `convert_let*`, `shadowed_binding_report` ほか |
| F6 | `feature/similarity` | 7,991 | `similarity_report`, `duplicate_report` |
| F7 | `feature/inline` | 7,142 | `inline_{function,let,lambda,local_function,literal_constant,symbol_macro}` |
| F8 | `feature/extract` | 6,052 | `extract_{function,local_function,constant}` |
| F9 | `feature/remove-unused` | 6,020 | `remove_unused_{binding,control,definition}`, `definition_{removal,movement}` |
| F10 | `feature/form-transform` | 5,017 | `thread_expression`, `unthread_expression`, `replace_forms`, `unwrap_call`, `sort_definitions`, `split_file`, `convert_{if,cond,when,unless,flet,labels}_to_*` |
| F11 | `feature/lint-report` | 1,955 | `lint_report`（CLI ワークフローと `REGISTRY` 消費側） |
| F12 | `feature/refactor-workflow` | 2,184+ | `application::refactor` + `presentation::cli::refactor` |

**注**: F2 に `call_cycle_report ↔ package_cycle_report` の唯一の相互循環が含まれるが、
`package_cycle_report` を F2 に置くことで同一クレート内に収まり解消する
（F4 は `package_cycle_report` を依存として参照する形になる）。

#### 5.2.2 lint ルール群（126 スライス / 61,289 行）— 判断を要する部分

残り 126 スライスは **極めて均質**である（1 スライス 347〜695 行、平均 486 行）。
それぞれ「`*_report` ドメインモジュール + usecase + CLI + `lint/rules/*.rs` アダプタ」の
定型 4 点セットで、他 feature への依存をほぼ持たない。

`RuleCategory` は分割軸として使えない（100/126 が `Suspicious` に偏る）。
**対象とする Lisp 構文の主題**で分けるのが妥当:

| 提案パッケージ | 概算スライス数 | 主題 |
| --- | ---: | --- |
| `feature/lint-conditional` | 約 25 | `if` / `when` / `unless` / `cond` / `case` / `typecase` |
| `feature/lint-sequence` | 約 25 | `car` / `cdr` / `append` / `nthcdr` / `subseq` / `list*` / `cons` / `reverse` |
| `feature/lint-numeric` | 約 20 | `eq` / `eql` / `=` / 除算 / 算術恒等 / 符号比較 / ステップ |
| `feature/lint-control-flow` | 約 18 | `progn` / `prog1` / `unwind-protect` / `handler-case` / `eval-when` / 戻り値 |
| `feature/lint-form-shape` | 約 25 | `setf` / `setq` / `the` / `typep` / スロット / キーワード / arity / `quote` / lambda |
| `feature/lint-string-char` | 約 13 | 文字・文字列・`format` |

**これは本仕様書で唯一、実測ではなく判断に基づく部分である。** §6 の Phase 5 で実施する際、
`head_filter` が対象とする head シンボルの実測クラスタリングで再検証すること。
均質性が高いため、**最悪の場合 `feature/lint-rules` 1 パッケージにまとめても機能する**
（61k 行は F1〜F12 の合計より小さく、単体で許容範囲）。

### 5.3 パッケージ総数

core 7 + feature 12 + lint 6 = **25 パッケージ + ルート 1 = 26**。

> 206 スライスを 1 スライス 1 クレートにすると 206 クレートになるが、これは採らない。
> クレートあたりのメタデータ・リンク時オーバーヘッドが実処理時間を上回り、
> `Cargo.toml` の保守コストが利益を消す。**中央値 481 行のスライスは
> クレートの粒度としては小さすぎる。**

---

## 6. 移行手順

各 Phase は独立した PR とし、**Phase 終了時点で必ず `nix flake check` が通る**こと。

**全体像**（§10 で追加するフェーズを含む）:

| Phase | 内容 | 依存 | 中止しても成果が残るか |
| --- | --- | --- | --- |
| **−1** | 機械的 lint 修正（§10） | なし | **残る** |
| 0 | workspace スケルトン | −1 | — |
| 1 | `core/sexpr`（パイロット） | 0 | — |
| 2 | 残り core（C2〜C7） | 1 | — |
| **2.5** | crane 移行（§6 Phase 2.5） | 2 | **残る**（`flake.nix` のみ） |
| 3 | パイロット feature（F6） | 2 | — |
| 4 | 大型 feature（F1〜F12） | 3 | — |
| 5 | lint ルール群 | 4 | — |
| 6 | 仕上げ（docs / 契約テスト） | 5 | — |
| **7** | 型設計の強化（§10） | 6 | **残る** |

`anyhow` → `thiserror`（§9.2）は独立フェーズを持たず、
**Phase 3〜5 の各移送 PR の中で同時に行う**（§9.5）。

移行前の準備は §11 にまとめた。**§11.1 のスナップショット取得は Phase −1 より前に行う。**

### Phase 0: 準備（コード変更なし）

1. **`docs/src/architecture.md` に移行方針を追記**（層構造の廃止ではなく feature 内へのネストであること、
   合成ルートの位置づけ）。移行後に全面改稿するが、Phase 0 時点で方向性を文書化しておく。
2. **workspace スケルトンの作成**: ルート `Cargo.toml` に `[workspace] members = ["packages/*/*"]`
   と `[workspace.package]` / `[workspace.dependencies]` を追加。この時点でメンバーはゼロ。
   - `[package]` セクションの `name` / `publish` / `license` / `rust-version` 等は
     **`crate_metadata_contract.rs` のためルートに文字列としてそのまま残す**
     （`workspace = true` への集約はしない）。
3. **CI の確認**: `nix flake check` が workspace 化後も通ることを空メンバーで検証。

**完了条件**: `cargo metadata` が workspace を認識し、`nix flake check` が緑。

### Phase 1: `core/sexpr` の切り出し（パイロット）

最も参照が多く（882 箇所）、最も依存が少ない（葉）モジュールを最初にやる。
**ここで移行手順のすべてが検証される。**

1. `packages/core/sexpr/` を作成し、`src/domain/{sexpr,leading_trivia,expression_equality,form_shape,graph,view_query}` を `git mv` で移送。
2. `packages/core/sexpr/src/lib.rs` に `#![doc = include_str!("../README.md")]` と
   `pub mod sexpr; pub mod leading_trivia; ...` を記述。
3. **`packages/core/sexpr/README.md` を §3.3 の必須項目に沿って書く。**
   後回しにしない。移送作業の直後、記憶が新しいうちに書くのが最も安く、最も正確になる。
4. 移送したファイル内の `crate::domain::sexpr::X` → `crate::sexpr::X` へ一括置換。
5. **可視性の修正**: `pub(crate)` / `pub(in crate::domain::...)` をコンパイラの指示に従って修正。
   `cargo check -p paredit-core-sexpr` を繰り返す。
   ここで `pub` 化した項目は README の「公開している型・関数」に反映する。
6. `src/domain/mod.rs` に façade を記述:
   ```rust
   pub use paredit_core_sexpr::sexpr;
   pub use paredit_core_sexpr::leading_trivia;
   ```
7. ルート `Cargo.toml` に `paredit-core-sexpr = { path = "packages/core/sexpr" }` を追加。
8. **`git add -N packages/core/sexpr`** を実行（`nix flake check` は git 管理下のファイルしか見ないため、
   これを忘れると MSRV チェックが `E0583: file not found for module` で落ちる。
   **README.md も追跡対象に入れないと `include_str!` が Nix サンドボックス内で解決できない**）。

**完了条件**:
- `cargo check --all-targets` / `cargo nextest run --locked` / `nix flake check` が緑
- **`packages/core/sexpr/README.md` が存在し、§3.3 の必須 6 項目をすべて含む**
- `src/**` 配下の既存 `use crate::domain::sexpr::...` が **1 行も変更されていない**
  （`git diff --no-ext-diff --stat` で確認。difftastic が外部 diff ドライバのため
  `--no-ext-diff` が必須）
- `benches/*.rs` 無改修で動作

**Phase 1 が想定より大幅に難航した場合、移行全体を中止する判断をここで下す。**

### Phase 2: 残りの core パッケージ（C2〜C7）

Phase 1 と同一手順を C2 → C3 → C4 → C5 → C6 → C7 の順（依存の浅い順）で繰り返す。

C5（`core/lint-engine`）で注意すべき点:
- `domain::lint::{registry, rules}` は **移送しない**。`src/domain/lint/` に残す。
- `domain::lint::mod.rs` を分割し、engine 側だけをクレート化する。

C7（`core/cli`）で注意すべき点:
- **`pub(in crate::presentation::cli)` 963 箇所がここで一斉に効いてくる。**
  移送対象ファイル内のものは `pub` へ、残留側から参照されるものも `pub` へ。
- `io.rs` 4,782 行は単一ファイルなので、この機会にディレクトリへ分割してもよい（任意）。

**完了条件**: Phase 1 と同じ（**各パッケージの README.md を含む**）。加えて
`src/domain/mod.rs` の `pub mod` 宣言が core 移送分だけ `pub use` に置き換わっていること。

C5 の README には「`registry` と `rules` を**持たない**理由」を必ず書くこと（§4.2 の循環回避）。
これは §3.3 が `## 責務` に「引き受けないもの」を要求している典型例である。

### Phase 2.5: crane への移行（コード変更なし / `flake.nix` のみ）

**この Phase はクレート構造に触れない。** `flake.nix` のビルド手段だけを差し替える独立した
PR とし、失敗しても `git revert` 1 回で完全に切り戻せる状態を保つ。
Phase 2 の直後に置く理由は、core パッケージが出揃った時点で workspace のメンバー構成が
安定し、fileset の設計が確定するため。

#### 2.5.1 flake input の追加

crane は 0.19.0 で最上位 `lib` 属性を廃止し、`mkLib pkgs` に一本化された。
**`inputs.nixpkgs.follows` は設定しない**（crane 自身が nixpkgs input を持たないため）。

```nix
inputs = {
  nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  rust-overlay = { url = "github:oxalica/rust-overlay"; inputs.nixpkgs.follows = "nixpkgs"; };
  treefmt-nix  = { url = "github:numtide/treefmt-nix";  inputs.nixpkgs.follows = "nixpkgs"; };
  crane.url = "github:ipetkov/crane";
};
```

追加する input は `crane` 1 つだけ。**`advisory-db` は追加しない**
（`cargoAudit` への移行は §8-6 のとおり見送るため）。

`flake.nix` 冒頭の `cargoToml = builtins.fromTOML (builtins.readFile ./Cargo.toml);` と
`msrvToolchainVersion` の導出はそのまま使える。crane にも
`craneLib.crateNameFromCargoToml` があるが、既存コードを置き換える理由はない。

#### 2.5.2 最重要: `cleanCargoSource` を使ってはならない

crane の標準例は `src = craneLib.cleanCargoSource ./.;` だが、
**本リポジトリでこれを使うとビルドとテストの両方が壊れる。**理由は独立に 2 つある。

1. **コンパイル時**: `src/lib.rs:1` が `#![doc = include_str!("../README.md")]`。
   `cleanCargoSource` は `*.rs` / `*.toml` / `Cargo.*` 以外を除去するため、README.md が消えて
   コンパイルエラーになる（crane 公式 FAQ「Building with non-Rust includes」の該当ケース）。
2. **テスト時**: `tests/` 配下がリポジトリ内の非 Rust ファイルを実行時に読む。
   全数を実測で洗い出した結果は次のとおり（`grep` による網羅調査済み）。

| 読まれるパス | 読み手 |
| --- | --- |
| `README.md` | `readme_*_contract.rs`, `lib.rs` の `include_str!` |
| `CHANGELOG.md` | リリース関連 contract |
| `Cargo.toml` | `crate_metadata_contract.rs` |
| **`flake.nix`** | contract（インストール手順との整合検査） |
| `action.yml` | `action_contract.rs` |
| `.github/actions/nix-setup/action.yml` | contract |
| `.github/workflows/{ci,docs,flake-update,release}.yml` | contract |
| `docs/src/{agents,commands,integrations,releases,releasing}.md` | contract |
| **`skills/paredit-cli/SKILL.md`** | contract |
| `src/lib.rs`, `src/domain/dialect/mod.rs` | contract（ソースを文字列として検査） |
| `tests/fixtures/**`（`*.lisp` / `*.golden` / `*.proptest-regressions`） | 各テスト |
| `benches/**` が生成する `bench.lisp` 等 | criterion（実行時生成。fileset 不要） |

> **`flake.nix` 自身がビルド入力に含まれる**点に注意。`flake.nix` を編集すると
> `cargoArtifacts` 以外の全デリベーションのハッシュが変わる。crane 化しても
> この性質は消えない（消すには contract test 側の設計変更が必要で、本移行のスコープ外）。

したがって `src` は `lib.fileset` で手組みするが、**除外できるものがほとんど残らない**。
これは現行 `flake.nix` の `src = ./.` に添えられたコメントが述べている設計意図そのもので、
**crane 化しても「リポジトリ全体が 1 ビルド入力」という性質は変えない。**

```nix
# 全体ビルド・全体テスト用のソース集合。
#
# craneLib.cleanCargoSource は使えない。理由は 2 つあり、どちらも単独で致命的:
#   1. src/lib.rs が #![doc = include_str!("../README.md")] で README を埋め込む
#   2. tests/cli/*_contract.rs が README / CHANGELOG / flake.nix / action.yml /
#      .github/**/*.yml / docs/src/*.md / skills/**/SKILL.md を実行時に読む
# 実質リポジトリ全体が入力なので、fileset は「除外」ではなく「明示」として書く。
workspaceSrc = lib.fileset.toSource {
  root = ./.;
  fileset = lib.fileset.unions [
    (craneLib.fileset.commonCargoSources ./.)   # Cargo.toml/lock + *.rs + *.toml
    ./README.md
    ./CHANGELOG.md
    ./flake.nix
    ./action.yml
    ./docs/src
    ./skills
    ./tests/fixtures
    ./.github/workflows
    ./.github/actions
    # 各パッケージの README.md（§3.3）。lib.rs が include_str! で読むため必須。
    # commonCargoSources は *.md を拾わないので明示する。
    (lib.fileset.fileFilter (f: f.name == "README.md") ./packages)
  ];
};
```

> 最後の 1 行を忘れると、**Phase 3 以降で最初のパッケージを切り出した瞬間に
> `include_str!` が解決できずビルドが落ちる**。Phase 2.5 の時点では
> `packages/` が空なので気づけない。先に書いておくこと。

> 取りこぼしがあると **テストは「ファイルがない」ではなく「期待値と違う」形で落ちる**
> （`assert!(contents.contains(...))` の失敗）。原因追跡が難しいため、
> Phase 2.5 の完了条件で**テスト実行件数の一致**を必ず確認すること。

#### 2.5.3 checks の再構成

```nix
craneLib = (crane.mkLib pkgs).overrideToolchain (p: p.rust-bin.stable.latest.default);

commonArgs = {
  src = workspaceSrc;
  strictDeps = true;
  pname = "paredit-cli";
  version = cargoToml.package.version;
};

# 157 個の依存クレートを 1 回だけビルドし、以下すべてで共有する
cargoArtifacts = craneLib.buildDepsOnly commonArgs;
```

| 現行 | crane 移行後 |
| --- | --- |
| `packages.default = rustPlatform.buildRustPackage { src = ./.; }` | `craneLib.buildPackage (commonArgs // { inherit cargoArtifacts; })` |
| `checks.clippy = package.overrideAttrs { buildPhase = "cargo clippy"; }` | `craneLib.cargoClippy (commonArgs // { inherit cargoArtifacts; cargoClippyExtraArgs = "--all-targets -- --deny warnings"; })` |
| `checks.nextest = package.overrideAttrs { buildPhase = "cargo nextest run"; }` | `craneLib.cargoNextest (commonArgs // { inherit cargoArtifacts; })` |
| `checks.msrv = mkPareditWithPlatform pkgs msrvRustPlatform` | 別 `craneLibMsrv = (crane.mkLib pkgs).overrideToolchain (p: p.rust-bin.stable."1.85.0".default)` を作り、**独自の `cargoArtifacts`** を持たせる |
| CI job `supply-chain: nix develop --command cargo audit` | **原則そのまま残す**（§2.5.4 の契約テストが literal で検査している）。crane 化する場合は契約テストの更新を同一 PR で行う |
| （なし） | `checks.doc = craneLib.cargoDoc (commonArgs // { inherit cargoArtifacts; env.RUSTDOCFLAGS = "--deny warnings"; })` を追加してもよい（任意 / §8-7） |

#### 2.5.4 `flake.nix` の**テキスト**を拘束している契約テスト

`tests/cli/action_contract.rs` は `flake.nix` と `ci.yml` を**文字列として**検査する。
crane 化はこれらを踏みやすい。実装前に必ず全部を確認すること。

| テスト | 要求している literal | crane 化での影響 |
| --- | --- | --- |
| `flake_exposes_the_documented_integration_surfaces` | `paredit-lint` / `paredit-format` / `paredit-format-files` / `overlays.default` / `mkLintCheck` / `mkFormatCheck` / `treefmtFormatter` / `treefmt-nix` / `lint-format-integration` / **`pkgs.cargo-audit`** / `excludes = [ "tests/fixtures/*" ]` | **`pkgs.cargo-audit` を devShell から外すと落ちる。** これらのブロックは crane 化の対象外なので触らない |
| `flake_checks_expose_the_org_conformance_aliases` | マーカー `checks = lib.genAttrs systems (` と `overlays.default = final: _prev: {` の**間に** `default = nextest;` / `formatting = treefmt;` / `docs = documentation;` | **`checks` ブロックの開始行と、その直後に `overlays.default` が来る構造を維持すること。** check の名前も `nextest` / `treefmt` / `documentation` のまま変えない |
| `flake_lisp_includes_cover_exactly_the_recognized_dialect_extensions` | `lispIncludes = [` から `]` までを構文解析し、`Dialect::from_extension` と突き合わせる | 触らなければ影響なし |
| `ci_runs_the_flake_checks_and_audits_dependencies` | `ci.yml` に `supply-chain:` と **`nix develop --command cargo audit --deny warnings`** | **`cargoAudit` に移行して CI job を削除すると落ちる。** テストの doc コメントは「ゲートの存在」を守る意図と述べているので、移行するなら**契約テストの期待値更新を同一 PR に含める**のが正しい対応 |

**変更してはならないもの**（上記の裏返し）:
- `checks.{default,formatting,docs}` のエイリアスと、参照先の `nextest` / `treefmt` / `documentation` という check 名
- `checks` ブロックの開始・終了マーカーとなるテキスト構造
- `treefmt` / `actionlint` / `documentation` / `lint-format-integration` の各 check
- `packages.{docs,lint,format,format-files}` / `overlays.default` / `lib.{treefmtFormatter,mkLintCheck,mkFormatCheck}`
  （外部リポジトリから利用される公開インタフェース）
- `lispIncludes` / `mkTreefmtModule` / devShell の `pkgs.cargo-audit`

#### 2.5.5 キャッシュは「メンバー単位」ではなく「レイヤー単位」で切る

crane には `cargoArtifacts` を**デリベーション間で連鎖させる**機能があり、
前段の `target` ディレクトリを次段が継承する（`inheritCargoArtifactsHook`）。
これを使えば、feature の変更が core のビルドキャッシュを壊さない構成が実際に作れる。
**やる価値はある。** ただし切り方を間違えると逆効果になる。

##### なぜ「メンバー単位 26 個」ではダメか

crane 公式 workspace 例の `fileSetForCrate` は、
**「利用者が必要なバイナリだけをビルドできるようにする」ための仕組み**であって、
ワークスペース全体のインクリメンタルキャッシュのための仕組みではない。
各メンバーのデリベーションは互いに独立しており、
`nix flake check` で全メンバーをビルドすると
**共通の core クレートが各メンバーのデリベーション内で重複コンパイルされる。**

25 個の feature がそれぞれ core 41,437 行をコンパイルし直すと、総 CPU 時間は激増する。

##### 何を切るべきか — 依存 DAG が答えを決める

本リポジトリの依存グラフは**きれいな層状**である（§5）。

```
deps (157 crates)
   ↓
core (7 crates / 41,437 行)          ← 全 feature が依存する
   ↓
feature (25 crates / 153,894 行)     ← 互いにほぼ独立、依存辺 89 本のみ（§2.2）
   ↓
paredit-cli（バイナリ + E2E テスト）  ← 全 feature を合成
```

**feature は誰からも依存されない葉である**（依存するのはルートだけ）。
これは §2.2 の実測（206 スライス中 146 が feature 間依存ゼロ、
連結成分 140 個）が示している構造そのものであり、
**層で切ったときにキャッシュが最も効く形**をしている。

```nix
# 4 層に切る。26 個には切らない。
depsArtifacts    = craneLib.buildDepsOnly { src = workspaceSrc; };

coreArtifacts    = craneLib.buildDepsOnly {              # core だけをビルド
  src = filesetFor [ ./packages/core ];
  cargoArtifacts = depsArtifacts;
  cargoExtraArgs = "-p paredit-core-sexpr -p paredit-core-dialect ...";
};

featureArtifacts = craneLib.buildDepsOnly {
  src = filesetFor [ ./packages/core ./packages/feature ];
  cargoArtifacts = coreArtifacts;
};

paredit = craneLib.buildPackage (commonArgs // { cargoArtifacts = featureArtifacts; });
```

##### 期待できる効果

| 変更箇所 | 現状（単一 src） | レイヤー化後 |
| --- | --- | --- |
| lint ルール 1 個 | 自コード 209,507 行を全ビルド | core 41,437 行が**キャッシュヒット**。残りをビルド |
| feature 1 個 | 同上 | 同上 |
| `core/sexpr` | 同上 | 同上（全依存が無効化されるので変わらない） |

自コードのビルドは release で約 196 秒（§2.3 の 239.8 − 43.8）。
core が 41,437 / 209,507 ＝ **約 20%** なので、
feature だけを触る変更では **約 39 秒**が追加で節約される。
crane の `buildDepsOnly` 単独の 88 秒（§1.3）と合わせると **約 127 秒**。

**このリポジトリは CI で Cachix を使っている**ため、
「前回と core が同一」という状況が常態である。レイヤー化が効く前提が揃っている。

##### トレードオフ（正直に）

| 論点 | 内容 |
| --- | --- |
| **直列化** | 連鎖は厳密なビルド順を強制する。`deps → core → feature → bin` が直列になり、**cold ビルドの wall-clock は悪化する**。得をするのは warm（Cachix ヒット）のとき |
| ストレージ | `installCargoArtifactsMode` を既定の `use-zstd` にすると連鎖段数に対して二次的に増える。**`use-symlink` を指定**して Nix ストアの重複排除を効かせること |
| E2E テスト | `tests/` 50,492 行は合成バイナリに依存するため、**この層は必ず再ビルドされる**。レイヤー化で縮むのはその手前まで |
| feature unification | crane が `cargo-hakari` を推奨する理由。ただし本リポジトリは実行時依存 8 個（anyhow / blake3 / cap-std / clap / clap_complete / libc / serde_json / thiserror）で feature の分岐余地が小さい。**hakari なしで先に実測してよい** |

##### 段階

1. **Phase 2.5 では `buildDepsOnly` 1 段のみ**を入れる（§2.5.3）。
   この時点では `packages/` が空なので、レイヤー化しても意味がない
2. **Phase 2 完了後**（core が出揃った時点）に `coreArtifacts` 層を追加する
3. **Phase 4 完了後**に `featureArtifacts` 層を追加する
4. 各段階で **cold と warm の両方を計測**し、
   warm の改善が cold の悪化を上回らなければその層を戻す

この 2〜4 は Phase 2.5 とは別の PR にする。**測ってから入れる。**

#### 2.5.6 `workspace.members` はワイルドカードで宣言する

crane の workspace 例が明示的に警告している事項:

> Note that the cargo workspace must define `workspace.members` using wildcards,
> otherwise, omitting a crate will result in errors since cargo won't be able to
> find the sources for all members.

Phase 0 で決めた `members = ["packages/*/*"]` はこの制約を満たす。
`members` を列挙形式に変えてはならない。

**完了条件**:
- `nix flake check` が緑
- **`cargo nextest run` の実行テスト件数が移行前と一致**（fileset の取りこぼし検出）。
  件数が減っていたら §2.5.2 のリストに漏れがある
- `tests/cli/action_contract.rs` の 4 テストが**無改修で**通る（§2.5.4）。
  通らない場合、`flake.nix` のテキスト構造を戻すか、契約テスト更新の是非を判断する
- `nix build .#` の成果物で `paredit --version` / `paredit inspect capabilities` の出力が移行前と一致
- `nix flake show` の出力が移行前と一致（`packages` / `checks` / `apps` / `overlays` / `lib` の属性名）
- `nix flake check` の総 wall time を移行前後で計測し記録する。
  **9% 前後の改善が見えない、あるいは悪化していれば revert する**（§1.3 の見積もり検証）

**切り戻し条件**: 完了条件のいずれかを満たせない場合、Phase 2.5 の PR を revert し、
Phase 3 以降を `buildRustPackage` のまま進める。crane は本移行の必須要素ではない。

### Phase 3: パイロット feature の切り出し（F6 `feature/similarity`）

feature 側の手順を検証する。`similarity_report` を選ぶ理由:
- 他 feature への依存ゼロ、被依存も少ない
- 3 層すべてに実体を持つ（domain 5,374 / application 834 / presentation 427 行）
- `SimilarityReportSourcePort` を持ち、**ポートを跨ぐパターンの検証になる**
- `benches/similarity_report.rs` があり、公開 API 経由の利用が壊れないか検証できる

追加手順（core にはなかったもの）:

8. `packages/feature/similarity/src/lib.rs` から `Args` 型と `run` 関数を `pub` 公開
   （スライス内の `cli/args.rs` / `cli/workflow.rs` を re-export する / §3.1）。
9. ルート `presentation/cli/command.rs` / `dispatch.rs` の `use super::similarity_report` を
   `use paredit_feature_similarity::presentation as similarity_report` に変更。
10. `src/application/usecase/mod.rs` と `src/presentation/cli/mod.rs` に façade 再エクスポートを追加。
11. README の `## 公開している型・関数` に、手順 8 で公開した `Args` 型と `run` 関数を明記する。
    **feature パッケージの README におけるこの節は、ルート `command.rs` / `dispatch.rs` との
    契約そのもの**であり、他のどの項目より優先度が高い。

**完了条件**: Phase 1 と同じ（**README.md を含む**）。加えて
`cargo test -p paredit-feature-similarity` がそのクレートのユニットテストのみを実行して緑。

### Phase 4: 大型 feature の切り出し（F1〜F5, F7〜F12）

Phase 3 の手順を反復。**行数の大きい順ではなく、依存の少ない順に進める。**

推奨順序:
```
F6 similarity(済) → F8 extract → F7 inline → F9 remove-unused → F10 form-transform
→ F5 binding → F3 function-parameter → F1 rename → F4 package → F2 project-analysis
→ F12 refactor-workflow → F11 lint-report
```

`F2 project-analysis` を最後の方に置く理由: 25 スライスの連結成分であり、
唯一の相互循環を含み、他 feature から最も多く参照される（`dependency_report` は 6 件、
`definition_report` は 5 件の被依存）。他が片付いてから境界を確定させたほうが安全。

`F11 lint-report` を最後にする理由: `REGISTRY` の移送（§4.2）を伴い、
Phase 5 の前提となるため。

**完了条件**: 各 feature ごとに Phase 3 と同じ条件。

### Phase 5: lint ルール群の分割（§5.2.2）

1. **分割軸の再検証**: 各 `lint/rules/*.rs` の `head_filter` が返す head シンボルを実測抽出し、
   クラスタリングして §5.2.2 の 6 分割案と突き合わせる。乖離があれば分割案を修正する。
2. 126 スライスを 6 パッケージへ移送。1 パッケージずつ PR を分ける。
3. `REGISTRY`（ルート側、§4.2）の各エントリの参照先を移送先クレートに更新。
   `RULE_COUNT` の `const` アサーションが移送漏れを検出する。

**完了条件**:
- `paredit inspect lint --list-rules` の出力が移行前と**バイト一致**すること
  （`tests/cli/lint_report.rs` と `tests/fixtures/lint_golden/` のゴールデンテストで担保）
- `benches/lint_report.rs` 無改修で動作
- 各 lint パッケージの README.md が存在し、`## 責務` に
  **そのパッケージが担当するルールの一覧（rule 名）**を持つこと。
  §5.2.2 の分割軸は判断に基づくものなので、README の一覧が
  「なぜこのルールがここにいるのか」の唯一の説明になる

### Phase 6: 仕上げ

1. **`docs/src/architecture.md` の全面改稿**。
   - 「4 層が最上位」→「`packages/{core,feature}` が最上位、層は feature 内にネスト」
   - 依存規則の記述を「crate 依存グラフが強制する」に更新
   - 「新しい lint ルールを足す 3 箇所」の手順を新構造に合わせて書き直す
   - 「変更がどこに属するか」の表を feature パッケージ単位に書き直す
   - `public_module_docs_contract.rs` が検査する doc コメント文字列は façade 側に残すため、
     必要なら契約テストの期待値も更新する
   - **各パッケージ README との役割分担を明記する**: `architecture.md` は
     「パッケージ間の関係と依存の向き」を、各 README は「そのパッケージの中身と境界」を持つ。
     同じ内容を両方に書かない
2. `docs/src/contributing.md` / `development.md` に追記:
   - `cargo test -p <package>` の開発ループ
   - **新しいパッケージを足すときの手順**（`Cargo.toml` + `README.md` + `lib.rs` の
     `include_str!` の 3 点セット）
3. **`CLAUDE.md` / エージェント向け指示の更新**: 新しいパッケージ配置と、
   「1 feature の変更は 1 ディレクトリで閉じる」原則、
   **「パッケージを新設したら README を同時に書く」規則**を明文化。
4. **アーキテクチャ契約テストの追加**（**必須**）:
   - `every_workspace_package_documents_itself`（§3.3）:
     `packages/*/*/Cargo.toml` のあるディレクトリすべてに `README.md` が存在し、
     `readme = "README.md"` が宣言され、README 先頭見出しが `name` と一致すること。
     **README を必須にすると決めた以上、人の注意力ではなく CI で守る。**
   - `packages/core/**` の `Cargo.toml` が `paredit-feature-*` に依存していないこと。
     core → feature の逆流をコンパイル前に検出できる。
   - **`domain_logic_never_depends_on_the_cli_argument_parser`**（§3.1）:
     `packages/*/*/src/**` のうちパスに `/cli/` を含まないファイルが `use clap` を
     持たないこと。**層ディレクトリを廃止した代わりの依存規則の担保であり、
     これが無いとスライス優先構造は単なる無秩序になる。**
   - 全メンバーが `[lints] workspace = true` を持つこと（§9.3）。
     忘れても何のエラーも出ないため機械検査が必須。
   - **`root_layer_modules_stay_pure_reexport_facades`**（§3.1.1）:
     `src/{domain,application,infrastructure}` 直下が `mod.rs` のみで、
     その中身が `//!` / `pub use` / `pub mod` に限られること。
     **層ディレクトリを公開 API 名前空間として残す判断のコストは、このテストで払う。**
     これが無いと「とりあえず domain に置く」が移行後に復活する。
   - どちらも `tests/cli/` 配下に置き、既存の contract test 群と同じ扱いにする
     （`Cargo.toml` をテキストとして読む方式で足りる。`cargo metadata` は不要）。

---

## 7. コストとリスク

### 7.1 機械的コスト（大きいが安全）

| 項目 | 量 | 性質 |
| --- | ---: | --- |
| 可視性指定の書き換え | 約 1,940 箇所（§2.5） | **コンパイラが漏れなく指摘する。** 見落としによる実行時バグは起きない |
| `Cargo.toml` の新規作成 | 26 ファイル | 定型 |
| **`README.md` の新規作成** | **26 ファイル** | §3.3 の 6 項目。定型なのは見出しだけで、**中身は書く人が理解していないと書けない**。1 パッケージあたり 30 分〜1 時間を見込む |
| `git mv` 対象 | 約 2,000 ファイル | ファイル内容は原則不変 |
| **機械的 lint 修正**（§9.5 / Phase −1） | 約 1,900 件 | 大半が `cargo clippy --fix` で自動適用。**lint 種別ごとに PR を分ける**ことでレビュー可能に保つ |
| **`anyhow` → `thiserror`**（§9.2） | **430 ファイル** | **機械的ではない。** バリアントの切り方は設計判断であり、本移行で最も判断を要する部分。移送 PR に同梱する（§9.5） |
| rustdoc intra-doc link の書き換え（§11.2） | 372 本（うち 128 本は 1 箇所） | `sed` 相当。ただし移送ファイル内のみが対象 |

`pub` 化により**公開 API 面が意図せず広がる**点だけは注意。core クレートは
`lib.rs` で明示的に `pub use` する項目を絞り、モジュール自体は
`pub(crate)` に留める設計を検討すること。

### 7.2 リスク

| リスク | 深刻度 | 緩和策 |
| --- | --- | --- |
| **移行途中で長期間ブランチが分岐し、main と乖離する** | 高 | §4.1 の façade により **各 Phase が独立してマージ可能**。1 PR = 1〜数パッケージを厳守し、長寿命ブランチを作らない |
| `nix flake check` が新ディレクトリを見ずに落ちる | 中 | 各 Phase で `git add -N packages/...` を必須手順に含める（§Phase 1-7）。これは過去に MSRV チェックが `E0583` で落ちた実績のある落とし穴 |
| ビルド時間が**悪化**する | 中 | クレート数増加でリンク回数が増える。Phase 2 終了時点で §2.3 と同条件の再計測を行い、悪化幅が許容外なら分割粒度を粗くする |
| `pub(in crate::presentation::cli)` 963 箇所の一括 `pub` 化で API が肥大 | 中 | C7 の PR で `lib.rs` の `pub use` を明示リスト化してレビューする |
| lint ルールの分割軸が誤りで、後から再編成が必要になる | 低 | Phase 5 で実測再検証（§Phase 5-1）。最悪 1 パッケージに統合しても機能する |
| E2E テストが feature 単位で分離できず、期待した「feature ごとの test 分離」が得られない | 中 | §1.1 で目的を組み替え済み。**仕様として最初から明示する**（§2.4） |
| **crane の fileset 取りこぼしでテストが「期待値違い」で落ちる** | 高 | §2.5.2 の実測リストを使う。完了条件に**テスト実行件数の一致**を含める。Phase 2.5 は `flake.nix` のみの変更なので revert が容易 |
| crane 移行で `packages.{lint,format,docs}` / `overlays.default` / `lib.mkLintCheck` が壊れる | 中 | これらは外部リポジトリから利用される公開インタフェース。§2.5.3 の「変更してはならないもの」を PR チェックリストにする |
| `checks.{default,formatting,docs}` エイリアスの喪失で組織横断 conformance check が落ちる | 中 | 同上。`nix flake show` の出力を移行前後で diff する |
| `advisory-db` を flake input 化したことで脆弱性検知が `flake-update.yml` の周期分遅れる | 低 | §8-6 の未決事項。許容しないなら現行の独立 CI job を残す |
| crane 版 `nix flake check` がむしろ遅くなる（デリベーション分割によるオーバーヘッド） | 低 | Phase 2.5 完了条件で総 wall time を計測。悪化していれば revert |
| **README が「作った時点で正しく、その後腐る」** | 中 | `include_str!` で rustdoc に載せ（§3.3）、契約テストで存在と見出しを強制する（Phase 6-4）。ただし**内容の鮮度は自動検査できない**。`## 依存` を `[dependencies]` の変更時に必ず見るルールを `CLAUDE.md` に書く（Phase 6-3） |
| README を後回しにして Phase が積み上がり、最後にまとめて 26 本書く羽目になる | 中 | 各 Phase の完了条件に含めた（Phase 1 手順 3 / Phase 3 手順 11）。**移送直後が最も安く書けるタイミング**であり、後で書くと当時の判断を思い出す作業が上乗せされる |
| **`[lints] workspace = true` の書き忘れで `unsafe_code = "deny"` が黙って無効化される** | **高** | §9.3。**何のエラーも出ない**ため人の注意力では守れない。Phase 6 の契約テストで全メンバーを機械検査する |
| `anyhow` → `thiserror` が移送 PR の diff を膨らませ、レビュー不能になる | 高 | パッケージ単位で PR を分ける（26 本）。1 PR で移送 + エラー型変換までを 1 パッケージ分に限定する。それでも大きい場合は「移送のみ」「エラー型のみ」の 2 PR に割る |
| 機械的 lint 修正（Phase −1）が意味的変更を紛れ込ませる | 中 | lint 種別ごとに PR を分割し、`cargo nextest list` の差分ゼロを完了条件にする（§11.1）。`--fix` の結果を鵜呑みにせずレビューする |
| スコープが膨張し、どれも完了しないまま長期化する | **高** | §0 の表のとおり **C（crane）と E（機械的 lint）は独立**。この 2 つを先に完了させ、成果を確定させてから A に入る。A が途中で止まっても façade（§4.1）により常に動作する状態が保たれる |

### 7.3 やらないこと（スコープ外）

- **`flake.nix` の `src` を feature メンバーごとに細分化すること** — 契約テストが
  README/docs/action.yml/workflows をフィクスチャとして読み、`lib.rs` が `include_str!` で
  README を埋め込むため、**リポジトリ全体が 1 ビルド入力である設計は crane 化後も維持する**（§2.5.2）
- **crane の `fileSetForCrate` によるメンバー個別デリベーション** — §2.5.4
- **`cargo-hakari` / `workspace-hack` クレートの導入** — §2.5.4
- **E2E テストの feature クレートへの移送** — §2.4 の制約による
- **公開ライブラリ API の変更** — `paredit_cli::domain::sexpr` 等は façade で維持する
- **1 スライス 1 クレート化** — §5.3

---

## 8. 未決事項

Phase 開始前に判断が必要なもの。

1. **`domain::view_query`（85 行 / 274 箇所から参照）の帰属**。
   本書では `core/sexpr` に置いたが、参照数の多さから独立クレートにする選択もある。
   実装時に中身を確認して確定すること。

2. **`feature/refactor-workflow`（F12）の境界**。
   `application::refactor`（2,184 行）は複数 feature の編集操作を合成するため、
   feature ではなく `core/refactor` とすべき可能性がある。
   Phase 4 で `application/refactor/**` の `use` を実測してから確定する。

3. **`presentation/cli/io.rs`（4,782 行）を分割するか**。
   `core/cli` に丸ごと移すだけでも移行は成立するが、単一ファイルとしては大きい。
   Phase 2 の作業量を膨らませないため、**分割は別課題に切り出すことを推奨**。

4. **`feature/lint-*` の分割軸**（§5.2.2）。Phase 5 で実測により確定。

5. **ビルド時間悪化時の撤退ライン**。Phase 2 終了時点の再計測で、
   cold `cargo check --all-targets` が現状 48.8 秒からどこまで悪化したら
   分割粒度を粗くするか、数値基準を事前に合意しておくこと。

6. **`cargo audit` を `nix flake check` に取り込むか**（§2.5.3 / §2.5.4）。
   現行は `ci.yml` の独立 job（`nix develop --command cargo audit --deny warnings`）。
   crane の `cargoAudit` + `advisory-db` input に移すと hermetic になり CI job を 1 つ削減できるが、
   トレードオフが 2 つある:
   - **`advisory-db` の更新が `flake.lock` 更新に依存する**ため、脆弱性の検知が
     `flake-update.yml` の実行間隔だけ遅れる。現行はジョブ実行のたびに最新 DB を取得している。
   - `ci_runs_the_flake_checks_and_audits_dependencies` の期待値更新が必要になる（§2.5.4）。

   **推奨は現状維持**（`advisory-db` input も追加しない）。crane 導入の主目的は
   `buildDepsOnly` による依存ビルド共有（§1.3）であり、audit の移設はそれとは無関係。
   移設する場合は Phase 2.5 とは別 PR にすること。

7. **`checks.doc`（`craneLib.cargoDoc`）を追加するか**。
   `devShells` の案内には `cargo doc --no-deps` があるが CI では実行していない。
   crane 化のついでに `RUSTDOCFLAGS = "--deny warnings"` で追加できるが、
   26 パッケージ分の rustdoc は無視できない時間になる可能性がある。Phase 2.5 で実測して判断。
   **§11.2 の intra-doc link 372 本を自動検出できるようになる**ため、判断材料としては加点。

8. **`semantic_coverage`（766 行・参照ゼロ）をどこへ置くか**（§11.3）。
   `core/semantics` 同梱 / `benches/` へ移動 / 削除の 3 択。
   **Phase 2 の `core/semantics` 切り出し前に決めること。**

9. **`#[non_exhaustive]` を使わない方針で合意できるか**（§9.4）。
   本書は「使わない」を推奨している（`match` の網羅性検査を優先するため）。
   将来いずれかのパッケージを crates.io に publish する計画があるなら、
   この判断は覆る。現時点では `publish = false` が全メンバーの前提（§3.2）。

10. **`[lints.clippy]` にどの lint を昇格させるか**（§9.3 / Phase −1）。
    3,608 件の警告のうち、恒久的に `warn`/`deny` にするものを個別に選ぶ必要がある。
    `pedantic` の一括有効化は推奨しない。

11. **ルート `src/` の層ディレクトリを将来フラット化するか**（§3.1.1）。
    本書の推奨は「残す」。理由は `CHANGELOG.md` 1.1.0 が
    `paredit_cli::domain::semantics` を公開 API として告知しており、
    畳むと直近リリースで公開したパスを次で壊すことになるため。
    **メジャーバージョンを上げる機会があれば再検討の価値がある**
    （`tests/` は既にフラットな `paredit_cli::{sexpr,dialect}` を使っており、
    移行先は用意されている）。判断を先送りしても移行は完了できる。

---

## 9. 型の厳格化と Rust ベストプラクティス適用

移行と同時に行う大規模リファクタリング。**移行の「ついで」ではなく、
パッケージ境界が確定するこのタイミングでしか安くできない作業**が含まれる。

### 9.1 実測: 現状の型の緩さ

```
cargo clippy --all-targets -- -W clippy::pedantic -W clippy::nursery
→ 警告 3,608 件
```

| 件数 | 警告 | 性質 |
| ---: | --- | --- |
| 967 | `#[must_use]` を付けるべき（method 621 + function 346） | 機械的 |
| 544 | `const fn` にできる | 機械的 |
| 364 | 末尾セミコロンの一貫性 | 機械的 |
| 305 | `Result` を返す関数に `# Errors` 節がない | 半機械的 |
| 275 | **`pub(crate)` が private module 内にある**（`redundant_pub_crate`） | **分割で自然解消** |
| 271 | 値渡しなのに消費していない（`needless_pass_by_value`） | 要判断 |
| 160 | doc コメント第 1 段落が長すぎる | 機械的 |
| 120 | ワイルドカード import（`use super::*;`） | 要判断 |
| 68 | `format!` にインライン変数を使える | 機械的 |
| 17 | `usize` → `f64` の精度欠落 | **要修正（正しさ）** |
| 15 | 構造体に bool が 4 つ以上 | **要修正（型設計）** |
| 9 | `f32`/`f64` の厳密比較 | **要修正（正しさ）** |

その他の実測:

| 項目 | 実測 | 評価 |
| --- | ---: | --- |
| **domain で本番コードが `anyhow` を使うファイル** | **430 / 435** | §9.2。最大の課題 |
| `thiserror` を使っている domain ファイル | 3 | 正しいパターンは既に存在する |
| struct の `bool` フィールド総数 | 910 | §9.4 |
| `#[must_use]` の使用箇所 | 3 | ほぼ未使用 |
| `#[non_exhaustive]` の使用箇所 | 0 | 未使用 |
| 数値 `as` キャスト | 43 | §9.4 |
| `.expect(` | 2,329 | 大半はテスト。本番分の切り分けが必要 |
| `.unwrap()` | 780 | 同上 |

### 9.2 最重要: `anyhow` を domain / application から追放する

`docs/src/architecture.md` は domain を
「不正な状態を型のレベルで閉じる」「安定した中核」と定義している。
しかし実測では **domain の 435 ファイル中 430 が本番コードで `anyhow` を使う**。

```rust
// 現状の典型（src/domain/**/*.rs）
pub fn plan_rename_function(request: RenameFunctionRequest<'_>) -> anyhow::Result<RenameFunctionPlan>
```

`anyhow::Error` は**型消去された動的エラー**である。これが意味するのは:

- 呼び出し側はエラーを `match` できない。文字列を見るしかない
- 「このシンボルは存在しない」と「ファイルが読めない」を型で区別できない
- CLI の exit code 分岐（`docs/src/agents.md` のコード表）が、
  本来なら型で決まるはずのものを**文字列や別経路の情報から再導出している**

`anyhow` は**アプリケーションの最外殻で使う道具**であり、ライブラリクレートで使うものではない。
26 パッケージ化すると domain/application はすべてライブラリクレートになるので、
この問題は「設計上の望ましさ」から**「クレート境界の設計ミス」**に格上げされる。

#### 目標形

| 層 | エラー型 |
| --- | --- |
| `packages/core/*` / `packages/feature/*` の domain・application | パッケージごとに `thiserror` の enum を定義。`pub enum RenameError { SymbolNotFound { .. }, ... }` |
| ルート `paredit-cli` の presentation | `anyhow` を継続使用。各パッケージのエラー型を `?` で吸い上げ、exit code にマップする |

既に `src/domain/similarity_report/options.rs` の `SimilarityReportOptionsError` が
**正しいパターンの実例**として存在する。これを全パッケージに横展開する。

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum SimilarityReportOptionsError {
    #[error("--threshold must be between 0.0 and 1.0")]
    ThresholdOutOfRange,
    // ...
}
```

#### 副次効果

- exit code のマッピングが `match` になり、**網羅性をコンパイラが検査する**
- 新しい失敗モードを追加したとき、CLI 側の対応漏れが**コンパイルエラーになる**
- エラーメッセージが型に紐づくため、`tests/cli/*` の文字列アサーションが構造化できる

#### やらないこと

- `presentation` から `anyhow` を外すこと。ここは正しい使い方であり、384 ファイルを触る意味がない
- 1 エラー 1 バリアントの機械的変換。**バリアントは呼び出し側が区別したい単位で切る**。
  区別しないなら 1 バリアントでよい

### 9.3 `[workspace.lints]` — 忘れると静かに壊れる

現行ルート `Cargo.toml`:

```toml
[lints.rust]
unsafe_code = "deny"
missing_debug_implementations = "warn"

[lints.clippy]
all = "warn"
```

**`[lints]` はパッケージ単位の設定であり、workspace メンバーに自動継承されない。**
`[workspace.lints]` を定義し、**各メンバーに `[lints] workspace = true` を書かなければ、
26 パッケージで `unsafe_code = "deny"` が黙って無効化される。**

`unsafe_code = "deny"` はこのプロジェクトの安全性主張の土台なので、
**これは静かな重大リグレッションになりうる。**

```toml
# ルート Cargo.toml
[workspace.lints.rust]
unsafe_code = "deny"
missing_debug_implementations = "warn"

[workspace.lints.clippy]
all = "warn"

# 各メンバー packages/*/*/Cargo.toml — 全 26 個に必須
[lints]
workspace = true
```

**Phase 6 の契約テストに、全メンバーが `[lints] workspace = true` を持つことの検査を追加する**
（§3.3 の README 検査と同じテストに含めてよい）。README と違い、
これは忘れても**何のエラーも出ない**ため、機械検査の価値がとりわけ高い。

同様にワークスペース継承すべきもの:

```toml
[workspace.package]
version = "<ルート [package] の version と同じ値>"   # 下の「注意」参照
edition = "2024"
rust-version = "1.85"
license = "MIT"
repository = "https://github.com/nerima-lisp/paredit-cli"

[workspace.dependencies]
anyhow = "1.0"
clap = { version = "4.6", features = ["derive"] }
thiserror = "2.0"
# ... 26 パッケージ間でバージョンが分岐しないよう一元管理する
```

> **注意**: ルート `Cargo.toml` の `[package]` セクションは
> `rust-version = "1.85"` / `license = "MIT"` 等を **literal のまま残す**
> （`crate_metadata_contract.rs` が文字列検査しているため / §Phase 0）。
> `[workspace.package]` と重複するが、重複は契約テストが守る。

### 9.4 型設計の強化

#### bool の追放（boolean blindness）

struct の `bool` フィールドは実測 **910 個**、うち clippy が
「4 つ以上の bool を持つ構造体」として警告するものが **15 箇所**。集中しているのは:

```
src/presentation/cli/refactor/args/{execute,plan,preview}.rs
src/presentation/cli/refactor/types/{apply,check,diff}.rs
src/domain/inline_function/types.rs
src/domain/split_file/types.rs
src/domain/refactor_execute.rs
```

`architecture.md` が既に方針を明記している —
「派生的な表示値（booleans, counts）は保存せず、シリアライズ境界で導出する」
「相関する `bool`/`usize` の袋ではなく、検証済み newtype か意味を持つ enum を選ぶ」。

**この方針は書かれているが、上記 15 箇所には適用されていない。**
移行時に `ReportLimit::{Complete, Limited(NonZeroUsize)}` と同じ形へ寄せる。

| 現状 | 目標 |
| --- | --- |
| `struct { dry_run: bool, force: bool, verbose: bool, json: bool }` | 意味のある enum に分解（`Mode::{DryRun, Apply}` 等）。同時に真になり得ない組み合わせを型で潰す |
| `fn f(..., recursive: bool)` — 公開 fn の bool 引数 158 箇所 | 呼び出し側で `f(x, true)` が読めない。2 値 enum を導入する |

#### `#[must_use]` / `#[non_exhaustive]`

| 属性 | 現状 | 方針 |
| --- | ---: | --- |
| `#[must_use]` | 3 箇所（clippy は 967 箇所を提案） | **`Plan` / `Report` / `Decision` を返す関数に付ける。** 「計画を作ったが使わなかった」はこのコードベースでは常にバグ |
| `#[non_exhaustive]` | 0 箇所 | **公開 enum に付けない**。26 パッケージは内部利用のみ（`publish = false`）であり、`non_exhaustive` は `match` の網羅性検査を殺す。§9.2 で得た網羅性を自ら捨てることになる |

`#[non_exhaustive]` を**あえて使わない**判断は README（§3.3）に記録すること。

#### 数値変換

- `as` キャスト 43 箇所のうち **`usize as f64` 17 箇所は精度欠落**（clippy `cast_precision_loss`）。
  類似度計算（`similarity_report`）に集中しているとみられ、**正しさの問題**である
- `f32`/`f64` の厳密比較 9 箇所も同様。`SimilarityRatio` のような検証済み newtype は
  既に存在するので、比較もその型のメソッドに寄せる
- 残りの `as` は `TryFrom` + 明示的なエラー処理に置き換える

### 9.2.1 【実装時の追記】パッケージ境界ができた後の §9.2 実施計画

Phase 6 完了時点での実測。**§9.2 はパッケージ単位で独立に実施できる状態になった。**

#### 呼び出し側は `?` で素通しするだけなので、戻り値型だけ変えればよい

最重要の実測結果: `discover_workspace_files` の呼び出し側は**どこも失敗の種類で
分岐していない**。そして `anyhow::Result<T>` の `?` は
`E: std::error::Error + Send + Sync + 'static` を吸収するため、
**関数の戻り値を具体的なエラー型に変えても呼び出し側は 1 行も変わらない。**

これが「1 パッケージ = 1 PR」を可能にする。24 パッケージを一斉に変換する必要はない。

#### パッケージ別の作業量（本番コードで `anyhow` を使うファイル数）

| パッケージ | 該当ファイル | 備考 |
| --- | ---: | --- |
| `core/workspace` | 2 / 7 | 最小。ただし失敗 19 箇所 |
| `core/cli` | 3 / 7 | I/O 規約そのものなので context が多い |
| `core/lint-engine` | 3 / 22 | |
| `core/syntax` | 5 / 63 | |
| `core/edit` | 8 / 14 | `ReaderConditionalSafetyError` が既にある |
| `feature/*` | 7〜35 / 各 | |

#### バリアントの切り方 — `core/workspace` の実測例

19 の失敗箇所は、**呼び出し側が別々に扱いたい 2 種類**に割れる:

| 種類 | 箇所 | 呼び出し側にとっての意味 |
| --- | ---: | --- |
| 上限超過 | 8 | 入力が大きすぎる。`--include` を絞れ |
| 安全性拒否 | 11 | ファイルが走査中に置き換わった / 正規パス外 / 非通常ファイル |

この 2 つは**別の exit code に値する**。§9.2 が「exit code の分岐が本来なら型で
決まるはずのものを文字列から再導出している」と述べているのは、まさにこの区別が
今は文字列にしか存在しないという意味である。

```rust
#[derive(Debug, Error)]
pub enum WorkspaceDiscoveryError {
    #[error("{0}")]
    LimitExceeded(#[from] WorkspaceLimitExceeded),
    #[error("{0}")]
    Refused(#[from] WorkspaceRefusal),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
```

`WorkspaceLimitExceeded` / `WorkspaceRefusal` の各バリアントが
**現在のメッセージ文字列をそのまま `#[error("...")]` に持つ**。
これで CLI 出力はバイト一致のまま、種類が `match` 可能になる。

#### やってはいけないこと（実測で確認した罠）

- **`.context(...)` を機械的にバリアント化しない。** `core/cli` の I/O 経路は
  context でパスや操作名を足しており、これを 1 バリアント 1 context に展開すると
  バリアントが爆発する。I/O は `#[error(transparent)] Io(#[from] std::io::Error)`
  に集約し、context は呼び出し側の `anyhow` 層で足す。
- **メッセージを「整理」しない。** `tests/cli/**` の文字列アサーションと
  `inspect capabilities` のゴールデンが全て落ちる。§9.2 の目的は
  メッセージの改善ではなく**型による区別**である。

---

### 9.2.2 【実施後の追記】3 パッケージを変換して分かったこと

`core/workspace` / `core/syntax` / `core/lint-engine` を変換した実測。
**§9.2.1 の見積もりは 2 点で外れていた。**

#### 外れ 1: ファイル数はコストの指標にならない

§9.2.1 は「本番コードで `anyhow` を使うファイル数」で作業量を見積もった。
実際に効くのは**失敗箇所の数**と、それが**何種類に割れるか**である。

| パッケージ | ファイル数（§9.2.1 の見積もり） | 実際の失敗箇所 | 比 |
| --- | ---: | ---: | ---: |
| `core/workspace` | 2 | 52 | 26x |
| `core/syntax` | 5 | 58 | 12x |
| `core/lint-engine` | 3 | 5 + トレイト 1 | 2x |
| `core/edit` | 8 | **100** | 12x |
| `core/cli` | 3 | 46 | 15x |

`core/edit` は「8 ファイル」だが実際は 100 メッセージで、最大の core パッケージ
である。ファイル数で並べた §9.2.1 の表は**順序が誤っている**。

#### 外れ 2: lint パッケージ 6 個は 1 行の変更で終わる

`feature/lint-*` は 536 ファイルが `anyhow` に触れており、§9.2.1 の表では
最大の作業に見える。**実測すると、536 ファイル全部が `use anyhow::Result;` の
1 行だけ**で、`bail!` も `context()` も `anyhow!` も 1 箇所もない。理由は
`LintRule::check` の戻り値型が `anyhow::Result<()>` だったからで、
**トレイトのシグネチャ 1 個が 536 ファイルを汚染していた。**

| 種類 | ファイル数 | `Result<` の出現 | 変換 |
| --- | ---: | ---: | --- |
| `rule.rs` | 134 | 各 1 | トレイトと同時に `LintResult` へ |
| `domain.rs` | 134 | 各 1 | 同上 |
| `cli/**` | 268 | 各 1 | `core/cli` の変換待ち |

268 ファイルを機械置換し、**壊れた呼び出し側は 2 箇所**（ルート façade の
ラッパ 2 個）だけだった。

#### 実測: `LintRule::check` は 134 ルール中 4 個しか失敗しない

`anyhow::Result<()>` は「何でも起こりうる」と言う型なので、
**本当は何も起きないという事実を隠していた**。134 ルールの `check` 本体を
走査すると、`?` を含むものは 4 個のみで、4 個とも失敗の原因は同じ
（木全体を見るルールがパスを解決し、解決できない）。

これは「バリアントが少なすぎて型にする価値がない」ではなく、逆である。
**集合が小さいことを型で言えるのが利益**であり、`LintError` は 1 バリアントでも
「ルールは I/O もリソース制限も報告しない」と述べている。

#### 実測: メッセージの反復は型の設計を教える

`core/edit` の 100 メッセージを正規化すると、**59% が 20 個の同じ形**に潰れる:

| 回数 | 形 |
| ---: | --- |
| 6 | `<op> cannot rewrite a form containing comments` |
| 4 | `<op> conservatively rejects declarations` |
| 4 | `<op> supports only Common Lisp and Emacs Lisp` |
| 4 | `<op> requires a plain binding list` |
| 4 | `<op> input is not valid` |

7 つの編集ファミリ（`convert-*` / `merge-nested-*` / `split-let*` / `flatten-progn` …）
が**同じ 7 種類の拒否理由**を、操作名だけ変えて書き直している。
`core/edit` の変換は「ファイルごとに enum を作る」のではなく、
**`operation: &'static str` を持つ 1 個の `EditRefusal`** にすべきである。

#### 実測: 型消去された payload を持つ typed error が既にあった

`SimilarityCandidateCollectionError` は `thiserror` の enum でありながら
`Selection(#[from] anyhow::Error)` を持っていた。**§9.2 が消そうとしている形が、
既に「変換済み」に見えるコードの中にいた。** 変換の対象を探すときは
`use anyhow` だけでなく `anyhow::Error` を payload に持つ enum も探すこと。

#### 実測: 文字列の前方一致が制御フローになっていた

`core/syntax` の `validate_edit_context` は、失敗メッセージに操作名を前置するか
どうかを `error.to_string().starts_with("input ")` で決めていた。
**人間向けの文章への前方一致が分岐条件になっており**、誰かがメッセージを
書き直せばテストが落ちないまま挙動が変わる状態だった。これが §9.2 の
「exit code の分岐を文字列から再導出している」の最も具体的な実例である。

#### 変換順序（クレート依存順、実測）

```text
core/syntax ✅ → core/semantics (11) → core/lint-engine ✅ → core/edit (100)
     └→ core/workspace ✅                                        └→ core/cli (46)
                                                                      └→ feature/* (18 個)
```

`core/semantics` と `core/edit` は `core/lint-engine` と独立なので順不同。
`feature/*` は全て `core/cli` の後。

---

### 9.2.3 【実施後の追記】core 6 パッケージ完了時点の実測

#### `{:#}` は anyhow の機能であり、型付きエラーには無い

**16 個のテストが `format!("{error:#}")` で落ちた。** `anyhow` の `{:#}` は
`source()` を辿って `": "` で連結するが、`thiserror` の `Display` は
**最外殻のメッセージしか出さない**。

CLI 出力は変わらない（`main` が `anyhow::Error` に変換するため）が、
**ワークスペース内のテストは変換を経ずに直接アサートしている**ので落ちる。
対策は `chain()` を型に生やし、テストをそれに向けること:

```rust
pub fn chain(&self) -> String {
    let mut rendered = self.to_string();
    let mut source = std::error::Error::source(self);
    while let Some(error) = source {
        rendered.push_str(": ");
        rendered.push_str(&error.to_string());
        source = error.source();
    }
    rendered
}
```

**変換前に `grep -rn 'format!("{[a-z_]*:#}")'` を実行して件数を数えること。**
これは §9.2.1 の「罠」リストに無かった最大の落とし穴である。

#### `.context()` を残すべき箇所と、変えるべき箇所の判定基準

`core/cli` の 54 箇所は 2 つに割れた:

| 種類 | 件数 | 変換先 |
| --- | ---: | --- |
| `io::Error` に**パスと操作名**を足すだけ | 34 | `CliError::Io { context: String, #[source] source }` |
| CLI が**判断して拒否**している | 20 | `IoRefusal` / `WriteTargetError` / `ArgumentError` の各バリアント |

判定基準は「**そのメッセージは呼び出し側が分岐に使えるか**」。
`failed to open or inspect {path}` は使えない（`io::Error` の種類で分岐する）。
`refusing target changed since parsing {path}` は使える（再読み込みして再試行）。

#### 型付けが明らかにした設計上の疑問: cleanup が primary error を隠している

`write_files_with_rollback` は書き込み失敗のあと後始末に失敗すると
`error.context("rollback/cleanup also failed: ...")` としていた。
`anyhow` では **context が最外殻**になるので、ユーザーに最初に見えるのは
「後始末に失敗した」であり、**本当の原因である書き込み失敗は原因欄に埋もれる**。

型にすると `CleanupFailure { summary, cause: Box<CliError> }` となり、
この入れ子が明示される。**出力は変えていない**（変えれば挙動変更）が、
2 つの失敗が別々に到達可能になった。

同じ理由で `BackupCleanupAfterCommit` を独立バリアントにした。これは
**「書き込みは成功した」失敗**であり、呼び出し側が「操作は起きなかった」と
読んではいけない唯一のケースである。型が無ければこの区別は
メッセージの読解に委ねられていた。

#### core 完了時点の波及実測

| パッケージ | 失敗箇所 | 波及した呼び出し側 |
| --- | ---: | ---: |
| `core/workspace` | 52 | 0 |
| `core/syntax` | 58 | 7 |
| `core/lint-engine` | 5 + トレイト | 2 |
| `core/semantics` | 11 | 0 |
| `core/edit` | 107 | 8 |
| `core/cli` | 54 | 20（うち 18 は 1 ファイル） |

**合計 287 箇所の変換に対して波及は 37 箇所。** §9.2.1 の
「呼び出し側は `?` で素通し」は正しかった。波及したものは全て
`?` を使っていない箇所（末尾式・関数ポインタ型・`#[from] anyhow::Error`）である。

#### `core/cli` は `anyhow` を落とせない（そして落とすべきでない）

2 箇所だけ残る:

- `gate_failure` — 型消去されたマーカーを作り、dispatch 層が `downcast` する
- `terminal_safe_error_chain` — `anyhow` のチェーンを端末向けに描画する

どちらも `main` が `anyhow::Result` を返すことに由来する**境界のヘルパ**であり、
§9.2 が追放しようとしている「domain / application の型消去」ではない。

### 9.2.4 【完了時の追記】`anyhow` はワークスペースから消えた

`feature/*` 全 29 パッケージと root crate、`xtask` を変換し、
**`Cargo.lock` から `anyhow` が消えた。** §9.2.3 が「落とせない、
そして落とすべきでない」と書いた `core/cli` の 2 箇所も無くなっている。

#### §9.2.3 の「落とせない 2 箇所」は落とせた

| 箇所 | §9.2.3 の判断 | 実際 |
| --- | --- | --- |
| `gate_failure` | 型消去マーカーは境界のヘルパだから残す | `CommandFailure::Gate` バリアントにした |
| `terminal_safe_error_chain` | anyhow のチェーンを描画するから残す | `&dyn Error` を取り `source()` を自力で辿る |

判断の誤りは「`main` が `anyhow::Result` を返す」を**前提**として扱った点にある。
それは選択であって制約ではなかった。

#### 実測: `downcast` の連鎖が 107 の拒否を「ツールの不具合」にしていた

§9.2 は「exit code の分岐が文字列から再導出されている」ことを問題にしたが、
`core` 変換後に残っていたのは**文字列ではなく型消去された値からの再導出**、
すなわち `diagnosis::classify` の `downcast_ref` 連鎖である。
そして末尾は `.map_or(ErrorCode::Internal, ...)` だった。

`main` のバイナリで実測した結果:

| 実行 | 変換前の code | 変換後 |
| --- | --- | --- |
| `refactor add-export --package nope` | `internal.unclassified` | `selection.no-match` |
| `refactor merge-nested-let`（Scheme） | `internal.unclassified` | `input.dialect-unsupported` |
| `refactor rename-binding --write`（`--file` 無し） | `internal.unclassified` | `argument.write-requires-file` |

`category_description` は "unclassified; a defect in this tool"。
**ごく普通の利用者側の誤りを、エージェントに「このツールが壊れている」と
報告していた。** 原因は `CliError` に `EditRefusal` のバリアントが無く、
`?` が `anyhow` に吸収し、probe が 1 つも一致しなかったこと。
`core/edit` の 107 の拒否**全部**がこの経路だった。

3 番目の行は別種の証拠になる。`--write requires --file` は
`ArgumentError::WriteRequiresFile` として**既に存在していた**のに、
`bail!("--write requires --file")` が 36 箇所で同じ文字列を書き直しており、
同一の利用者エラーが経路によって 2 つの code を持っていた。

#### 型消去は「ポート」の名の下でも起きていた

4 つの hexagonal port が `anyhow::Result` を返しており、いずれも
「ユースケースはアダプタの失敗を知るべきでない」と正しく説明されていた。
**説明は正しく、道具が誤っていた。** `anyhow::Error` は
「名前を知らない何らかのエラー」ではなく「エラー型が無い」を意味し、
分類まで一緒に捨てる。関連型がこれを型で言う:

```rust
pub trait DefinitionSourcePort {
    type Error: Into<CliError>;
    fn load(&mut self, file: &FsPath) -> Result<LoadedDefinitionSource, Self::Error>;
}
```

境界が閉じている（code カタログは文書化された契約）一方で
アダプタ集合は開いている、という非対称性がこの形の理由である。

#### 閉じた分類 × 開いたペイロード

`CliError` が 29 個の feature のエラーを列挙することは依存方向の反転になる。
そこで `FeatureRefusal { code, message }` を置き、**code を必須の
コンストラクタ引数**にした。ペイロード（`source()` の連鎖）は
その場で文字列に潰れるが、**決定（どの code か）は型のまま残る**。
`anyhow` はペイロードと決定の両方を捨てていた。ここが違いである。

#### 追加した code は 2 つ

| code | 理由 |
| --- | --- |
| `input.dialect-unsupported` | `input.shape-refused` と**取るべき行動が逆**。前者は「このファイル内で別の form を選べ」、後者は「このファイルは言語が違う、ここで再試行するな」 |
| `argument.flag-combination` | 16 箇所の `bail!` が「このフラグ同士は併用できない」を各々別の文で書いていた |

#### DDD: root crate の 7 モジュールのうち 5 は composition root ではない

`architecture_contract.rs` の許可リストは「複数の feature を集約するもの」を
composition root と定義している。実測すると、`AWAITING_EXTRACTION` の
7 個は**どれもこの条件を満たさない** — import は `crate::domain` の
re-export façade を経由して `packages/core/*` にのみ解決する
（`duplicate_export_report` だけが feature を 1 つ参照するが、
feature → feature は既に許された形である）。

つまり「まだ動かしていない」だけであり、`COMPOSITION_ROOT` と同じリストに
混ぜていたことが「一時的」を恒久化させる仕組みだった。2 つのリストに分け、
**backlog が増えないことをテストで固定**した（コメントは fail しない）。
実際の抽出はパッケージ 1 個ごとの独立した作業なので、本変換には含めない。

---

### 9.5 順序が結果を決める

**機械的な修正を分割の前にやるか後にやるかで、総コストが大きく変わる。**

| 分類 | 件数 | いつやるか | 理由 |
| --- | ---: | --- | --- |
| `redundant_pub_crate` | 275 | **やらない** | パッケージ分割で `pub` 化されるため自然消滅（§2.5 / §7.1）。先に直すと二度手間 |
| `must_use` / `const fn` / セミコロン / `format!` インライン化 | 約 1,900 | **分割前**（Phase −1） | 1 クレートで一括処理できる。分割後だと 26 回に分散し、移送 PR の diff とも衝突する |
| `# Errors` doc の追加 | 305 | **§9.2 と同時** | エラー型を作り直すので、その時に書く |
| `anyhow` → `thiserror` | 430 ファイル | **パッケージ移送と同時**（Phase 3〜5 の各 PR 内） | エラー型の境界 = パッケージ境界。移送時に一緒にやるのが最も安い |
| bool → enum / 数値型 | 15 struct + 43 cast | **分割後**（Phase 7） | 設計判断を要する。移送 PR に混ぜると差分が読めなくなる |
| `needless_pass_by_value` / wildcard import | 391 | **分割後**（Phase 7、任意） | 判断を要し、正しさへの影響が小さい |

---

## 10. 追加フェーズ

§6 の Phase 構成に、以下を挿入・追加する。

### Phase −1: 機械的 lint 修正（Phase 0 の前）

**分割を始める前に、単一クレートのうちに機械的な修正を済ませる。**

1. `cargo clippy --fix --all-targets -- -W clippy::pedantic` を段階的に適用。
   **一度に全部やらない。** lint 種別ごとに PR を分け、それぞれ `cargo nextest run` で検証する。
2. 対象は §9.5 の「分割前」行のみ。`redundant_pub_crate` は**明示的に除外**する
   （`-A clippy::redundant_pub_crate`）。
3. 適用後、`[lints.clippy]` に定着させた lint を昇格させる:
   ```toml
   [lints.clippy]
   all = "warn"
   must_use_candidate = "warn"        # 定着したものから個別に足す
   ```
   **`pedantic = "warn"` を丸ごと有効化しない。** 3,608 件のうち
   本プロジェクトが受け入れないもの（`needless_pass_by_value` 等）が混ざるため、
   個別 lint 名で足していく。

**完了条件**:
- `cargo clippy --all-targets -- -D warnings` が緑
- `cargo nextest run --locked` のテスト件数と結果が変化なし
- **差分がすべて機械的であること**（レビューで意味的変更が見つかったら分離する）

**この Phase は独立している。** 移行全体を中止しても、ここまでの成果は残る。

### Phase 7: 型設計の強化（Phase 6 の後）

§9.4 の内容。パッケージごとに独立した PR にできるため、
**Phase 6 完了後に並行して進められる**。

1. bool 4 個以上の struct 15 箇所を enum へ分解
2. 公開 fn の bool 引数 158 箇所を 2 値 enum へ（利用頻度の高いものから）
3. `usize as f64` 17 箇所と f64 厳密比較 9 箇所を検証済み newtype のメソッドへ
4. `#[must_use]` を `Plan` / `Report` / `Decision` の生成関数へ付与

**完了条件**: パッケージごとに、README の `## 公開している型・関数` が
新しい型を反映していること（§3.3）。

---

## 11. 移行前に必ずやっておくこと

### 11.1 CLI 表面のゴールデンスナップショットを取る

`paredit inspect capabilities --output json` は **clap のコマンドツリー全体**
（サブコマンド・引数・help 文字列・デフォルト値）を JSON で出力する。

```
paredit inspect capabilities --output json > /tmp/capabilities-before.json
```

`command.rs` / `dispatch.rs` を 26 パッケージに向けて書き換える作業は、
**引数の取り違え・サブコマンドの欠落を静かに起こしうる**。
各 Phase の完了確認でこの JSON を diff することが、最も安く効く回帰検出になる。

同様に取得しておくもの:

```
paredit --help                          > /tmp/help-before.txt
paredit inspect lint --list-rules       > /tmp/rules-before.txt   # 全ルール名（§Phase 5）
cargo nextest list                      > /tmp/tests-before.txt   # 全テスト名
```

**`cargo nextest list` の差分がゼロであること**を各 Phase の完了条件に加える。
テスト件数だけでなく名前まで見ることで、`#[path]` 付き mod（`tests/cli.rs`）の
取りこぼしを検出できる。

### 11.2 rustdoc の intra-doc link 372 本の扱いを決める

実測: `src` 配下に `[`crate::...`]` 形式の intra-doc link が **372 本**。
うち **128 本が `crate::domain::view_query::for_each_subview` 1 箇所に集中**しており、
そのすべてが `src/domain/` 配下（＝移送対象）にある。

- **移送されないファイル内のリンクは無傷**。ルート façade（§4.1）が
  `crate::domain::view_query` を解決し続けるため
- **移送されるファイル内のリンクは全滅する**。移送先クレートでは
  `crate::domain::view_query` というパスが存在しない

対応方針:

| 方針 | 内容 |
| --- | --- |
| 推奨 | 移送時に `crate::domain::X` → `paredit_core_sexpr::X` へ書き換える。他パッケージを指すリンクは**フルパスのクレート名**で書く |
| 検出 | `cargo doc --no-deps 2>&1 \| grep 'unresolved link'` を各 Phase で実行。§8-7 の `checks.doc` を入れるなら自動化される |

**128 本が 1 箇所に集中している**ため、実作業は `sed` 1 回に近い。恐れる必要はないが、
**放置すると rustdoc が静かに劣化する**（デフォルトでは warning 止まり）。

### 11.2.1 【実施後の追記】intra-doc link は 19 本壊れていた

§11.2 は移行前に「372 本の intra-doc link をどう扱うか決めよ」と言っている。
Phase 6 完了後に実測すると、**19 本が解決不能なまま残っていた**。

内訳はすべて移行の落穂拾いである:

| 種類 | 本数 | 内容 |
| --- | ---: | --- |
| `crate::domain::X` | 10 | 単一クレート時代は有効。X は今や別パッケージか、リネームされている |
| `RULES` / `CATEGORIES` | 4 | **エンジンは意図的にレジストリを知らない**（§4.2 の `RuleCatalog` 反転）。リンクは反転が取り除いた結合を要求している |
| その他 | 5 | `PackageId`、`LICENSE`、Lisp 構文の `[result]` |

加えて **18 本の「public な doc が private な項目にリンクしている」警告**があった。
単一クレート時代に `pub(crate)` だった項目が、パッケージ化で doc だけ public になり、
リンク先は private のまま残ったもの。

対応方針:

- **別パッケージの項目はプレーンなコードスパンにする。** 依存を doc コメントのために
  増やすのは本末転倒であり、解決しないリンクは無いより悪い
- **private な実装テーブル（`KEY_HEADS` 等）もプレーンにする。** リンクを通すために
  `pub` にすると、doc コメントのためにパッケージの API が広がる
- `RULES` / `CATEGORIES` は「カタログの登録順」等の記述に置き換える。
  リンクできないこと自体が §4.2 の設計の帰結である

結果、37 本の警告が **2 本**になった。残る 2 本は
`--fail-on-violation` の clap ヘルプ内の `(var form [result])` で、
rustdoc はこれを未解決リンクと読む。**エスケープすると clap がそのまま表示するため
CLI 表面が変わる。** §11.1 のゴールデンが CLI 表面を固定している以上、
警告 1 本のほうが安い。理由はコード内にコメントとして残した。

> この作業中に、`rule.rs` の doc コメントを直すつもりの正規表現が
> **ルールの説明文字列と finding メッセージまで書き換え**、
> lint ゴールデンテストが落ちた。doc コメントとユーザー可視文字列が
> 同じファイルの数行違いに同居しているので、
> **行番号ではなく「`//!` か `///` か、それ以外か」で区別すること。**

### 11.3 `semantic_coverage`（766 行）の帰属を決める

`src/application/usecase/semantic_coverage.rs` は **766 行あるが、
`usecase/mod.rs` の `pub mod` 宣言以外からまったく参照されていない。**
CLI サブコマンドも、テストからの利用も、bench からの利用もない
（ファイル内の `#[cfg(test)]` テストのみが実行経路）。

> **【実装時の訂正 — 「参照ゼロ」は誤り】**
>
> `examples/semantic_coverage.rs`（コミット `a335358` で追加済み・追跡下）が
> **公開 API 経由でこのモジュールを使っている**:
>
> ```rust
> use paredit_cli::application::usecase::semantic_coverage::{
>     SemanticCoverageRequest, SemanticCoverageSourcePort, build_semantic_coverage_report,
> };
> ```
>
> `cargo metadata` にも `example` ターゲットとして現れる。したがって:
>
> - **削除は選択肢から外れる**（§8-8 の 3 択のうち 1 つが消える）
> - 「`benches/` に移す」案は **`examples/` の開発ハーネスとして既に実現済み**
> - 帰属は「計測対象と同居」＝ `core/semantics` でよい。
>   ただし**移送後もルート façade が
>   `application::usecase::semantic_coverage` を再エクスポートし続けること**が
>   この example のコンパイル条件になる
>
> **併せて §2.6・§11.5 への補足**: façade の利用者として本書は `benches/` しか
> 挙げていないが、**`examples/` も同格の利用者**である。各 Phase の完了確認では
> `cargo build --examples` も対象に含めること。

doc コメントによれば、`domain::semantics` が実コードのどれだけを解決できるかを
**計測するための内部ツール**である。移行時に 3 つの選択肢がある:

| 選択肢 | 評価 |
| --- | --- |
| `packages/core/semantics` に同梱する | 計測対象と同居する。妥当 |
| `benches/` に移す | 「計測」という性質に最も合う。criterion 化するかは別途 |
| 削除する | 参照ゼロだが、766 行の設計意図がある。**独断で消さない** |

**Phase 2（`core/semantics` 切り出し）の前に判断が必要。** 未決事項として §8 に追加する。

### 11.4 移行スクリプトを用意する

26 パッケージ × 手作業は事故る。Phase 1 のパイロットで**手順をスクリプト化**しておく。

```
scripts/extract-package.sh <kind> <name> <module...>
  1. packages/<kind>/<name>/{src,Cargo.toml,README.md の雛形} を作成
  2. git mv で指定モジュールを移送
  3. 移送ファイル内の crate::domain::<name> → crate::<name> を置換
  4. ルート mod.rs に pub use façade を追記
  5. ルート Cargo.toml に path 依存を追記
  6. git add -N packages/<kind>/<name>
```

**手順 3 の置換は必ずレビューする。** 文字列一致なので doc コメント内の
`crate::domain::` も書き換わる（§11.2 の観点ではむしろ望ましい）が、
テストフィクスチャ内の文字列を壊さないか確認すること。

### 11.5 その他の細かい確認事項

| 項目 | 内容 |
| --- | --- |
| `[profile]` | `lto = "fat"` / `codegen-units = 1` は **workspace root の `Cargo.toml` でのみ有効**。メンバー側に書いても無視される。移動させないこと |
| `release.yml` の version 抽出 | `sed -n 's/^version = "\([^"]*\)".*/\1/p' Cargo.toml \| head -n1` でタグと照合している。`[workspace.package]` に `version` を置くと**どちらが先に来るかで拾う値が変わる**。両方 `1.0.0` なら問題ないが、**片方だけ更新される事故を防ぐため契約テストで一致を検査する** |
| `.cargo/config.toml` | 現在存在しない。workspace 化後も不要 |
| `rustfmt.toml` | 現在存在せず、treefmt が `edition = "2024"` で rustfmt をかけている。26 パッケージでも設定は 1 箇所のままでよい |
| 未使用依存の検出 | 分割後、各パッケージが実際には使っていない依存を持つ可能性がある。`cargo-machete` または `cargo-udeps` を Phase 6 で 1 回流す |
| `benches/` の帰属 | `benches/{similarity_report,lint_report}.rs` は `paredit_cli::domain::...` を使う。façade が維持されるため**無改修で動く**が、Phase 6 で対応パッケージへ移すかを判断する |

### 11.5.1 【実装時の追記】§5.1 の割り当てには「合成ルートの部品」が混ざっている

Phase 1〜2 で **4 回続けて同じ原因**により、§5.1 が core に割り当てたモジュールが
実際には core に置けなかった。**共通する見分け方があるので、Phase 3 以降は
移送前にこれを確認すること。**

| モジュール | §5.1 の割り当て | 実際の帰属 | 理由 |
| --- | --- | --- | --- |
| `report_policy` | C5 | **ルート** | 3 つの feature が持つ policy 型の再エクスポートのみ |
| `system_order` | C3 | **ルート**（将来 F2） | `dependency_report` / `system_cycle_report` を呼ぶ |
| `lint_report` / `lint_suppression` | C5 | **F11** | registry 付きで engine を呼ぶ |
| `presentation::cli::contract` | C7 | **ルート** | 3 つの feature の `supports_*_dialect` を列挙 |

**判定基準**: そのモジュールが**複数の feature を名前で列挙・集約している**なら、
それは core ではなく**合成ルート**である。§4.2 が `REGISTRY` について述べている
論理がそのまま適用される — 列挙する側は列挙される側すべてに依存するため、
core に置くと core → feature の逆流になる。

行数や「層」ではなく、**依存の向き**で判定すること。`report_policy` は 7 行、
`contract.rs` は 521 行だが、どちらも同じ理由でルートに残る。

### 11.6 Phase 1 で判明した手順上の追加事項

以下はいずれも**本書に記載がなく、ゲートに落ちて初めて判明した**もの。
Phase 2 以降の各移送で必ず実施すること。

| # | 事項 | 症状 | 対処 |
| --- | --- | --- | --- |
| 1 | **`[workspace] default-members` が必須** | ルートパッケージを持つ workspace では素の `cargo nextest run` が**ルートしかビルドしない**。Phase 1 では新パッケージの 272 テストが**警告もなく消え、結果は緑のまま**だった。`flake.nix` は `--workspace` を付けないので **CI が黙ってテストを回さなくなる** | `default-members = [".", "packages/*/*"]` を宣言。各 Phase で `cargo nextest list` の**総数**を必ず確認する |
| 2 | **doctest はルートクレートを参照できない** | 移送ファイル内の `/// use paredit_cli::…` は移送先クレートからは解決不能。さらに **`compile_fail` doctest は「別の理由でコンパイルに失敗する」ため通り続け、何も検証しなくなる**（Phase 1 で 3 本該当） | 移送時に `paredit_cli::` → `<移送先クレート>::` へ一括置換し、`cargo test --doc -p <pkg>` で**実行本数**を確認する |
| 3 | **契約テストが移送対象ソースをフィクスチャとして読む** | §2.6 は `public_module_docs_contract.rs` が無改修で通るとするが、それは `src/domain/mod.rs`（façade）についてのみ正しい。**個別モジュールのパスは移送で壊れる**（Phase 1 で 3 テスト / 18 パスリテラル） | 移送前に `grep -rn '"src/domain/' tests/` でパスリテラルを洗い出す |
| 4 | **可視性の拡大が昇格済み lint を再発火させる** | `pub(crate)` → `pub` により `must_use_candidate` 等が新たに公開された項目に適用される（Phase 1 で 104 件）。`missing_debug_implementations` も同様 | 移送直後に `cargo clippy --fix -p <pkg>` を回す。**`--fix` はワークスペース全体指定だとメンバーを処理しないことがあるので `-p` を明示する** |
| 5 | **`cargo clippy --fix` はフォーマットしない** | `const ` 挿入で幅超過、`format!` 引数インライン化で行が詰まる。**`treefmt-check` でしか検出できず、判明まで約 15 分かかる** | `clippy --fix` の直後に必ず `nix fmt` |

### 11.6.1 【実装時の追記】feature 移送は core 移送より高くつく

Phase 3〜4 で判明。core パッケージはほぼ自己完結していたが、**feature の
`cli` 側は `src/presentation/cli.rs` の「暗黙のスコープ」に依存している。**

`cli.rs` は次を提供しており、各 feature の `cli` ファイルは**一切 import せずに**
使っていた:

| 提供元 | 内容 | 個数 |
| --- | --- | ---: |
| `use args::*;` | `DialectArg`, `OutputFormat`, `MoveInsert` 等の値 enum | 12 |
| `pub(crate) use shared::{...}` | `read_input_and_dialect`, `write_file_with_rollback` 等 | 17 |
| `macro_rules! safe_text`（テキストスコープ） | 端末安全レンダリング | 1 |
| `cli.rs` 自身の `use` | `Result`, `Args`, `json!`, `PathBuf`, `SyntaxTree`, `Dialect` 等 | 19 |

**合計 49 個。** クレート境界を越えると全て明示が必要になる
（F6 で 16 箇所、F8 で 47 箇所）。`scripts/rewrite-feature-package.py` が
この 49 名を把握し、**実際に使われているファイルにだけ** import を追加する。
残り 10 feature を手作業でやると事故る。

**併せて 2 つの落とし穴**:

1. **`X.rs` と `X/` が両方存在する**（Rust 2018 スタイル。ファイルがモジュール
   ルート、ディレクトリが子）。**ディレクトリだけ `git mv` するとルートが残り、
   パッケージが解決できない。** 実測で **18 モジュール**が該当
   （domain 12 / application::usecase 6）。
2. **移送によりルート側の再エクスポートが孤立する。**
   `application::usecase::extract_shared` は 1 行の `pub(crate) use` で、
   利用者が extract feature と共に去った瞬間 `unused_imports` で
   `--deny warnings` に落ちた。

### 11.6.2 【実装時の追記】ゲート実行中にツリーを編集しない

`nix flake check` は**追跡ファイルのワーキングツリーを読む**（§11.6 の
`git add -N` の裏返し）。Phase 3 のゲートをバックグラウンドで回したまま
Phase 4 の編集を始めた結果、**ゲートが Phase 3 ではなく「Phase 3 + 作りかけの
Phase 4」を検証して落ちた**。1 回 40 分（実測 35〜40 分）
を無駄にする。**ゲート中は待つか、別 worktree で回すこと。**

補足: `pub(crate)` の一括 `pub` 化は**必須**である。移送後は `crate` が
パッケージを指すため、ルートが使っている項目が**パッケージ内から見ると
dead_code になる**（Phase 1 では 101 件）。個別に絞るのではなく一括で広げ、
公開範囲の制御は **façade 側の再エクスポートの可視性**で行う
（Phase 1 では 5 モジュールを `pub`、4 モジュールを `pub(crate)` で再エクスポートし、
ルートクレートの公開 API を維持した）。

---

## 12. テスト構造の見直し

### 12.1 実測

```
cargo nextest run --no-fail-fast
→ Summary [ 166.175s ] 5627 tests run: 5627 passed (5 slow), 1 skipped
```

| 項目 | 実測 |
| --- | --- |
| テスト総数 | **5,627** |
| 実行時間 | **166.2 s** |
| テストバイナリ数 | **1**（`tests/cli.rs` のみ。235 個の `#[path]` mod を内包） |
| テストコード行数 | 50,492 |
| `slow` 判定されたテスト | **5 本**（いずれも ~102 s） |
| CLI プロセスを起動する proptest | 19 ファイル / 16 関数 |
| `src` 内のインプロセス proptest | 44 箇所 |

**テスト実行 166 秒は、crane が削る 88 秒（§1.3）より大きい単一のコストである。**
移行の前後どちらでも効くので、独立した改善項目として扱う。

### 12.2 最優先: `function_parameter` の proptest 5 本が設定漏れ

このリポジトリの CLI プロパティテストは、`tests/cli.rs:485` の
`cli_proptest_config(cases)` でケース数を明示する規約になっている。

```rust
// tests/cli/rename/function/mod.rs ほか 14 ファイル
#![proptest_config(cli_proptest_config(24))]
// tests/cli/sort_definitions.rs ほか
#![proptest_config(cli_proptest_config(12))]
```

**しかし `tests/cli/function_parameter/*/property.rs` の 5 ファイルだけ
`proptest_config` を書いていない。** その結果 proptest のデフォルト
**256 ケース**が適用され、各ケースが `paredit` プロセスを起動している。

```
tests/cli/function_parameter/add/property.rs           → 設定なし（= 256 cases）
tests/cli/function_parameter/move_parameter/property.rs → 設定なし
tests/cli/function_parameter/remove/property.rs        → 設定なし
tests/cli/function_parameter/reorder/property.rs       → 設定なし
tests/cli/function_parameter/swap/property.rs          → 設定なし
```

**この 5 本が `slow` 判定された 5 本と完全に一致し、テストスイートのクリティカルパスを決めている。**

#### 対応の選択肢

| 案 | 内容 | 評価 |
| --- | --- | --- |
| A | 他 14 ファイルと同じく `cli_proptest_config(24)` を付ける | **即座に効く。5 行の変更。** ただし探索ケース数が 256→24 に減る |
| B | プロパティを**インプロセス化**して feature クレートへ移す（§12.4） | ケース数を減らさずに高速化できる。**パッケージ分割が可能にする** |
| C | 現状維持 | この 5 本だけ 10 倍のケース数を回す理由が説明できるなら妥当 |

**推奨は A を即座に適用し、Phase 4 の `feature/function-parameter` 移送時に B へ移す。**

> **注意**: 直近のコミット `b26123b test: record a new proptest shrink for the add-parameter property`
> が示すとおり、この 5 本は**実際に不具合を見つけている**。ケース数を減らす判断は
> 探索能力を落とす判断でもある。だからこそ B（インプロセス化してケース数を維持）が本命であり、
> A はその前の暫定措置と位置づける。
> `tests/cli/function_parameter/*/property.proptest-regressions` に記録済みの
> シュリンク結果は、どの案でも**必ず引き継ぐこと**。

### 12.3 テストバイナリの分割

Cargo は `tests/*.rs` の**各トップレベルファイルを別々のテストバイナリ**としてビルドし、
それらを**並列にコンパイル・リンク**する。

現状は `tests/cli.rs` 1 本に 50,492 行が集約されており、
**この並列性をまったく使えていない**。cold の `cargo test --no-run` が
`cargo check` の後にさらに 58.3 秒かかる（§2.3）のはここが効いている。

```
現状:  tests/cli.rs (235 mods, 50,492 行)          → 1 バイナリ・直列
分割後: tests/inspect.rs / tests/edit.rs /
        tests/refactor.rs / tests/lint.rs /
        tests/contract.rs ...                       → N バイナリ・並列
```

パッケージ分割（§5）と**同じ軸で割れる**ため、Phase 4〜5 と同時に進められる。

**注意点**:

- 各バイナリが `paredit_cli` を個別にリンクするため、**総 CPU 時間は増える**。
  改善するのは wall-clock のみ。コア数の少ない CI ランナーでは効果が薄い
- `tests/cli.rs` の共通ヘルパー（`fresh_temp_dir` / `cli_proptest_config` 等 593 行）を
  `tests/common/mod.rs` に括り出す必要がある
- **`tests/cli.rs` の mod 宣言は 2 行 1 組**（`#[path = "cli/xxx.rs"]` + `mod xxx;`）である。
  分割スクリプトはこの 2 行ペアを単位に動かすこと。1 行だけ動かすと静かに壊れる
- `nextest` はもともとテスト**実行**を並列化しているので、**実行時間 166 秒は縮まない**。
  縮むのはビルド時間だけ

**この項目は §12.2 より優先度が低い。** 5 行の設定追加で 100 秒縮む前者に対し、
こちらは構造変更で 58 秒のビルド時間の一部を並列化するにすぎない。

### 12.4 プロパティテストのインプロセス化

CLI プロパティテスト 16 本が検証しているのは、大半が
**「出力が再びパースできる」というドメイン不変条件**である。

```rust
// tests/cli/function_parameter/swap/property.rs
fn pbt_cli_swap_function_parameters_output_remains_parseable(...)
    // fs::write でフィクスチャを作り、paredit プロセスを起動し、終了コードを見る
```

これは**ドメインの性質であって CLI の性質ではない**。にもかかわらず
1 ケースごとにプロセス起動 + ファイル I/O を払っている。

パッケージ分割後は `packages/feature/function-parameter/src/<slice>/domain.rs` の
ユニットテストとして**インプロセスで**書ける。`src` 側には既に 44 箇所の
インプロセス proptest があり、パターンは確立している。

| | 現状（CLI proptest） | インプロセス化後 |
| --- | --- | --- |
| 1 ケースのコスト | プロセス起動 + fs I/O | 関数呼び出し |
| 実行できるケース数 | 12〜24（コスト制約） | 256〜1000 |
| 検証できるもの | CLI 引数解析・exit code・ファイル書き込みを含む全経路 | ドメイン不変条件のみ |

**CLI 経路そのものの検証は捨てないこと。** 各操作につき
「代表的な 1 ケースを CLI 経由で流す例示テスト」は残し、
**網羅的な探索だけをインプロセスへ移す**のが正しい分業である。

---

### 12.5 【実施後の追記】§12.3 の前提は分割によって消えた

§12.3 は「`tests/cli.rs` 1 本に 50,492 行が集約されており、cold の
`cargo test --no-run` が `cargo check` の後にさらに **58.3 秒**かかる」ことを
根拠に、テストバイナリの分割を提案している。

**パッケージ分割後に実測すると 5.2 秒である。**

```
cargo clean -p paredit-cli
cargo test --no-run -p paredit-cli
→ Finished `test` profile in 5.17s
```

理由は単純で、テストバイナリはもう 209k 行の単一クレートを再コンパイルしていない。
24 個のビルド済み rlib をリンクしているだけである。
**§12.3 が解こうとしていた問題は、§6 のパッケージ分割が既に解いていた。**

いま分割すると、§12.3 自身が挙げた欠点だけが残る:

- 各バイナリが `paredit_cli` を個別にリンクするため**総 CPU 時間は増える**
- 縮むはずの wall-clock は既に 5.2 秒しかない
- 472 個の `#[path]` + `mod` の 2 行ペアを動かす作業で、
  **1 行だけ動かすと静かにテストが消える**（§12.3 自身の注意書き）

したがって **§12.3 は実施しない**。これは §5.1・§9.4・§11.3 と同じく、
実測が仕様の前提を否定した項目である。

### 12.6 【実施後の追記】§12.4 の実施内容

インプロセス化は完了した。ただし §12.4 が想定していた形とは 1 点違う。

| | §12.4 の想定 | 実施 |
| --- | --- | --- |
| インプロセス proptest | 256〜1000 ケース | **256 ケース**（proptest 既定）、5 操作すべて |
| 探索する入力の形 | （明示なし） | **CLI proptest と同一**（2〜3 パラメータ + `--all-calls`） |
| CLI proptest | 代表 1 ケースの例示テストに置換 | **24 ケースのまま残置** |

CLI proptest を残したのは、§12.2 の A を適用した後の実測が
**5 本合計 4.3 秒**だったからである。§12.4 が置換を勧めたのは
1 本 102 秒だった時点の判断であり、その前提はもう成り立たない。
4 秒のために CLI 経路（引数解析・exit code・ファイル書き込み・
`assert_cli_check_succeeds`）のプロパティ検証を捨てる取引は割に合わない。
なお各操作には既に 12〜20 本の CLI 例示テストがあり、
§12.4 の「代表 1 ケースは残すこと」は元から満たされている。

#### 記録済みシュリンクの引き継ぎ方

§12.2 は「記録済みのシュリンク結果はどの案でも**必ず引き継ぐこと**」と書いている。
**`.proptest-regressions` ファイルは引き継げない。** あれはシード値で記録されており、
シードは「それを生成したテストと入力の形」に対してしか意味を持たない。
インプロセステストは形が違うので、シードを再生しても別の入力を探索するだけで
何も証明しない。

忠実な引き継ぎは**シュリンクした値そのものを明示的にアサートすること**である。
`packages/feature/function-parameter/src/function_parameter/domain/tests/recorded_shrinks.rs`
が 3 本の記録済み入力をそのまま検証している。

またこの作業で、インプロセス proptest が **CLI proptest より狭い形しか
探索していなかった**ことが判明した（`add` は 1 パラメータ・明示 call path、
CLI は 2 パラメータ・`--all-calls`）。記録済みシュリンクはすべて後者の形から
出ており、**インプロセス側の探索範囲の外にあった**。形を揃えて解消済み。

---

## 13. リポジトリ運用上やっておくこと

### 13.1 `git mv` と内容変更を別コミットに分ける

約 2,000 ファイルを移動する（§7.1）。Git はリネームを**内容の類似度で推定**しているため、
**移動と内容変更を同一コミットで行うとリネーム検出が外れ、`git log --follow` と
`git blame` の履歴が切れる。**

209k 行のコードベースで履歴を失うのは実質的に取り返しがつかない。

```
コミット 1: git mv のみ（内容は 1 バイトも変えない） ← リネーム検出が確実に効く
コミット 2: パス書き換え・可視性修正・README 追加
```

**コミット 1 の時点ではコンパイルが通らない。** これは許容する。
PR 単位（= Phase 単位）で通ればよく、§6 の「Phase 終了時点で `nix flake check` が通る」
という条件はコミット単位ではなく Phase 単位の要求である。

検証:

```
git log --follow --oneline packages/core/sexpr/src/sexpr/parser.rs | wc -l
# 移行前の src/domain/sexpr/parser.rs の履歴本数と一致すること
```

### 13.2 `.git-blame-ignore-revs` を用意する

Phase −1 の機械的 lint 修正（約 1,900 件）は、**ほぼ全ファイルの `git blame` を汚す。**
`must_use` 属性の追加や `const fn` 化は「そのコードを書いた人」を書き換えてしまう。

現在このファイルは存在しない。Phase −1 の開始と同時に作成する。

```
# .git-blame-ignore-revs
# Phase -1: mechanical clippy fixes (must_use / const fn / formatting).
# No behavioural change; see SPEC-package-by-feature.md §9.5.
<commit-sha>
```

GitHub の blame ビューはこのファイルを自動的に尊重する。ローカルでは:

```
git config blame.ignoreRevsFile .git-blame-ignore-revs
```

**このリポジトリでは `git mv` のコミット（§13.1）も対象に含める。**
`docs/src/contributing.md` に `git config` の 1 行を書いておくこと。

### 13.3 公開 API のスナップショットを取る

§7.1 で挙げたとおり、移行では `pub(crate)` / `pub(in ...)` 合わせて
**約 2,500 箇所が `pub` へ格上げされる可能性**がある。
これは意図せず公開 API を膨張させる。

`crate_metadata_contract.rs` は README が
「A typed Rust library API behind the CLI」と謳っている限り
公開ライブラリ API の契約を検査している。つまり**このリポジトリは
公開 API を意図的に管理している**。

移行前にスナップショットを取り、各 Phase で差分を確認する:

```
cargo public-api > /tmp/public-api-before.txt      # 要 cargo-public-api
# または
cargo doc --no-deps && 生成された HTML の項目一覧を保存
```

**目標は「差分ゼロ」ではない**（パッケージが増える以上、増えるのが正しい）。
目標は**差分をレビューすること**。「なぜこれが公開になったか」を
各パッケージの README `## 公開している型・関数`（§3.3）で説明できない項目は、
`pub` にすべきでなかった項目である。

### 13.4 依存の重複を確認する

```
cargo tree --duplicates
```

現状の重複:

| パッケージ | 重複バージョン | 由来 |
| --- | --- | --- |
| `getrandom` | v0.3.4 / v0.4.3 | dev-dependency（`proptest` 経由と `tempfile` 経由） |
| `io-lifetimes` | v2.0.4 / v3.0.1 | `cap-std` の内部ツリー。**こちらでは制御できない** |

いずれも実害は小さく、**本移行で対処すべきものではない**。
ただし §9.3 の `[workspace.dependencies]` を導入したら、
26 パッケージ間で直接依存のバージョンが分岐していないことを
`cargo tree --duplicates` で定期確認する運用にする。

直接依存 36 行に対して `Cargo.lock` は 157 パッケージ。
実行時依存は 8 個（§2.5.5）なので、残りは dev-dependency（criterion / proptest）由来である。

### 13.5 その他

| 項目 | 状態 | 判断 |
| --- | --- | --- |
| `.github/CODEOWNERS` | 存在しない | 26 パッケージになってもレビュアーが増えるわけではない。**不要** |
| `rustfmt.toml` | 存在しない（treefmt が edition 2024 で実行） | 設定は 1 箇所のままでよい。**変更不要** |
| `.cargo/config.toml` | 存在しない | workspace 化後も不要 |
| `skills/paredit-cli/SKILL.md` | 契約テストが読む（§2.5.2） | CLI の使い方を書いたもので内部構造に触れていない。**移行での更新は不要**。ただし fileset には含めること |
| `benches/` | 2 本、`paredit_cli::domain::...` を使用 | façade で無改修動作。§11.5 |

---

## 14. 優先順位の要約

**費用対効果の高い順**。上 3 つは移行の採否と無関係に、単独で実施できる。

| # | 施策 | 効果 | コスト | 節 |
| --- | --- | --- | --- | --- |
| 1 | `function_parameter` proptest 5 本に `cli_proptest_config` を付ける | **テスト実行 ~100 秒短縮** | **5 行** | §12.2 |
| 2 | crane 導入（`buildDepsOnly`） | `nix flake check` ~88 秒短縮（≒9%） | `flake.nix` のみ・revert 容易 | §1.3 / Phase 2.5 |
| 3 | 機械的 lint 修正 | 品質。約 1,900 件 | `--fix` 中心。PR 分割が必要 | §9.5 / Phase −1 |
| 4 | **パッケージ分割**（本体） | 変更の局所性・境界の強制 | **最大**。§7 参照 | §1.1 / Phase 0〜6 |
| 5 | `anyhow` → `thiserror` | エラーの網羅性検査・exit code の型安全 | 430 ファイル。4 と同時が最安 | §9.2 |
| 6 | 型設計の強化（bool / 数値） | 正しさ（精度欠落 17 + f64 比較 9） | 設計判断を要する | §9.4 / Phase 7 |
| 7 | テストバイナリ分割 | ビルド wall-clock の一部 | 構造変更 | §12.3 |
| 8 | crane のレイヤー化キャッシュ（core / feature） | warm ビルドで追加 ~39 秒短縮 | 4 完了後。cold は悪化するので要計測 | §2.5.5 |

> **1 を最初にやること。** 5 行で 100 秒縮み、他のどの作業とも衝突せず、
> 以降すべての Phase の検証ループが速くなる。
