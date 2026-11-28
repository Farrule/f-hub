use crate::model::{
    BatteryInfo, Capabilities, DeviceError, DeviceKind, DevicePresence, ProfileResult,
};
use hidapi::HidApi;

pub trait DeviceDriver: Send + Sync {
    /// CLI や API で指定する一意ID（例: "nape-pro"）
    fn id(&self) -> &'static str;
    /// 表示名（例: "Keychron Nape Pro"）
    fn name(&self) -> &'static str;
    /// デバイス種別
    fn kind(&self) -> DeviceKind;
    /// 対応機能フラグ（UI 側の表示制御にも流用）
    fn capabilities(&self) -> Capabilities;

    /// 接続検出（有線 / 2.4G / 未接続 と検出した VID:PID を返す）
    fn probe(&self, api: &HidApi) -> DevicePresence;

    /// バッテリー残量・充電状態の取得（未対応機はデフォルトで Unsupported）
    fn read_battery(&self, _api: &HidApi) -> Result<BatteryInfo, DeviceError> {
        Err(DeviceError::Unsupported)
    }

    /// プロファイル／レイヤー切替（未対応機はデフォルトで Unsupported）
    fn switch_profile(&self, _api: &HidApi, _slot: u8) -> Result<ProfileResult, DeviceError> {
        Err(DeviceError::Unsupported)
    }
    /// リマップ機能の対応状況を返す（未対応デバイスは None）
    fn remap_capabilities(&self) -> Option<crate::remap::RemapCapabilities> {
        None
    }

    /// 指定レイヤーの現在リマップ状態を読み出す
    fn read_remap(&self, _api: &hidapi::HidApi, _layer: u8,) -> Result<crate::remap::RemapState, DeviceError> {
        Err(DeviceError::Unsupported)
    }

    /// 指定スロットへリマップ割当を書き込む
    fn write_remap(
        &self,
        _api: &hidapi::HidApi,
        _layer: u8,
        _slot_id: &str,
        _action: &crate::remap::RemapAction,
    ) -> Result<(), DeviceError> {
        Err(DeviceError::Unsupported)
    }
}