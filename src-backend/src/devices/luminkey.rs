use crate::devices::driver::DeviceDriver;
use crate::model::{Capabilities, ConnectionMode, DeviceKind, DevicePresence};
use hidapi::{DeviceInfo, HidApi};

const VID: u16 = 0x36b0;
const PID_WIRED: u16 = 0x3124;
const PID_24G: u16 = 0x3002;

pub struct LuminkeyNovaKine;

impl LuminkeyNovaKine {
    fn find_info<'a>(api: &'a HidApi) -> Option<(&'a DeviceInfo, ConnectionMode)> {
        // VIA 通信可能な有線接続を優先検出
        if let Some(wired) = api.device_list().find(|d| {
            d.vendor_id() == VID
                && d.product_id() == PID_WIRED
                && d.usage_page() == 0xff60
                && d.usage() == 0x0061
        }) {
            return Some((wired, ConnectionMode::Wired));
        }

        api.device_list()
            .find(|d| {
                d.vendor_id() == VID
                    && d.product_id() == PID_24G
                    && d.usage_page() == 0xff60
                    && d.usage() == 0x0061
            })
            .map(|d| (d, ConnectionMode::Wireless24G))
    }
}

impl DeviceDriver for LuminkeyNovaKine {
    fn id(&self) -> &'static str {
        "luminkey"
    }

    fn name(&self) -> &'static str {
        "LUMINKEY NOVA KINE"
    }

    fn kind(&self) -> DeviceKind {
        DeviceKind::Keyboard
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            battery: false,
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
}