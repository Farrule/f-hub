pub mod driver;

mod lofree_hyzen;
mod luminkey;
mod m6_8k;
mod nape_pro;
mod pulsar;

use driver::DeviceDriver;

/// 登録済みデバイスドライバ一覧（新規デバイス追加時はここへ1行追加するだけ）
pub fn all_drivers() -> Vec<Box<dyn DeviceDriver>> {
    vec![
        Box::new(luminkey::LuminkeyNovaKine),
        Box::new(nape_pro::KeychronNapePro),
        Box::new(m6_8k::KeychronM68k),
        Box::new(lofree_hyzen::LofreeHyzen),
        Box::new(pulsar::PulsarFeinmannF01),
    ]
}

pub fn find_driver(id: &str) -> Option<Box<dyn DeviceDriver>> {
    all_drivers().into_iter().find(|d| d.id() == id)
}