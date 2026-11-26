use crate::devices::driver::DeviceDriver;
use crate::model::{
    BatteryInfo, Capabilities, ChargingState, ConnectionMode, DeviceError, DeviceKind,
    DevicePresence,
};
use hidapi::{DeviceInfo, HidApi, HidDevice};

const VID: u16 = 0x388d;
const PID_WIRED: u16 = 0x0024;
const PID_24G: u16 = 0x0025;

pub struct LofreeHyzen;

impl LofreeHyzen {
    fn is_control_col(d: &DeviceInfo) -> bool {
        d.usage_page() == 0xff1c
            && d.usage() == 0x0092
            && d.path()
                .to_string_lossy()
                .to_ascii_lowercase()
                .contains("col04")
    }

    fn find_info<'a>(api: &'a HidApi) -> Option<(&'a DeviceInfo, ConnectionMode)> {
        if let Some(wired) = api
            .device_list()
            .find(|d| d.vendor_id() == VID && d.product_id() == PID_WIRED && Self::is_control_col(d))
        {
            return Some((wired, ConnectionMode::Wired));
        }

        api.device_list()
            .find(|d| d.vendor_id() == VID && d.product_id() == PID_24G && Self::is_control_col(d))
            .map(|d| (d, ConnectionMode::Wireless24G))
    }

    fn send_step(dev: &HidDevice, payload: &[u8]) -> Result<[u8; 64], DeviceError> {
        let mut pkt = [0u8; 64];
        pkt[0] = 0x04;
        pkt[1..1 + payload.len()].copy_from_slice(payload);

        dev.write(&pkt)
            .map_err(|e| DeviceError::Communication(format!("write失敗: {e}")))?;

        let mut resp = [0u8; 64];
        let n = dev
            .read_timeout(&mut resp, 500)
            .map_err(|e| DeviceError::Communication(format!("read失敗: {e}")))?;
        if n == 0 {
            return Err(DeviceError::Communication("無応答".to_string()));
        }
        Ok(resp)
    }
}

impl DeviceDriver for LofreeHyzen {
    fn id(&self) -> &'static str {
        "hyzen"
    }

    fn name(&self) -> &'static str {
        "Lofree Hyzen"
    }

    fn kind(&self) -> DeviceKind {
        DeviceKind::Keyboard
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
        let (info, mode) = Self::find_info(api).ok_or(DeviceError::Disconnected)?;
        let dev = info
            .open_device(api)
            .map_err(|e| DeviceError::Communication(format!("オープン失敗: {e}")))?;

        // Report 0x04 トランザクション: START -> ONLINE -> BATTERY -> END
        Self::send_step(&dev, &[0x00, 0x00, 0x01])?;

        let result = (|| {
            let online = Self::send_step(&dev, &[0x00, 0x00, 0xaa])?;
            if online[8] == 0x00 {
                return Err(DeviceError::Communication(
                    "オフライン/スリープ中".to_string(),
                ));
            }
            let bat = Self::send_step(&dev, &[0x00, 0x00, 0x1a])?;
            let percent = bat[8];
            let charging = if mode == ConnectionMode::Wired {
                Some(ChargingState::Charging)
            } else {
                None
            };
            Ok(BatteryInfo {
                percent: Some(percent),
                charging,
                note: None,
            })
        })();

        // 成否にかかわらず END を送ってトランザクションを閉じる
        let _ = Self::send_step(&dev, &[0x00, 0x00, 0x02]);
        result
    }

    fn switch_profile(&self, api: &HidApi, slot: u8) -> Result<crate::model::ProfileResult, DeviceError> {
        if slot > 6 {
            return Err(DeviceError::Communication(
                "Lofree Hyzen のプロファイル番号は 0〜6 を指定してください".to_string(),
            ));
        }
        let (info, _) = Self::find_info(api).ok_or(DeviceError::Disconnected)?;
        let dev = info
            .open_device(api)
            .map_err(|e| DeviceError::Communication(format!("オープン失敗: {e}")))?;

        Self::send_step(&dev, &[0x00, 0x00, 0x01])?;
        let res = Self::send_step(&dev, &[0x00, 0x00, 0x06, 0x01, 0x00, 0x00, 0x00, slot]);
        let _ = Self::send_step(&dev, &[0x00, 0x00, 0x02]);

        let resp = res?;
        Ok(crate::model::ProfileResult {
            slot,
            message: format!("プロファイル {slot} へ切替完了 (resp={:02x?})", &resp[..10]),
        })
    }
}