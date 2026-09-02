mod bindings {
    wit_bindgen::generate!({
        path: "../../wit/nur-cms-plugin",
        world: "cms-plugin",
    });
}

use bindings::exports::nur::cms::http_handler::{Guest, PluginError, Request, Response};
use bindings::nur::cms::types::Header;

struct VueAdminPlugin;

impl Guest for VueAdminPlugin {
    fn handle(request: Request) -> Result<Response, PluginError> {
        if request.route_id != "ping" {
            return Err(PluginError::NotFound);
        }

        Ok(Response {
            status: 200,
            headers: vec![Header {
                name: "content-type".into(),
                value: "text/plain; charset=utf-8".into(),
            }],
            body: b"Authenticated plugin request succeeded.".to_vec(),
        })
    }
}

bindings::export!(VueAdminPlugin with_types_in bindings);
