use crate::devices::driver::DeviceDriver;
use crate::model::{
    BatteryInfo, Capabilities, ConnectionMode, DeviceError, DeviceKind, DevicePresence,
};
use crate::protocols::via;
use hidapi::{DeviceInfo, HidApi};

const VID: u16 = 0x3434;
const PID_24G: u16 = 0xd026;

pub struct KeychronNapePro;

impl KeychronNapePro {
    fn find_info<'a>(api: &'a HidApi) -> Option<&'a DeviceInfo> {
        api.device_list().find(|d| {
            d.vendor_id() == VID
                && d.product_id() == PID_24G
                && d.interface_number() == 3
                && d.usage_page() == 0xff60
                && d.usage() == 0x0061
        })
    }
}

impl DeviceDriver for KeychronNapePro {
    fn id(&self) -> &'static str {
        "nape-pro"
    }

    fn name(&self) -> &'static str {
        "Keychron Nape Pro"
    }

    fn kind(&self) -> DeviceKind {
        DeviceKind::Trackball
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            battery: true,
            profile: false,
            profile_slots: None,
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

        let resp = via::send_raw_32(&dev, &[0xa7, 0x31], 500)
            .map_err(DeviceError::Communication)?;

        if resp.is_empty() {
            return Err(DeviceError::Communication(
                "無応答（スリープ中の可能性）".to_string(),
            ));
        }
        if resp.len() >= 3 && resp[0] == 0xa7 && resp[1] == 0x31 {
            Ok(BatteryInfo {
                percent: Some(resp[2]),
                charging: None,
                note: None,
            })
        } else {
            Err(DeviceError::Communication(format!(
                "想定外応答: {:02x?}",
                &resp[..resp.len().min(8)]
            )))
        }
    }
}