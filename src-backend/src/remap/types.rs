use serde::{Deserialize, Serialize};

/// デバイスのリマップ種別
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RemapKind {
    KeyMatrix,
    MouseButtons,
}

/// デバイスがサポートするリマップ機能メタデータ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemapCapabilities {
    pub kind: RemapKind,
    /// レイヤー数（マウス等の単一レイヤー時は 1）
    pub layer_count: u8,
    /// キーボードの行数（マウス時は None）
    pub rows: Option<u8>,
    /// キーボードの列数（マウス時は None）
    pub cols: Option<u8>,
    /// 現在の接続状態で書き込み可能か（例: NOVA KINE の 2.4G 時は false）
    pub writable: bool,
    /// 書き込み不可・制限がある場合のユーザー向け案内メッセージ
    pub notice: Option<String>,
    /// 選択可能なアクションカテゴリ一覧
    pub supported_categories: Vec<ActionCategory>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionCategory {
    Key,
    Shortcut,
    Mouse,
    Media,
    Special,
}

/// ハードウェア非依存の共通アクション表現
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RemapAction {
    /// 工場出荷時のデフォルト割当
    Default,
    /// 無効化 (KC_NO)
    Disabled,
    /// 単一キーまたは修飾キー付きショートカット
    Key {
        /// 正規化済みキー名（例: "A", "Enter", "F1", "Space"）
        key: String,
        /// 修飾キー一覧（例: ["Ctrl", "Shift"]）
        #[serde(default)]
        modifiers: Vec<String>,
    },
    /// マウスボタン・ホイール操作
    MouseButton {
        /// "left" | "right" | "middle" | "back" | "forward" | "wheel_up" | "wheel_down"
        button: String,
    },
    /// メディア・システム操作
    Media {
        /// "vol_up" | "vol_down" | "mute" | "play_pause" | "next" | "prev" | "stop"
        command: String,
    },
    /// デバイス固有・レイヤー機能
    Special {
        /// 例: "dpi_cycle", "dpi_up", "dpi_down", "mo_1", "mo_2", "tg_1"
        code: String,
    },
    /// 未定義・独自生コード（読み出し時のフォールバック保持用）
    Raw {
        /// 16進文字列表現（例: "0x5210"）
        hex: String,
    },
}

/// 1 キー / 1 ボタン分の割当エントリ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemapSlot {
    /// スロット識別子（例: "r0_c0", "btn_01", "hyzen_000102"）
    pub slot_id: String,
    /// UI 表示用ラベル（例: "Esc", "左クリック", "サイド戻る"）
    pub label: String,
    /// 行インデックス（キーボード用）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub row: Option<u8>,
    /// 列インデックス（キーボード用）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub col: Option<u8>,
    /// 現在割り当てられているアクション
    pub action: RemapAction,
}

/// GET /api/devices/{id}/remap のレスポンス
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemapState {
    pub device_id: String,
    pub layer: u8,
    pub capabilities: RemapCapabilities,
    pub slots: Vec<RemapSlot>,
}

/// PUT /api/devices/{id}/remap のリクエスト
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateRemapRequest {
    #[serde(default)]
    pub layer: u8,
    pub slot_id: String,
    pub action: RemapAction,
}

/// POST /api/devices/{id}/remap/reset のリクエスト
#[derive(Debug, Clone, Deserialize)]
pub struct ResetRemapRequest {
    #[serde(default)]
    pub layer: u8,
    /// None の場合は当該レイヤーの全スロットを初期値へ戻す
    pub slot_id: Option<String>,
}