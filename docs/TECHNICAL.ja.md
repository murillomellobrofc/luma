# 技術ドキュメント — Luma

このドキュメントは Luma のアーキテクチャ、プロジェクトのビルド方法、
貢献方法を説明します。

---

## アーキテクチャ概要

Luma はモジュール式に設計されています。単一の巨大バイナリの代わりに、
環境は互いに対話する独立した部分に分割されています:

- **コンポジター** — Wayland のコア、ウィンドウの描画、入力処理、
  画面管理を担当
- **luma-bar** — タスクバー
- **luma-menu** — アプリケーションメニュー
- **luma-applets** — system tray のコンポーネント

この分離により、各部分を他を壊さずに進化させ、将来的にコンポー
ネントを置き換える道を開きます。

---

## 技術スタック

- **言語:** Rust (edition 2024、MSRV 1.85)
- **グラフィックプロトコル:** Wayland
- **コンポジターフレームワーク:** Smithay
- **SVG レンダリング:** usvg、resvg、tiny-skia
- **フォント:** fontdue (BGRA premultiplied ラスタライズ)
- **検索:** fuzzy-matcher
- **画像:** image (PNG と JPEG)
- **設定:** toml + serde
- **D-Bus:** zbus 5 (blocking、サービスごとに専用スレッド)
- **通知オーディオ:** libcanberra を libloading 経由で実行時ロード
  (dlopen、ビルドに開発ヘッダーは不要)
- **PipeWire/PulseAudio クライアント:** `wpctl` (制御) と `paplay`
  (シンプルな再生) を spawn — `pipewire` crate への依存なし
- **その他:** chrono (日付/時刻)、meval (電卓)、
  tracing + tracing-subscriber (構造化ログ)、walkdir

Smithay はローカルフォーク (`vendor/smithay` 内) として使用、winit
バックエンドでのマルチウィンドウサポートのための独自パッチ付き。

### 通知システムのアーキテクチャ

`org.freedesktop.Notifications` 通知システムは専用スレッドで動作
する D-Bus サーバー (zbus blocking)。toast スタックの状態は
`Arc<Mutex<NotificationsState>>` に存在し、D-Bus スレッド (`Notify`
を受信) と main loop (レンダリングと入力処理) で共有。

`ActionInvoked` と `NotificationClosed` シグナルは、サーバースレッド
が `RequestName` 後に populate する `Arc<Mutex<Option<zbus::blocking
::Connection>>>` を介して main loop から emit。

オーディオ層は分離 (`src/sound.rs`) — 実行時に `libcanberra.so.0`
をロード、セッション全体で単一の `ca_context` を維持、内部 debounce
でラッシュが可聴ストロボになるのを防止。`.so` がインストールされて
いない場合、エンジンはサイレントに失敗、ビルドを壊さず実行時に
クラッシュもしない。

画面フラッシュ (visual bell) はオーディオと同じアラートゲート
(`alert_pending` エントリごと) を共有 — オーディオとフラッシュは
「アラートすべき」決定の独立した 2 つのコンシューマー。不透明度と
継続時間の制限は光感受性安全のために厳格 (WCAG 2.3.1 — 1 秒あたり
最大 3 フラッシュ)。

---

## コード設計の原則

### コードに固定色なし

色やビジュアル測定の 16 進値は Rust に直接書かれない。すべてテーマ
ファイル `theme.toml` から来る。これは Luma の外観を再コンパイル
なしで完全に変えられることを意味する。

### ルールとしてのアクセシビリティ

クリックターゲットは最小 40 ピクセル (PCD Max プリセットでは 48 以上)。
すべての機能はキーボードでアクセス可能でなければならない。コントラ
ストは WCAG AA ガイドラインに従い、コストなしで可能な場合は AAA
(通知パネルの不透明スクリムは、例えば、明るい壁紙の上でも AAA を
保証)。インターフェースのどの部分でも交渉不可。

### 小さく機能するコミット

プロジェクトの各コミットはビルドして実行される。作業は名前付き
ステップ (例: "etapa H33.taskbar.4") に分割され、各ステップは検証
可能なステップ。

---

## ビルド方法

### システム依存

#### Debian / Ubuntu

```
sudo apt install \
  build-essential pkg-config \
  libwayland-dev wayland-protocols \
  libxkbcommon-dev libinput-dev \
  libegl-dev libgbm-dev \
  libdrm-dev libseat-dev \
  libsystemd-dev libdbus-1-dev libpixman-1-dev \
  xwayland \
  fonts-dejavu-core fonts-liberation
```

system tray (ネットワーク、bluetooth、オーディオ) と通知音の実行時
ツール:

```
sudo apt install network-manager bluez pipewire wireplumber libcanberra0
```

`libcanberra0` はオプション — 不在の場合、通知音はサイレントに失敗、
何も壊さない。

#### Fedora

```
sudo dnf install \
  gcc pkgconf-pkg-config \
  wayland-devel wayland-protocols-devel \
  libxkbcommon-devel libinput-devel \
  mesa-libEGL-devel mesa-libgbm-devel \
  libdrm-devel libseat-devel \
  systemd-devel dbus-devel pixman-devel \
  xorg-x11-server-Xwayland \
  libcanberra
```

#### Arch Linux

```
sudo pacman -S --needed \
  base-devel pkgconf \
  wayland wayland-protocols \
  libxkbcommon libinput \
  mesa libdrm seatd \
  systemd dbus pixman \
  xorg-xwayland \
  ttf-dejavu ttf-liberation
```

実行時:

```
sudo pacman -S --needed networkmanager bluez bluez-utils pipewire wireplumber libcanberra
```

rustup 経由で Rust もインストールが必要。

### ビルド

```
git clone <repository>
cd luma
cargo build
```

Smithay フォークはリポジトリにすでに含まれているので、ビルドは追加
ステップなしで動作。実行:

```
cargo run
```

---

## フォルダ構造

```
src/                Rust ソースコード
  backends/         レンダリングと入力のバックエンド
  menu/             アプリケーションメニュー
  handlers/         Wayland プロトコルハンドラー
  input/            キーボード、ポインター、ショートカット
  notifications.rs  通知スタック状態 + D-Bus サーバー
  sound.rs          libcanberra ロード + 再生
  state.rs          LumaState (コンポジターのグローバル状態)
  theme.rs          ビジュアルトークン + luma.toml 読み込み
assets/             アイコン、サウンド、ビジュアル ID、スクリーンショット
vendor/             Smithay のローカルフォーク
docs/               このドキュメント
```

---

## テーマシステム

すべての外観は `~/.config/luma/theme.toml` で制御される。このファイル
は各コンポーネントの色、サイズ、間隔、動作を定義: タスクバー、メニュー、
ウィンドウ装飾、アプレット。

デフォルトパレットは Golden Hour:

- コーヒー — 暗い背景トーン
- コーラル — ハイライトと重要なアクション
- ハニー — 二次要素と輝き
- セージ — 確認とポジティブな状態

---

## 貢献方法

現在リポジトリは非公開で、開発は一人で行われている。バージョン 1.0
後にプロジェクトは公開され、このドキュメントは完全な貢献ガイドで
拡張される。

これより前にこれを読んでいる場合、おそらく身近なサークルの一員。
提案とレビューは歓迎。
