pub mod damage;
pub mod frame;
pub mod inspector;
pub mod keepalive;
pub mod nocd;
pub mod speed;

use crate::modules::harness::Harness;

pub fn harnesses() -> Vec<Harness> {
    vec![
        frame::harness(),
        speed::harness(),
        inspector::harness(),
        keepalive::harness(),
        damage::harness(),
        nocd::harness(),
    ]
}
