use nur_core::utils::public_url::configured_public_url;

use super::HostState;
use crate::runtime::bindings;

impl bindings::nur::cms::configuration::Host for HostState {
    fn public_url(&mut self) -> Option<String> {
        configured_public_url()
    }
}
