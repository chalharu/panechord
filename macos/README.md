# macOS AppKit Backend

macOS 向け backend。Swift の AppKit 部品を薄い C ABI 経由で共通 runtime・proc macro へ
接続する。Tao/iced の置換実装ではなく、NSWindow / NSTextField / NSButton / NSMenu を使う。

## 起動

macOS の対話デスクトップ、Rust 1.95 以上、Xcode Command Line Tools の Swift が必要。
このリポジトリの root から、backend を一つだけ選んで実行する。

```sh
env -u DYLD_LIBRARY_PATH -u DYLD_FALLBACK_LIBRARY_PATH -u DYLD_INSERT_LIBRARIES \
  cargo run --locked -p panechord-library --no-default-features --features backend-macos -- --folder /path/to/files
sh scripts/verify-macos.sh
sh scripts/measure-macos.sh
```

`DYLD_LIBRARY_PATH=/opt/homebrew/lib` を継承すると、OS の ImageIO が Homebrew の
PNG codec へ解決され、native 画像取得中に SIGBUS となることを実機で確認した。
手順はこのプロセスの DYLD 探索・注入設定だけを除き、グローバル環境は変更しない。

アプリの build.rs や UI 契約 crate は不要。backend 内部の build.rs だけが Swift を
コンパイルする。Swift モード 5、SDK 付属の AppKit、追加 Swift package なし。
bridge の install name は `@rpath/libPanechordAppKit.dylib`。
`python3 scripts/bundle-macos.py --verify` でアプリ・bridge・相対 rpath を bundle へ配置し、
ad-hoc codesign と再配置後の smoke を検証する。Library は `--package panechord-library`
を指定する。Cargo の単体 example は Cargo の loader 設定を使うため、binary だけの
コピーは配布手順ではない。Developer ID 署名・notarization は未認証で、ad-hoc 署名の
成功から推定しない。deployment target の既定は 11.0 だが、最低 OS や Intel での
実機動作を確認した意味ではない。

## 契約

- 主な対象は demo と一覧の二画面。Stack、Label、TextBox、Button、固定高 ItemsControl、Menu。
  CheckBox、Radio、Slider、Combo、Image、Tab、Dialog と保持型 visible/enabled/Style Binding、
  typed submenu も共通契約 (`API.md`) で検証する。
  Grid/Flex/Absolute は実行時エラー。ContentControl/custom meter は API 未実装でコンパイル不可。
  GTK/Tao 全 gallery との完全な機能互換は主張しない。
- main thread の `MacHost` が共通 loop を駆動する。UiHost/Session へ接続し、複数 Window を保持する。
  `run_host` と `register_view(session,id,view,hooks)` を使い、終了所有権は明示 hook へ置く。
  Window close は個別、app Quit は Session 全体。共通 delegate と active Window 別 menu を使う。
  1 host につき Session は一つ。同じ Session の clone の再 attach は冪等、別 Session の attach と
  窓登録は副作用前に拒否する。`register_view` は最初の Session を自動 attach する。
  closed View は attach 前に拒否し、Session や窓登録を変更しない。
  active 窓を閉じたら生存窓へ menu と Quit を引き継ぐ。再 `present()` は不要。
  全窓の確認には期限を置かず、承認後の drain だけを 5 秒に制限する。
  `run` は既存単窓の互換入口。外部 reactor との統合は別途必要。
- DialogFooter は Cancel 先行の右端 row で、destructive な accept には
  `NSButton.hasDestructiveAction` を設定する (macOS 11 以降)。
  Return/Escape の配送は dialog 所有者が持ち、backend は見た目の順序と強調だけを持つ。
- TextBox は AppKit field editor を利用し、選択、undo、clipboard、IME marked text を OS へ委ねる。
  marked text 中はモデル通知による stringValue 置換を抑止する。これは汎用編集 transaction や
  外部モデル更新との競合解決を保証するものではない。
- caption が `<field_id>_caption` ならその文字列を AX label へ渡す。
  入力は AXTextField、部品には安定した accessibility identifier を設定する。
  宣言順の key-view loop を作り、動的 node 削除時に参照も更新する。
- Cmd+S 等は共通 Shortcut から native key equivalent へ変換する。
  Edit menu は native responder chain へ接続し、WM close / Cmd+Q は終了要求へ接続する。
  動的 Menu は全 ID・shortcut の検証後に native 表示と handler を交換する。不正更新は旧 Menu を保持し、
  `menu_error(id)` で報告する。正常更新でその Menu のエラーを解除し、host は終了しない。
- View は購読 token を持ち、callback 側は WeakView を使う。強制 close は冪等。
- 一覧は全 key を検査し、表示範囲の行だけ mount する。同じ key/Store の可視行は移動して保持する。
  conformance では field editor identity、marked text、selection を reorder 時に保持することを検査する。
  固定行高、viewport count、native scroller を使用する。編集行 probe は 56px で、template 全体が収まる必要がある。
  count は 1..256、行高は有限な 1..4096 で、共通 runtime と静的 literal が同じ条件を検査する。
  初期不正なら mount せず、不正更新は前の正常表示を保持する。正常値への復帰と末尾ページを native 検査する。
  readonly/literal でも受理済み viewport をローカルに移動し、モデルへの書込は TwoWay だけで行う。
  `item_keys` は全 source でなく mount 済み key を返す。行高変更は既存 mount にも反映する。
  行の退避では入れ子一覧の registry、viewport handler、scoped token も解放する。
  Template 中の `keep` も行 scope へ保持し、退去で validation を中断して pending を解除する。
  画面外の行変更は validation を開始せず、再 mount による購読の蓄積を防ぐ。
  不正な source 更新後も、保持した正常 source でスクロールできる。
- control ID と内部 event は ABI の種別で分離する。`__close`、`rows@viewport`、`__title` も
  通常 control として扱い、終了・scroll・title 操作へ誤配送しない。
- padding/margin は native frame inset へ合算する限定契約。任意の placement、CSS 互換、
  backend 間の pixel 一致は対象外。幅 640px 以上を対象とし、縦方向は native scroll を使う。

## 公平性

macOS 版の両アプリは `Store::with_dispatch_budget(..., NonZeroUsize(64))` を選択する。
既定の `Store::new` / `with_limits` は同期契約を維持する。
予算付き Store は 1 回の drain で指定件数を処理した後、Weak な継続 future へ渡す。
FIFO、容量制限、watch の coalescing、close による未実行処理の破棄は維持する。
受付済みの書き込みが呼出しから戻る時点で反映済みとは限らない。

ホストは native イベントを 1 件配送してから最大 32 future を 1 回ずつ poll する。
新規 future は次ターンへ回し、Pending は wake まで poll しない。
ready task がなければ native 側で最大 8ms 待ち、worker の wake も次ターンで確認する。
一つの callback / Clone / projection / poll は分割・中断できないため、時間上限や
リアルタイム性の保証ではない。1 回の poll が長ければ OS イベント処理も遅れる。

## 検証範囲

| 項目 | 自動検証の範囲 |
|---|---|
| demo / 一覧 | 共通 smoke: Draft 保持、readonly、保存、終了 veto、保存待ち、再確認、最終 JSON / sandbox で編集 burst、Latest 結果、失敗復帰、選択、終了 |
| 入力 | field editor へ marked text と日本語 commit を注入、実 OS clipboard で copy/cut/paste、Tab/逆 Tab |
| Menu | native key equivalent で Primary/Cmd+S、有効/無効、動的再構築 |
| 型付き Menu | `MenuItem<i32>` を String 変換なしで配送 |
| 三窓 | 共有 Store、補助窓の再生成、active 窓 close 後の menu 引き継ぎ、再 present なしの Quit 確認 hook 到達・拒否、confirm/cancel/drain/close 順序 |
| Session 制限 | 同一 clone の再 attach、別 Session と closed View の窓登録拒否、失敗後に別 Session を attach 可能 |
| validation scope | 退去/再 mount、pending 解除、画面外更新の無反応、購読の非蓄積 |
| Accessibility | role、identifier の読み返し。スクリーンリーダーによる操作ではない |
| 一覧 mount | 100 行 Model、6 行 mount、範囲変更、順序変更、不正 key 拒否と復帰 |
| Viewport 契約 | 初期不正は 0 mount、count/行高の範囲外を拒否して mount 保持、正常値復帰、usize::MAX の末尾ページ |
| readonly viewport | ローカル移動でモデルは不変、後続のモデル通知を反映。automation は native scroll と同じ handler を利用 |
| Menu 拒否 | 不正 ID・重複 ID・後続項目の不正 shortcut でも旧 handler/引数を保持し、正常候補へ復帰 |
| 入れ子 cleanup | 同じ key で mount/退避を繰り返し、registry 消去・古い scroll の無効化・テンプレート資源の弱参照失効 |
| 動的行高 | `frame_height` で実 native frame の変化を読み返し、テンプレート再生成なしを確認 |
| 過負荷 | 大量更新、合成キーを各連鎖完了前に配送、WeakView・close の検査 |
| 描画 | 640/960px の native bitmap、連鎖中の AppKit 描画 CPU 時間 |

conformance 例は main thread で実行する。libtest の worker thread から AppKit を初期化しない。
clipboard は試験前のデータへ復元する。画像は表示された専用 temp directory へ保存する。
CPU 描画は compositor presentation/fence ではなく、合成キーは物理入力ではない。
時間に合否閾値を置かず、入力が連鎖完了前に届くこと・状態・mount 数を assert する。
release 観測は `measure-macos.sh` で再現し、数値を安定 API や性能保証と扱わない。
CPU 回帰予算は別途 `sh scripts/measure.sh` で判定する。native 入力時間の閾値ではない。
GTK/Tao の native 試験は Linux 側の結果であり、macOS の試験数へ合算しない。

macOS 上の Tao も strict Clippy、実窓描画、system clipboard、gallery、demo/一覧 smoke を
main-thread の専用 process で実行する。`verify-macos.sh` は `verify-tao-macos.sh` も実行する。
Tao の close/menu 操作は API 注入で、実 OS の WM/accelerator 配送や外部 IME の適合とは区別する。
詳細は [../tao/README.md](../tao/README.md)。

## 手動ゲート (未実施)

以下は未実施。自動試験が成功しても適合済みと扱わない。

1. macOS 日本語 IME でかな入力、候補表示、かな漢字変換、確定・取消。保存中と連続検索中にも行う。
2. 変換中の別欄移動、別 app への切替、再フォーカス、Cmd 操作後の修飾キー残留を実キーで確認する。
3. Full Keyboard Access の有無で Tab/Shift+Tab、Space、Cmd+S/Q、未保存終了確認を操作する。
4. VoiceOver で欄名、値、readonly、ボタン状態、一覧、保存状態とエラーの到達性を確認する。
5. Instruments 等で実入力から表示までの遅延と compositor frame を確認する。

対象 OS と主 backend は macOS/AppKit、Linux/X11/GTK とするが、この macOS 実装は限定アルファ。
Windows、Wayland、外部 IME、VoiceOver、配布 package まで含む安定版ではない。
上記の実機ゲートを完了する前に API 固定は行わない。
