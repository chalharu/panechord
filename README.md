# Panechord

宣言的 UI フレームワーク。YAML で画面を宣言し、共有状態 `Store` と型付き `Binding` /
`Command` で backend (GTK / Tao / AppKit) を駆動する。公開入口は `panechord`
facade で、runtime と宣言マクロを再 export する。`native`・`gtk`・`tao`・`macos`
feature で使う backend を選ぶ。既定では backend をロードしない。

## ワークスペース構成

| クレート | 役割 |
|---|---|
| `panechord` (root) | 公開 facade。runtime + macros を再 export |
| `runtime/` | 共有状態・Binding・Command・effect・終了契約 |
| `markup/` | schema-v2 YAML/JSON の strict parser |
| `macros/` | `view!` / `owned_view!` / `#[commands]` のコード生成 |
| `native/` | 実行 OS 既定 backend の選択 |
| `gtk/` | GTK4 backend (Linux / macOS global-menu mirror) |
| `tao/` | Tao + software iced 部品 backend (Linux / macOS / Windows) |
| `macos/` | AppKit backend (macOS)。Swift bridge を内部 build |
| `demo/` | 回帰 gallery と最小例 |
| `library/` | フォルダ走査の一覧画面 (第2の実用画面) |

構文と生成規則は `macros/README.md`、解析制限は `markup/README.md`、
所有権・上限・終了契約は `runtime/README.md`、API 全体契約は `API.md`、
対応範囲と検証手順は `HARDENING.md`、backend 差は `gtk/`・`tao/`・`macos/` の
README を参照。

## 動作環境

- Rust 1.95 以上。
- Linux: GTK4 開発環境 (Tao backend は GTK3 側の bridge を使う)。
- macOS: Xcode Command Line Tools の Swift。
- Windows: Tao backend のみ (`backend-tao`)。実機検証は Windows 機で行う。

## まず動かす

リポジトリ root から実行する。

```sh
cargo run --locked -p panechord-demo --example minimal
cargo run --locked -p panechord-demo
cargo run --locked -p panechord-demo --no-default-features --features backend-tao
cargo run --locked -p panechord-library -- --folder /path/to/files
cargo run --locked -p panechord-library --no-default-features --features backend-tao -- --folder /path/to/files
```

最小例は `demo/examples/minimal.rs` と `demo/ui/minimal.yaml`。
操作 probe は `cargo run --locked -p panechord-demo --example controls`。
対話確認は `PANECHORD_CONTROLS_REVIEW=1` を付け、backend-tao も同じ feature 指定で選ぶ。
回帰 gallery は `demo/ui/gallery.yaml` で、全 Layout と Template、1000件の項目 Model を
使う。表示する行だけを mount し、画面外へ出てもモデルの編集状態は保持する。
保存先は一時ディレクトリの `panechord-demo-<pid>.txt`。API キーや `.env` は不要。

`Clone` な状態型と `#[commands]` メソッドを書き、`Store` を作り、共有前に
`try_into_owner` で `StoreOwner` を取得して `owned_view!` へ渡す。
`Store` を直接渡すコードはコンパイルエラーになる。
共有するモデルには `view!` を使う。共有 `View` の終了は他の所有者の `Store` を閉じない。
`owned_view!` はその `View` をモデルの終了責任者とする明示的な選択である。
画面接続と `Command` の定型コードはマクロが生成し、field・型・署名は Rust が検査する。
YAML 変更を `include_str!` で追跡し、実行時に定義を再解析しない。

## 検証

```sh
sh scripts/verify.sh            # 整形・Clippy・core/macro 試験・両 backend native 試験
sh scripts/verify-docker.sh     # Debian/bookworm + Rust + GTK4/GTK3 + Xvfb の Linux 正式ゲート
sh scripts/verify-macos.sh      # macOS 対話デスクトップ上の AppKit/Tao ゲート
sh scripts/measure.sh           # release CPU 予算の観測
```

`verify.sh` は終了コード 0 と `VERIFY3 PASS` で成功とする。
GTK 試験は初期化スレッド制約から一件ずつ別プロセスで実行する。
`GDK_BACKEND=x11` を明示し、継承 Wayland display が Xvfb を迂回しないようにする。
clipboard probe は `PANECHORD_ALLOW_CLIPBOARD_PROBE=1` が必要で、
他の copy/paste 操作と競合しない専用デスクトップで実行する。
YAML 診断は `sh scripts/check-yaml.sh check ...` で有効化する (Python 3)。
typo・型不一致・未知 command・参照シグネチャ不一致が YAML 行列を primary span に持つ。
stable proc-macro 単体では外部ファイル span を作れないため wrapper が必要で、LSP ではない。

Docker が利用可能なら `sh scripts/verify-docker.sh` で検証 image を構築して
同じ Linux 正式ゲートと release 性能ゲートを実行できる。host の target は共有せず、
専用 Docker volume を使う。Linux/X11 の証拠であり、macOS 実 IME・VoiceOver や
Wayland 実機の証拠とはしない。

macOS では対話デスクトップ上で `sh scripts/verify-macos.sh` を使う。
`python3 scripts/bundle-macos.py --verify` で AppKit の配布 App を生成し、
relocate 後の smoke を検証する。Developer ID 署名・notarization は対象外。

## 境界を明示する

- `view!` はアプリ型をホスト側で実行しない。任意 typetag 拡張 crate の自動ロード方式ではなく、
  明示 factory path を生成し、呼び出し側の Rust 型検査へ渡す。
- Template は対応する Control の任意合成。nested Window、循環 Template は拒否する。
  項目 key は空文字・`/`・重複を拒否。仮想化は固定行高で、可変行高は未対応。
- `Command` と動的 Menu 引数は Clone + 'static な型を利用できる。generic モデル、
  readonly field 属性、完全な source map / editor LSP は未対応。
- 入力途中状態はアプリ側の Draft で保持するのが定石。汎用 converter の失敗時は
  従来どおり正規値へ戻る。
- 任意 closure 更新時の `S` の Clone と全購読の射影・比較は残る。`snapshot()` も所有 Clone を返す。
  clone-free 置換と field 別 index による購読省略は scoped API の上級契約である。
  詳細は `runtime/README.md`。
- async validation はモデルへの反映後検証。古い結果は捨て、退去時に中断する。
- system clipboard は実 display へ接続する。Wayland は seat/focus 条件が別途必要。
- 通常終了は要求経路、`close()` は明示的な強制終了。外部 destroy / process 終了は
  graceful shutdown ではない。
- Linux/X11 での実証を Windows・macOS・Wayland 実機対応の証拠にしない。
  1000件の動作は性能保証でもない。未検証事項は `HARDENING.md` へ記録する。

## ライセンス

MPL-2.0 (`LICENSE` を参照)。
