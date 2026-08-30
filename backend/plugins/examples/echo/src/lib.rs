mod bindings {
    wit_bindgen::generate!({
        path: "../../wit/nur-cms-plugin",
        world: "cms-plugin",
    });
}

use bindings::{
    exports::nur::cms::http_handler::{Guest, PluginError, Request, Response},
    nur::cms::types::Header,
};

struct EchoPlugin;

impl Guest for EchoPlugin {
    fn handle(request: Request) -> Result<Response, PluginError> {
        let body = match request.route_id.as_str() {
            "echo" => request.body,
            "author" | "editor" => {
                let roles = request
                    .identity
                    .map(|identity| identity.roles.join(","))
                    .unwrap_or_default();
                format!("authorized roles: {roles}").into_bytes()
            }
            "root" => b"Hello from a nur-cms root plugin route".to_vec(),
            _ => return Err(PluginError::NotFound),
        };

        Ok(Response {
            status: 200,
            headers: vec![Header {
                name: "content-type".into(),
                value: "text/plain; charset=utf-8".into(),
            }],
            body,
        })
    }
}

bindings::export!(EchoPlugin with_types_in bindings);
