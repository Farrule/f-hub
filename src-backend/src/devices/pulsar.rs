use crate::devices::driver::DeviceDriver;
use crate::model::{
    BatteryInfo, Capabilities, ChargingState, ConnectionMode, DeviceError, DeviceKind,
    DevicePresence,
};
use hidapi::{DeviceInfo, HidApi, HidDevice};
use std::thread;
use std::time::Duration;

const VID: u16 = 0x3710;
const PID_DONGLE: u16 = 0x5404;

pub struct PulsarFeinmannF01;

impl PulsarFeinmannF01 {
    fn find_info<'a>(api: &'a HidApi) -> Option<&'a DeviceInfo> {
        api.device_list().find(|d| {
            d.vendor_id() == VID
                && d.product_id() == PID_DONGLE
                && d.interface_number() == 3
                && d.usage_page() == 0xffff
        })
    }

    fn build_packet(header: &[u8]) -> [u8; 65] {
        let mut buf = [0u8; 65];
        buf[0] = 0x00;
        for (i, &b) in header.iter().enumerate() {
            buf[1 + i] = b;
        }
        let sum: u16 = buf[1..63].iter().map(|&b| b as u16).sum();
        let cs = sum.to_le_bytes();
        buf[63] = cs[0];
        buf[64] = cs[1];
        buf
    }

    fn query(dev: &HidDevice, cmd: &[u8]) -> Result<[u8; 65], DeviceError> {
        let req = Self::build_packet(cmd);
        dev.send_feature_report(&req)
            .map_err(|e| DeviceError::Communication(format!("SET_REPORT失敗: {e}")))?;

        for _ in 0..15 {
            thread::sleep(Duration::from_millis(35));
            let mut resp = [0u8; 65];
            resp[0] = 0x00;
            match dev.get_feature_report(&mut resp) {
                Ok(_) if resp[1] == 0x01 && resp[2..5] == cmd[1..4] => return Ok(resp),
                Ok(_) => continue,
                Err(e) => {
                    return Err(DeviceError::Communication(format!("GET_REPORT失敗: {e}")));
                }
            }
        }
        Err(DeviceError::Communication(
            "タイムアウト（スリープ中または未接続）".to_string(),
        ))
    }
}

impl DeviceDriver for PulsarFeinmannF01 {
    fn id(&self) -> &'static str {
        "pulsar"
    }

    fn name(&self) -> &'static str {
        "Pulsar Feinmann F01"
    }

    fn kind(&self) -> DeviceKind {
        DeviceKind::Mouse
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            battery: true,
            profile: true,
            profile_slots: Some((1, 6)),
            remap: true,
            wired_only_control: false,
        }
    }

    fn probe(&self, api: &HidApi) -> DevicePresence {
        match Self::find_info(api) {
            Some(info) => DevicePresence {
                mode: ConnectionMode::Wireless24G,
                vid: Some(info.vendor_id()),
                pid: Some(info.product_id()),
                product_name: info.product_string().map(ToOwned::to_owned),
            },
            None => DevicePresence::disconnected(),
        }
    }

    fn read_battery(&self, api: &HidApi) -> Result<BatteryInfo, DeviceError> {
        let info = Self::find_info(api).ok_or(DeviceError::Disconnected)?;
        let dev = info
            .open_device(api)
            .map_err(|e| DeviceError::Communication(format!("オープン失敗: {e}")))?;

        let bat_resp = Self::query(&dev, &[0x00, 0x08, 0x81, 0x01])?;
        let percent = bat_resp[7];

        let (charging, note) = match Self::query(&dev, &[0x00, 0x08, 0x83, 0x01]) {
            Ok(chg_resp) => match chg_resp[7] {
                0x00 => (Some(ChargingState::Discharging), None),
                0x01 => (Some(ChargingState::Charging), None),
                0x02 => (Some(ChargingState::FullOrIdle), Some("raw=0x02".to_string())),
                other => (Some(ChargingState::Unknown(other)), None),
            },
            Err(_) => (None, Some("充電状態取得スキップ".to_string())),
        };

        Ok(BatteryInfo {
            percent: Some(percent),
            charging,
            note,
        })
    }

    fn switch_profile(
        &self,
        api: &HidApi,
        slot: u8,
    ) -> Result<crate::model::ProfileResult, DeviceError> {
        if !(1..=6).contains(&slot) {
            return Err(DeviceError::Communication(
                "Pulsar のプロファイル番号は 1〜6 を指定してください".to_string(),
            ));
        }
        let info = Self::find_info(api).ok_or(DeviceError::Disconnected)?;
        let dev = info
            .open_device(api)
            .map_err(|e| DeviceError::Communication(format!("オープン失敗: {e}")))?;

        let resp = Self::query(&dev, &[0x00, 0x02, 0x06, 0x01, 0x00, 0x00, slot])?;
        Ok(crate::model::ProfileResult {
            slot,
            message: format!("プロファイル {slot} へ切替完了 (resp={:02x?})", &resp[..8]),
        })
    }
}