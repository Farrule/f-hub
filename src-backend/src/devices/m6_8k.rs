use crate::devices::driver::DeviceDriver;
use crate::model::{
    BatteryInfo, Capabilities, ChargingState, ConnectionMode, DeviceError, DeviceKind,
    DevicePresence,
};
use hidapi::{DeviceInfo, HidApi};

const VID: u16 = 0x3434;
const PID_WIRED: u16 = 0xd049;
const PID_24G: u16 = 0xd028;

pub struct KeychronM68k;

impl KeychronM68k {
    fn find_info<'a>(api: &'a HidApi) -> Option<(&'a DeviceInfo, ConnectionMode)> {
        // 有線接続中はドングル側が応答しないため有線 PID を優先
        if let Some(wired) = api.device_list().find(|d| {
            d.vendor_id() == VID
                && d.product_id() == PID_WIRED
                && d.interface_number() == 4
                && d.usage_page() == 0xffc1
        }) {
            return Some((wired, ConnectionMode::Wired));
        }

        api.device_list()
            .find(|d| {
                d.vendor_id() == VID
                    && d.product_id() == PID_24G
                    && d.interface_number() == 4
                    && d.usage_page() == 0xffc1
            })
            .map(|d| (d, ConnectionMode::Wireless24G))
    }
}

impl DeviceDriver for KeychronM68k {
    fn id(&self) -> &'static str {
        "m6-8k"
    }

    fn name(&self) -> &'static str {
        "Keychron M6 8K"
    }

    fn kind(&self) -> DeviceKind {
        DeviceKind::Mouse
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            battery: true,
            profile: true,
        }
    }

    fn probe(&self, api: &HidApi) -> DevicePresence {
        match Self::find_info(api) {
            Some((info, mode)) => DevicePresence {
                mode,
                vid: Some(info.vendor_id()),
                pid: Some(info.product_id()),
                product_name: info.product_string().map(ToOwned::to_owned),
            },
            None => DevicePresence::disconnected(),
        }
    }

    fn read_battery(&self, api: &HidApi) -> Result<BatteryInfo, DeviceError> {
        let (info, _) = Self::find_info(api).ok_or(DeviceError::Disconnected)?;
        let dev = info
            .open_device(api)
            .map_err(|e| DeviceError::Communication(format!("オープン失敗: {e}")))?;

        let mut pkt = [0u8; 64];
        pkt[0] = 0xb3; // Report ID 179
        pkt[1] = 0x06;
        pkt[2] = 0x00;

        dev.write(&pkt)
            .map_err(|e| DeviceError::Communication(format!("write失敗: {e}")))?;

        let mut buf = [0u8; 64];
        let n = dev
            .read_timeout(&mut buf, 500)
            .map_err(|e| DeviceError::Communication(format!("read失敗: {e}")))?;

        if n == 0 {
            return Err(DeviceError::Communication(
                "無応答（スリープ中または未接続）".to_string(),
            ));
        }
        if n >= 21 && buf[0] == 0xb4 && buf[1] == 0x06 {
            let raw = buf[20];
            let charging = if (raw & 0x80) != 0 {
                ChargingState::Charging
            } else {
                ChargingState::Discharging
            };
            let percent = raw & 0x7f;
            Ok(BatteryInfo {
                percent: Some(percent),
                charging: Some(charging),
                note: None,
            })
        } else {
            Err(DeviceError::Communication(format!(
                "想定外応答 ({n}B): {:02x?}",
                &buf[..n.min(8)]
            )))
        }
    }

    fn switch_profile(&self, api: &HidApi, slot: u8) -> Result<crate::model::ProfileResult, DeviceError> {
        if !(1..=2).contains(&slot) {
            return Err(DeviceError::Communication(
                "M6 8K のプロファイル番号は 1〜2 を指定してください".to_string(),
            ));
        }
        let (info, _) = Self::find_info(api).ok_or(DeviceError::Disconnected)?;
        let dev = info
            .open_device(api)
            .map_err(|e| DeviceError::Communication(format!("オープン失敗: {e}")))?;

        let mut pkt = [0u8; 64];
        pkt[0] = 0xb5; // Report ID 181
        pkt[1] = 0x0e;
        pkt[2] = 0x01;
        pkt[3] = slot;

        dev.write(&pkt)
            .map_err(|e| DeviceError::Communication(format!("write失敗: {e}")))?;

        let mut buf = [0u8; 64];
        let resp_info = match dev.read_timeout(&mut buf, 500) {
            Ok(n) if n > 0 => format!("resp({n}B)={:02x?}", &buf[..n.min(8)]),
            Ok(_) => "応答なし(送信完了)".to_string(),
            Err(e) => format!("readスキップ({e})"),
        };

        Ok(crate::model::ProfileResult {
            slot,
            message: format!("プロファイル {slot} へ切替送信 ({resp_info})"),
        })
    }
}